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
    this.sizeScale = Math.max(0.2, scale);
    for (const sprite of this.sprites) {
      sprite.scale.set(380 * this.sizeScale, 115 * this.sizeScale, 1);
    }
  }

  public setBrightness(brightness: number) {
    this.brightness = Math.max(0.1, Math.min(1.0, brightness));
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
      const center = corpus.center || [0, 0, 0];
      const texture = this.createLabelTexture(corpus.name, corpus.nodes, corpus.edges);
      const material = new THREE.SpriteMaterial({
        map: texture,
        transparent: true,
        opacity: this.brightness,
        depthTest: false,
        depthWrite: false,
        toneMapped: false,
      });

      const sprite = new THREE.Sprite(material);
      sprite.renderOrder = 9999;
      // Float at the exact space center (center of sphere)
      sprite.position.set(center[0], center[1], center[2]);
      sprite.scale.set(380 * this.sizeScale, 115 * this.sizeScale, 1);

      this.sprites.push(sprite);
      this.spritesByCorpus.set(corpus.name, sprite);
      this.group.add(sprite);
    }
  }

  private createLabelTexture(name: string, nodes?: number, edges?: number): THREE.CanvasTexture {
    const canvas = document.createElement('canvas');
    canvas.width = 1024;
    canvas.height = 320;
    const ctx = canvas.getContext('2d')!;

    // Transparent glowing glass pill floating in space center
    ctx.clearRect(0, 0, 1024, 320);

    ctx.save();
    ctx.shadowColor = 'rgba(56, 189, 248, 0.85)';
    ctx.shadowBlur = 24;
    ctx.fillStyle = 'rgba(15, 23, 42, 0.75)';
    ctx.strokeStyle = 'rgba(56, 189, 248, 0.95)';
    ctx.lineWidth = 5.0;

    const r = 40;
    ctx.beginPath();
    ctx.roundRect(16, 16, 992, 288, r);
    ctx.fill();
    ctx.stroke();
    ctx.restore();

    // Subtle neon divider inside pill
    ctx.strokeStyle = 'rgba(56, 189, 248, 0.45)';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(48, 168);
    ctx.lineTo(976, 168);
    ctx.stroke();

    // Title text: radiant cyan/white
    ctx.save();
    ctx.shadowColor = 'rgba(56, 189, 248, 0.95)';
    ctx.shadowBlur = 14;
    ctx.fillStyle = '#ffffff';
    ctx.font = 'bold 64px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(name, 512, 98);
    ctx.restore();

    // Subtitle: Node & Edge counts
    ctx.fillStyle = '#38bdf8';
    ctx.font = '600 36px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    const sub = `${(nodes || 0).toLocaleString()} nodes · ${(edges || 0).toLocaleString()} edges`;
    ctx.fillText(sub, 512, 232);

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
