import * as THREE from 'three';
import { CorpusMetadata } from '../types.ts';

export class CorpusLabels {
  public group: THREE.Group;
  private sprites: THREE.Sprite[] = [];
  private spritesByCorpus: Map<string, THREE.Sprite> = new Map();
  private sizeScale: number = 1.0;
  private brightness: number = 0.85;

  constructor() {
    this.group = new THREE.Group();
    this.group.name = 'CorpusLabels';
  }

  public setSize(scale: number) {
    this.sizeScale = scale;
    for (const sprite of this.sprites) {
      sprite.scale.set(160 * this.sizeScale, 48 * this.sizeScale, 1);
    }
  }

  public setBrightness(brightness: number) {
    this.brightness = Math.max(0.05, Math.min(1.0, brightness));
    for (const sprite of this.sprites) {
      const mat = sprite.material as THREE.SpriteMaterial;
      mat.opacity = this.brightness;
    }
  }

  public updatePositions(centers: Map<string, [number, number, number]>) {
    for (const [name, center] of centers.entries()) {
      const sprite = this.spritesByCorpus.get(name);
      if (sprite) {
        sprite.position.set(center[0], center[1], center[2]);
      }
    }
  }

  public setCorpora(corpora: CorpusMetadata[]) {
    this.clear();

    for (const corpus of corpora) {
      if (!corpus.center) continue;

      const texture = this.createLabelTexture(corpus.name, corpus.nodes, corpus.edges);
      const material = new THREE.SpriteMaterial({
        map: texture,
        transparent: true,
        opacity: this.brightness,
        depthTest: false,
        toneMapped: false,
      });

      const sprite = new THREE.Sprite(material);
      // Float at the exact gravity center (centroid) of the corpus
      sprite.position.set(corpus.center[0], corpus.center[1], corpus.center[2]);
      sprite.scale.set(160 * this.sizeScale, 48 * this.sizeScale, 1);

      this.sprites.push(sprite);
      this.spritesByCorpus.set(corpus.name, sprite);
      this.group.add(sprite);
    }
  }

  private createLabelTexture(name: string, nodes: number, edges: number): THREE.CanvasTexture {
    const canvas = document.createElement('canvas');
    canvas.width = 512;
    canvas.height = 160;
    const ctx = canvas.getContext('2d')!;

    // Transparent glowing glass pill floating in gravity center
    ctx.clearRect(0, 0, 512, 160);

    ctx.save();
    ctx.shadowColor = 'rgba(56, 189, 248, 0.65)';
    ctx.shadowBlur = 16;
    ctx.fillStyle = 'rgba(15, 23, 42, 0.32)';
    ctx.strokeStyle = 'rgba(56, 189, 248, 0.85)';
    ctx.lineWidth = 3.5;

    const r = 24;
    ctx.beginPath();
    ctx.roundRect(12, 12, 488, 136, r);
    ctx.fill();
    ctx.stroke();
    ctx.restore();

    // Subtle neon header accent line inside pill
    ctx.strokeStyle = 'rgba(56, 189, 248, 0.35)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(32, 84);
    ctx.lineTo(480, 84);
    ctx.stroke();

    // Title text: radiant cyan/white
    ctx.save();
    ctx.shadowColor = 'rgba(56, 189, 248, 0.8)';
    ctx.shadowBlur = 8;
    ctx.fillStyle = '#f8fafc';
    ctx.font = 'bold 36px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(name, 256, 52);
    ctx.restore();

    // Subtitle: Node & Edge counts
    ctx.fillStyle = '#94a3b8';
    ctx.font = '600 22px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    const sub = `${nodes.toLocaleString()} nodes · ${edges.toLocaleString()} edges`;
    ctx.fillText(sub, 256, 112);

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
    this.spritesByCorpus.clear();
  }
}
