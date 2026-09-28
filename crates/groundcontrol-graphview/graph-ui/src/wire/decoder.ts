import { EdgeClass, EdgeData, GraphPayload, NodeData } from '../types.ts';

const WIRE_MAGIC = 0x43545856; // 'CTXV' in little-endian
const WIRE_MAGIC_LEGACY = 0x47564945; // 'GVIE' fallback

export function decodeBinaryGraph(buffer: ArrayBuffer): GraphPayload {
  const view = new DataView(buffer);
  if (buffer.byteLength < 16) {
    throw new Error('Buffer too small for GraphView header');
  }

  const magic = view.getUint32(0, true);
  if (magic !== WIRE_MAGIC && magic !== WIRE_MAGIC_LEGACY) {
    throw new Error(`Invalid wire magic: 0x${magic.toString(16)} (expected 0x${WIRE_MAGIC.toString(16)})`);
  }

  const version = view.getUint32(4, true);
  const nodeCount = view.getUint32(8, true);
  const edgeCount = view.getUint32(12, true);

  const edgeRecordSize = version >= 2 ? 16 : 12;
  const nodesOffset = 16;
  const edgesOffset = nodesOffset + nodeCount * 32;
  const stringTableOffset = edgesOffset + edgeCount * edgeRecordSize;

  if (buffer.byteLength < stringTableOffset + 4) {
    throw new Error('Buffer truncated before string table');
  }

  // 1. Decode String Table
  let strPtr = stringTableOffset;
  const stringsCount = view.getUint32(strPtr, true);
  strPtr += 4;

  const stringTable: string[] = [];
  const textDecoder = new TextDecoder('utf-8');

  for (let i = 0; i < stringsCount; i++) {
    if (strPtr + 2 > buffer.byteLength) break;
    const len = view.getUint16(strPtr, true);
    strPtr += 2;
    if (strPtr + len > buffer.byteLength) break;
    const bytes = new Uint8Array(buffer, strPtr, len);
    stringTable.push(textDecoder.decode(bytes));
    strPtr += len;
  }

  // 2. Decode Nodes
  const nodes: NodeData[] = new Array(nodeCount);
  let maxComm = 0;

  for (let i = 0; i < nodeCount; i++) {
    const offset = nodesOffset + i * 32;
    const id = view.getUint32(offset, true);
    const x = view.getFloat32(offset + 4, true);
    const y = view.getFloat32(offset + 8, true);
    const z = view.getFloat32(offset + 12, true);
    const community = view.getUint16(offset + 16, true);
    const degree = view.getUint16(offset + 18, true);
    const colorRgb = view.getUint32(offset + 20, true);
    const size = view.getFloat32(offset + 24, true);
    const typeIdx = view.getUint16(offset + 28, true);
    const pathIdx = view.getUint16(offset + 30, true);

    if (community > maxComm) maxComm = community;

    const path = stringTable[pathIdx] || '';
    const entityType = stringTable[typeIdx] || 'CodeSymbol';

    nodes[i] = {
      id,
      path,
      position: [x, y, z],
      degree,
      community,
      entityType,
      colorRgb,
      size,
    };
  }

  // 3. Decode Edges
  const edges: EdgeData[] = new Array(edgeCount);

  for (let i = 0; i < edgeCount; i++) {
    const offset = edgesOffset + i * edgeRecordSize;
    const source = view.getUint32(offset, true);
    const target = view.getUint32(offset + 4, true);
    const typeIdx = view.getUint16(offset + 8, true);
    const qw = view.getUint16(offset + 10, true);
    const weight = qw / 1000.0;

    let edgeClass = EdgeClass.Structural;
    let confidence = 0;

    if (version >= 2) {
      edgeClass = (view.getUint8(offset + 12) as EdgeClass) || EdgeClass.Structural;
      confidence = view.getUint8(offset + 13) || 0;
    }

    const edgeType = stringTable[typeIdx] || 'references';

    edges[i] = {
      source,
      target,
      edgeType,
      weight,
      edgeClass,
      confidence,
    };
  }

  return {
    corpus: 'current',
    nodes,
    edges,
    communitiesCount: maxComm + 1,
  };
}
