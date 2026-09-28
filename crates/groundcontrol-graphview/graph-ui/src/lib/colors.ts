import * as THREE from 'three';
import { EdgeClass } from '../types.ts';

export const EDGE_TYPE_COLORS: Record<string, number> = {
  // Code relations
  calls: 0x10b981,
  call: 0x10b981,
  defines: 0xa855f7,
  defines_method: 0xa855f7,
  imports: 0x38bdf8,
  import: 0x38bdf8,
  implements: 0xf97316,

  // Structural relations
  wikilink: 0x38bdf8,
  reference: 0x38bdf8,
  related: 0x06b6d4,
  frontmatter: 0x06b6d4,

  // Semantic relations
  similar_to: 0x8b5cf6,
  tag_similarity: 0x8b5cf6,

  // CrossModal relations
  documents: 0xf59e0b,
  tested_by: 0xf59e0b,
  specifies: 0xf59e0b,

  // Cross-corpus inter-repo links
  cross_corpus: 0xe2e8f0,
};

export const EDGE_CLASS_COLORS: Record<EdgeClass, number> = {
  [EdgeClass.Structural]: 0x38bdf8, // Cyan
  [EdgeClass.Semantic]: 0x8b5cf6,   // Violet
  [EdgeClass.Code]: 0x10b981,       // Emerald
  [EdgeClass.CrossModal]: 0xf59e0b, // Amber
  [EdgeClass.Hybrid]: 0x94a3b8,     // Slate Silver
};

export function getEdgeColor(edgeType: string, edgeClass: EdgeClass): number {
  const norm = edgeType.toLowerCase();
  if (norm in EDGE_TYPE_COLORS) {
    return EDGE_TYPE_COLORS[norm];
  }
  return EDGE_CLASS_COLORS[edgeClass] ?? 0x64748b;
}

export function stellarColorForDegree(degree: number): number {
  if (degree <= 1) return 0xb45309;       // Ember Red/Orange
  if (degree <= 3) return 0xd97706;       // Warm Amber
  if (degree <= 7) return 0xf59e0b;       // Bright Gold
  if (degree <= 15) return 0xfef08a;      // Solar Pale Yellow
  if (degree <= 30) return 0xf8fafc;      // Stellar Pure White
  if (degree <= 60) return 0x38bdf8;      // Electric Ice-Cyan
  return 0x818cf8;                        // High-Energy Plasma Indigo
}

const tempColor = new THREE.Color();

export function communityColor(commId: number): number {
  // Golden ratio angle in HSL color wheel guarantees smooth, maximally distinct separation
  const hue = ((Math.abs(commId) * 137.507764) % 360) / 360;
  tempColor.setHSL(hue, 0.70, 0.55);
  return tempColor.getHex();
}

export function hexToCss(colorHex: number): string {
  return '#' + colorHex.toString(16).padStart(6, '0');
}

export const CATEGORY_COLORS: Record<string, number> = {
  DocNode: 0x38bdf8,     // Ice Blue
  Function: 0x10b981,    // Emerald
  Struct: 0x8b5cf6,      // Violet
  Trait: 0xec4899,       // Rose Pink
  Enum: 0xf59e0b,        // Amber
  Module: 0x06b6d4,      // Teal Cyan
  Macro: 0x14b8a6,       // Aqua
  CodeSymbol: 0x64748b,  // Slate
};

export function getCategoryColor(cat: string): number {
  return CATEGORY_COLORS[cat] ?? 0x64748b;
}
