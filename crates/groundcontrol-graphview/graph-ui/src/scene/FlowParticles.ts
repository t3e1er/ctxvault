import * as THREE from 'three';
import { EdgeClass, EdgeData, NodeData } from '../types.ts';
import { getEdgeColor } from '../lib/colors.ts';

interface Particle {
  source: [number, number, number];
  target: [number, number, number];
  progress: number;
  speed: number;
  color: THREE.Color;
}

export class FlowParticles {
  public group: THREE.Group;
  private points: THREE.Points | null = null;
  private particles: Particle[] = [];
  private positions: Float32Array = new Float32Array(0);
  private colors: Float32Array = new Float32Array(0);
  private maxParticles = 600;

  constructor() {
    this.group = new THREE.Group();
    this.group.name = 'FlowParticles';
  }

  public setEdges(nodes: NodeData[], edges: EdgeData[]) {
    this.clear();
    if (edges.length === 0 || nodes.length === 0) return;

    const nodePos = new Map<number, [number, number, number]>();
    for (const n of nodes) {
      nodePos.set(n.id, n.position);
    }

    // Select candidate edges for animated flow (prioritizing calls and cross_corpus)
    const candidates = edges.filter((e) => nodePos.has(e.source) && nodePos.has(e.target));
    if (candidates.length === 0) return;

    const particleCount = Math.min(this.maxParticles, Math.max(40, Math.floor(candidates.length * 0.15)));
    this.particles = new Array(particleCount);
    this.positions = new Float32Array(particleCount * 3);
    this.colors = new Float32Array(particleCount * 3);

    for (let i = 0; i < particleCount; i++) {
      const edge = candidates[Math.floor(Math.random() * candidates.length)];
      const p1 = nodePos.get(edge.source)!;
      const p2 = nodePos.get(edge.target)!;
      const hex = getEdgeColor(edge.edgeType, edge.edgeClass);
      const color = new THREE.Color(hex);

      this.particles[i] = {
        source: p1,
        target: p2,
        progress: Math.random(),
        speed: 0.15 + Math.random() * 0.35,
        color,
      };
    }

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(this.positions, 3));
    geometry.setAttribute('color', new THREE.BufferAttribute(this.colors, 3));

    // Glowing particle texture
    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 64;
    const ctx = canvas.getContext('2d')!;
    const grad = ctx.createRadialGradient(32, 32, 0, 32, 32, 32);
    grad.addColorStop(0, 'rgba(255, 255, 255, 1)');
    grad.addColorStop(0.25, 'rgba(255, 255, 255, 0.85)');
    grad.addColorStop(0.6, 'rgba(255, 255, 255, 0.25)');
    grad.addColorStop(1, 'rgba(0, 0, 0, 0)');
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, 64, 64);

    const texture = new THREE.CanvasTexture(canvas);

    const material = new THREE.PointsMaterial({
      size: 7.0,
      map: texture,
      vertexColors: true,
      transparent: true,
      opacity: 0.95,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    this.points = new THREE.Points(geometry, material);
    this.group.add(this.points);
  }

  public update(dt: number) {
    if (!this.points || this.particles.length === 0) return;

    for (let i = 0; i < this.particles.length; i++) {
      const p = this.particles[i];
      p.progress += p.speed * dt;
      if (p.progress > 1.0) {
        p.progress -= 1.0;
      }

      const t = p.progress;
      const x = p.source[0] + (p.target[0] - p.source[0]) * t;
      const y = p.source[1] + (p.target[1] - p.source[1]) * t;
      const z = p.source[2] + (p.target[2] - p.source[2]) * t;

      const idx = i * 3;
      this.positions[idx] = x;
      this.positions[idx + 1] = y;
      this.positions[idx + 2] = z;

      this.colors[idx] = p.color.r;
      this.colors[idx + 1] = p.color.g;
      this.colors[idx + 2] = p.color.b;
    }

    const posAttr = this.points.geometry.getAttribute('position') as THREE.BufferAttribute;
    const colAttr = this.points.geometry.getAttribute('color') as THREE.BufferAttribute;
    posAttr.needsUpdate = true;
    colAttr.needsUpdate = true;
  }

  private clear() {
    if (this.points) {
      this.group.remove(this.points);
      this.points.geometry.dispose();
      (this.points.material as THREE.Material).dispose();
      this.points = null;
    }
    this.particles = [];
  }
}
