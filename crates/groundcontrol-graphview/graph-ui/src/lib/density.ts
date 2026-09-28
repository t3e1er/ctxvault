export function calcEdgeOpacity(edgeCount: number): number {
  if (edgeCount <= 100) return 0.55;
  if (edgeCount <= 1000) return 0.35;
  if (edgeCount <= 10000) return 0.20;
  return Math.max(0.04, 0.65 / Math.log10(edgeCount + 10));
}

export function calcBloomStrength(nodeCount: number): { strength: number; radius: number; threshold: number } {
  if (nodeCount < 1000) {
    return { strength: 0.6, radius: 0.35, threshold: 0.40 };
  } else if (nodeCount < 10000) {
    return { strength: 0.45, radius: 0.30, threshold: 0.45 };
  } else {
    return { strength: 0.38, radius: 0.25, threshold: 0.48 };
  }
}
