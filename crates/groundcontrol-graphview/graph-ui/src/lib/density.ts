export function calcEdgeOpacity(edgeCount: number): number {
  if (edgeCount <= 100) return 0.45;
  if (edgeCount <= 1000) return 0.28;
  if (edgeCount <= 10000) return 0.16;
  return Math.max(0.03, 0.50 / Math.log10(edgeCount + 10));
}

export function calcBloomStrength(nodeCount: number): { strength: number; radius: number; threshold: number } {
  if (nodeCount < 1000) {
    return { strength: 0.50, radius: 0.35, threshold: 0.45 };
  } else if (nodeCount < 10000) {
    return { strength: 0.40, radius: 0.28, threshold: 0.50 };
  } else {
    return { strength: 0.32, radius: 0.22, threshold: 0.55 };
  }
}
