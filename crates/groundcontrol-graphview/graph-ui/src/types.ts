export type ViewMode = 'entity' | 'degree' | 'community';

export enum EdgeClass {
  Structural = 0,
  Semantic = 1,
  Code = 2,
  CrossModal = 3,
  Hybrid = 4,
}

export interface NodeData {
  id: number;
  path: string;
  title?: string;
  position: [number, number, number];
  degree: number;
  community: number;
  entityType: string;
  colorRgb: number;
  size: number;
  corpus?: string;
}

export interface EdgeData {
  source: number;
  target: number;
  edgeType: string;
  weight: number;
  edgeClass: EdgeClass;
  confidence: number;
}

export interface GraphPayload {
  corpus: string;
  nodes: NodeData[];
  edges: EdgeData[];
  communitiesCount: number;
}

export interface CorpusMetadata {
  name: string;
  nodes: number;
  edges: number;
  graph_mtime: number;
  center?: [number, number, number];
}

export interface AgentActivation {
  timestamp: number;
  tool: string;
  client_id?: string;
  client_name?: string;
  client_color?: string;
  corpus?: string;
  query?: string;
  paths: string[];
  duration_ms: number;
  success: boolean;
}
