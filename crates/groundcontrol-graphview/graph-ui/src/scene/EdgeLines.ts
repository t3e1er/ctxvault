import * as THREE from 'three';
import { EdgeClass, EdgeData, NodeData } from '../types.ts';
import { getEdgeColor } from '../lib/colors.ts';
import { calcEdgeOpacity } from '../lib/density.ts';

export class EdgeLines {
  public group: THREE.Group;
  private lineSegments: THREE.LineSegments | null = null;
  private allEdges: EdgeData[] = [];
  private nodePositions: Map<number, [number, number, number]> = new Map();
  private activeClasses: Set<EdgeClass> = new Set([
    EdgeClass.Structural,
    EdgeClass.Semantic,
    EdgeClass.Code,
    EdgeClass.CrossModal,
    EdgeClass.Hybrid,
  ]);
  private hiddenTypes: Set<string> = new Set();
  private customOpacity: number | null = null;
  private gradientContrast: boolean = true;

  constructor() {
    this.group = new THREE.Group();
    this.group.name = 'EdgeLines';
  }

  public setGradientContrast(enabled: boolean) {
    this.gradientContrast = enabled;
    this.rebuildGeometry();
  }

  public setOpacity(opacity: number) {
    this.customOpacity = opacity;
    if (this.lineSegments && this.lineSegments.material) {
      (this.lineSegments.material as THREE.LineBasicMaterial).opacity = opacity;
    }
  }

  public setData(nodes: NodeData[], edges: EdgeData[]) {
    this.allEdges = edges;
    this.nodePositions.clear();
    for (const node of nodes) {
      this.nodePositions.set(node.id, node.position);
    }
    this.rebuildGeometry();
  }

  public toggleClass(edgeClass: EdgeClass, enabled: boolean) {
    if (enabled) {
      this.activeClasses.add(edgeClass);
    } else {
      this.activeClasses.delete(edgeClass);
    }
    this.rebuildGeometry();
  }

  public isClassActive(edgeClass: EdgeClass): boolean {
    return this.activeClasses.has(edgeClass);
  }

  public toggleType(edgeType: string, enabled: boolean) {
    const norm = edgeType.toLowerCase();
    if (enabled) {
      this.hiddenTypes.delete(norm);
    } else {
      this.hiddenTypes.add(norm);
    }
    this.rebuildGeometry();
  }

  public isTypeActive(edgeType: string): boolean {
    return !this.hiddenTypes.has(edgeType.toLowerCase());
  }

  public updatePositions(nodePositions: Map<number, [number, number, number]>) {
    this.nodePositions = nodePositions;
    if (!this.lineSegments) return;
    const posAttr = this.lineSegments.geometry.getAttribute('position') as THREE.BufferAttribute;
    if (!posAttr) return;

    let writeIdx = 0;
    for (const edge of this.allEdges) {
      if (!this.activeClasses.has(edge.edgeClass)) continue;
      if (this.hiddenTypes.has(edge.edgeType.toLowerCase())) continue;
      if (nodePositions.has(edge.source) && nodePositions.has(edge.target)) {
        const p1 = nodePositions.get(edge.source)!;
        const p2 = nodePositions.get(edge.target)!;
        posAttr.setXYZ(writeIdx * 2, p1[0], p1[1], p1[2]);
        posAttr.setXYZ(writeIdx * 2 + 1, p2[0], p2[1], p2[2]);
        writeIdx++;
      }
    }
    posAttr.needsUpdate = true;
  }

  public rebuildGeometry() {
    if (this.lineSegments) {
      this.group.remove(this.lineSegments);
      this.lineSegments.geometry.dispose();
      (this.lineSegments.material as THREE.Material).dispose();
      this.lineSegments = null;
    }

    // Filter edges
    const visibleEdges: EdgeData[] = [];
    for (const edge of this.allEdges) {
      if (!this.activeClasses.has(edge.edgeClass)) continue;
      if (this.hiddenTypes.has(edge.edgeType.toLowerCase())) continue;
      if (this.nodePositions.has(edge.source) && this.nodePositions.has(edge.target)) {
        visibleEdges.push(edge);
      }
    }

    const count = visibleEdges.length;
    if (count === 0) return;

    const positions = new Float32Array(count * 6);
    const colors = new Float32Array(count * 6);
    const tempColor = new THREE.Color();

    for (let i = 0; i < count; i++) {
      const edge = visibleEdges[i];
      const p1 = this.nodePositions.get(edge.source)!;
      const p2 = this.nodePositions.get(edge.target)!;

      const idx = i * 6;
      positions[idx] = p1[0];
      positions[idx + 1] = p1[1];
      positions[idx + 2] = p1[2];
      positions[idx + 3] = p2[0];
      positions[idx + 4] = p2[1];
      positions[idx + 5] = p2[2];

      const hex = getEdgeColor(edge.edgeType, edge.edgeClass);
      tempColor.setHex(hex);
      // Calibrate edges so they are distinctly colored but visibly softer and dimmer than nodes
      tempColor.multiplyScalar(0.60);

      // Source vertex
      colors[idx] = tempColor.r;
      colors[idx + 1] = tempColor.g;
      colors[idx + 2] = tempColor.b;
      // Target vertex
      colors[idx + 3] = tempColor.r;
      colors[idx + 4] = tempColor.g;
      colors[idx + 5] = tempColor.b;
    }

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));

    const baseOpacity = this.customOpacity !== null ? this.customOpacity : calcEdgeOpacity(count);
    const opacity = this.gradientContrast ? Math.min(0.26, baseOpacity * 0.70) : Math.min(0.32, baseOpacity * 0.85);
    const material = new THREE.LineBasicMaterial({
      vertexColors: true,
      transparent: true,
      opacity,
      blending: THREE.NormalBlending,
      depthWrite: false,
    });

    this.lineSegments = new THREE.LineSegments(geometry, material);
    this.group.add(this.lineSegments);
  }
}
