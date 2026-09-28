import { EdgeClass } from '../types.ts';

export const EDGE_TYPE_COLORS: Record<string, number> = {
  // Code relations
  calls: 0x10b981,
  call: 0x10b981,
  defines: 0xa855f7,
  defines_method: 0xa855f7,
  imports: 0x3b82f6,
  import: 0x3b82f6,
  implements: 0xf97316,

  // Structural relations
  wikilink: 0x38bdf8,
  reference: 0x38bdf8,
  related: 0x67e8f9,
  frontmatter: 0x67e8f9,

  // Semantic relations
  similar_to: 0x8b5cf6,
  tag_similarity: 0x8b5cf6,

  // CrossModal relations
  documents: 0xf59e0b,
  tested_by: 0xf59e0b,
  specifies: 0xf59e0b,

  // Cross-corpus inter-repo links
  cross_corpus: 0xffffff,
};

export const EDGE_CLASS_COLORS: Record<EdgeClass, number> = {
  [EdgeClass.Structural]: 0x38bdf8, // Cyan
  [EdgeClass.Semantic]: 0x8b5cf6,   // Violet
  [EdgeClass.Code]: 0x10b981,       // Emerald
  [EdgeClass.CrossModal]: 0xf59e0b, // Amber
  [EdgeClass.Hybrid]: 0xe2e8f0,     // Silver
};

export function getEdgeColor(edgeType: string, edgeClass: EdgeClass): number {
  const norm = edgeType.toLowerCase();
  if (norm in EDGE_TYPE_COLORS) {
    return EDGE_TYPE_COLORS[norm];
  }
  return EDGE_CLASS_COLORS[edgeClass] ?? 0x64748b;
}

export function stellarColorForDegree(degree: number): number {
  if (degree <= 1) return 0xef4444;       // Class M: Red
  if (degree <= 4) return 0xf97316;       // Class K: Orange
  if (degree <= 9) return 0xfacc15;       // Class G: Yellow
  if (degree <= 19) return 0xf8fafc;      // Class F/A: White
  if (degree <= 49) return 0x67e8f9;      // Class B: Light Blue
  return 0x38bdf8;                        // Class O: High-energy Deep Cyan / Blue
}

const COMMUNITY_PALETTE: number[] = [
  0x38bdf8, 0x10b981, 0xa855f7, 0xf59e0b,
  0xec4899, 0x06b6d4, 0x14b8a6, 0x6366f1,
  0xe11d48, 0x84cc16, 0xeab308, 0xd946ef,
  0x0284c7, 0x059669, 0x7c3aed, 0xf97316,
];

export function communityColor(commId: number): number {
  return COMMUNITY_PALETTE[Math.abs(commId) % COMMUNITY_PALETTE.length];
}

export function hexToCss(colorHex: number): string {
  return '#' + colorHex.toString(16).padStart(6, '0');
}

export const CATEGORY_COLORS: Record<string, number> = {
  DocNode: 0x3b82f6,
  Function: 0x10b981,
  Struct: 0x8b5cf6,
  Trait: 0xec4899,
  Enum: 0xf59e0b,
  Module: 0x06b6d4,
  Macro: 0x14b8a6,
  CodeSymbol: 0x64748b,
};

export function getCategoryColor(cat: string): number {
  return CATEGORY_COLORS[cat] ?? 0x64748b;
}
