export function calcEdgeOpacity(edgeCount: number): number {
  if (edgeCount <= 100) return 0.65;
  if (edgeCount <= 1000) return 0.45;
  if (edgeCount <= 10000) return 0.28;
  return Math.max(0.05, 0.85 / Math.log10(edgeCount + 10));
}

export function calcBloomStrength(nodeCount: number): { strength: number; radius: number; threshold: number } {
  if (nodeCount < 1000) {
    return { strength: 1.4, radius: 0.5, threshold: 0.1 };
  } else if (nodeCount < 10000) {
    return { strength: 1.0, radius: 0.4, threshold: 0.15 };
  } else if (nodeCount < 50000) {
    return { strength: 0.75, radius: 0.3, threshold: 0.2 };
  } else {
    return { strength: 0.5, radius: 0.25, threshold: 0.25 };
  }
}
