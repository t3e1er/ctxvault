import * as THREE from 'three';
import { CorpusMetadata } from '../types.ts';

export class CorpusLabels {
  public group: THREE.Group;
  private sprites: THREE.Sprite[] = [];

  constructor() {
    this.group = new THREE.Group();
    this.group.name = 'CorpusLabels';
  }

  public setCorpora(corpora: CorpusMetadata[]) {
    this.clear();

    for (const corpus of corpora) {
      if (!corpus.center) continue;

      const texture = this.createLabelTexture(corpus.name, corpus.nodes, corpus.edges);
      const material = new THREE.SpriteMaterial({
        map: texture,
        transparent: true,
        depthTest: false,
        toneMapped: false,
      });

      const sprite = new THREE.Sprite(material);
      // Place above corpus centroid
      sprite.position.set(corpus.center[0], corpus.center[1] + 120.0, corpus.center[2]);
      sprite.scale.set(160, 48, 1);
      this.sprites.push(sprite);
      this.group.add(sprite);
    }
  }

  private createLabelTexture(name: string, nodes: number, edges: number): THREE.CanvasTexture {
    const canvas = document.createElement('canvas');
    canvas.width = 512;
    canvas.height = 160;
    const ctx = canvas.getContext('2d')!;

    // Rounded glowing glass pill
    ctx.fillStyle = 'rgba(10, 15, 29, 0.88)';
    ctx.strokeStyle = 'rgba(56, 189, 248, 0.7)';
    ctx.lineWidth = 4;

    const r = 24;
    ctx.beginPath();
    ctx.roundRect(8, 8, 496, 144, r);
    ctx.fill();
    ctx.stroke();

    // Title
    ctx.fillStyle = '#f8fafc';
    ctx.font = 'bold 36px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(name, 256, 56);

    // Subtitle: Node & Edge counts
    ctx.fillStyle = '#94a3b8';
    ctx.font = '500 24px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
    const sub = `${nodes.toLocaleString()} nodes · ${edges.toLocaleString()} edges`;
    ctx.fillText(sub, 256, 108);

    const texture = new THREE.CanvasTexture(canvas);
    texture.minFilter = THREE.LinearFilter;
    return texture;
  }

  public clear() {
    for (const sprite of this.sprites) {
      this.group.remove(sprite);
      sprite.material.map?.dispose();
      sprite.material.dispose();
    }
    this.sprites = [];
  }
}
