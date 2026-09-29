import * as THREE from 'three';
import { NodeData, ViewMode } from '../types.ts';
import { communityColor, stellarColorForDegree } from '../lib/colors.ts';

export class NodeCloud {
  public group: THREE.Group;
  private instancedMesh: THREE.InstancedMesh | null = null;
  private pointsMesh: THREE.Points | null = null;
  private nodes: NodeData[] = [];
  private dummy = new THREE.Object3D();
  private color = new THREE.Color();
  private currentMode: ViewMode = 'entity';
  private hoveredId: number | null = null;
  private selectedId: number | null = null;
  private particleScale: number = 1.0;
  private searchMatchIds: Set<number> | null = null;
  private entityFilter: string = 'all';
  private gradientContrast: boolean = true;

  private basePositions: Float32Array = new Float32Array(0);
  private currentPositions: Map<number, [number, number, number]> = new Map();
  private commCentroids: Map<number, { count: number; cx: number; cy: number; cz: number }> = new Map();

  constructor() {
    this.group = new THREE.Group();
    this.group.name = 'NodeCloud';
  }

  public setGradientContrast(enabled: boolean) {
    this.gradientContrast = enabled;
    if (this.nodes.length > 0) {
      this.setNodes(this.nodes, this.currentMode);
    }
  }

  public setNodes(nodes: NodeData[], mode: ViewMode = 'entity') {
    this.nodes = nodes;
    this.currentMode = mode;
    this.clear();

    const count = nodes.length;
    if (count === 0) return;

    this.basePositions = new Float32Array(count * 3);
    this.currentPositions.clear();
    this.commCentroids.clear();

    // 1. Record base positions & compute community centroids
    for (let i = 0; i < count; i++) {
      const n = nodes[i];
      this.basePositions[i * 3] = n.position[0];
      this.basePositions[i * 3 + 1] = n.position[1];
      this.basePositions[i * 3 + 2] = n.position[2];
      this.currentPositions.set(n.id, [n.position[0], n.position[1], n.position[2]]);

      const comm = n.community;
      const c = this.commCentroids.get(comm) || { count: 0, cx: 0, cy: 0, cz: 0 };
      c.count++;
      c.cx += n.position[0];
      c.cy += n.position[1];
      c.cz += n.position[2];
      this.commCentroids.set(comm, c);
    }

    // Average centroids
    for (const entry of this.commCentroids.values()) {
      if (entry.count > 0) {
        entry.cx /= entry.count;
        entry.cy /= entry.count;
        entry.cz /= entry.count;
      }
    }

    if (count <= 75000) {
      this.buildInstanced(nodes, mode);
    } else {
      this.buildPoints(nodes, mode);
    }
  }

  private clear() {
    if (this.instancedMesh) {
      this.group.remove(this.instancedMesh);
      this.instancedMesh.geometry.dispose();
      (this.instancedMesh.material as THREE.Material).dispose();
      this.instancedMesh = null;
    }
    if (this.pointsMesh) {
      this.group.remove(this.pointsMesh);
      this.pointsMesh.geometry.dispose();
      (this.pointsMesh.material as THREE.Material).dispose();
      this.pointsMesh = null;
    }
  }

  private buildInstanced(nodes: NodeData[], mode: ViewMode) {
    const geometry = new THREE.SphereGeometry(1.0, 8, 6);
    const material = new THREE.MeshBasicMaterial({
      color: 0xffffff,
      toneMapped: true,
    });

    const mesh = new THREE.InstancedMesh(geometry, material, nodes.length);
    mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);

    for (let i = 0; i < nodes.length; i++) {
      const node = nodes[i];
      const pos = this.currentPositions.get(node.id) || node.position;
      this.dummy.position.set(pos[0], pos[1], pos[2]);

      this.resolveColor(node, mode, this.color);

      if (this.gradientContrast) {
        const deg = node.degree || 1;
        const falloff = 1.0 / Math.pow(1.0 + Math.log10(deg), 0.55);
        this.color.multiplyScalar(0.45 + 0.55 * falloff);
        const s = Math.max(0.9, (node.size * 0.45 * this.particleScale) * (0.55 + 0.45 * falloff));
        this.dummy.scale.set(s, s, s);
      } else {
        const s = Math.max(1.2, node.size * 0.45 * this.particleScale);
        this.dummy.scale.set(s, s, s);
      }

      this.dummy.updateMatrix();
      mesh.setMatrixAt(i, this.dummy.matrix);
      mesh.setColorAt(i, this.color);
    }

    mesh.instanceMatrix.needsUpdate = true;
    if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;

    this.instancedMesh = mesh;
    this.group.add(mesh);
  }

  private buildPoints(nodes: NodeData[], mode: ViewMode) {
    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(nodes.length * 3);
    const colors = new Float32Array(nodes.length * 3);

    for (let i = 0; i < nodes.length; i++) {
      const n = nodes[i];
      const pos = this.currentPositions.get(n.id) || n.position;
      positions[i * 3] = pos[0];
      positions[i * 3 + 1] = pos[1];
      positions[i * 3 + 2] = pos[2];

      this.resolveColor(n, mode, this.color);

      if (this.gradientContrast) {
        const deg = n.degree || 1;
        const falloff = 1.0 / Math.pow(1.0 + Math.log10(deg), 0.55);
        this.color.multiplyScalar(0.45 + 0.55 * falloff);
      }

      colors[i * 3] = this.color.r;
      colors[i * 3 + 1] = this.color.g;
      colors[i * 3 + 2] = this.color.b;
    }

    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));

    const material = new THREE.PointsMaterial({
      size: 4.5 * this.particleScale,
      vertexColors: true,
      transparent: true,
      opacity: 0.88,
      sizeAttenuation: true,
    });

    this.pointsMesh = new THREE.Points(geometry, material);
    this.group.add(this.pointsMesh);
  }

  public updateClusterScales(
    clusterDistScale: number,
    nodeDispScale: number
  ): Map<number, [number, number, number]> {
    for (let i = 0; i < this.nodes.length; i++) {
      const node = this.nodes[i];
      const c = this.commCentroids.get(node.community) || { cx: 0, cy: 0, cz: 0 };
      const bx = this.basePositions[i * 3];
      const by = this.basePositions[i * 3 + 1];
      const bz = this.basePositions[i * 3 + 2];

      const dx = bx - c.cx;
      const dy = by - c.cy;
      const dz = bz - c.cz;

      const nx = c.cx * clusterDistScale + dx * nodeDispScale;
      const ny = c.cy * clusterDistScale + dy * nodeDispScale;
      const nz = c.cz * clusterDistScale + dz * nodeDispScale;

      this.currentPositions.set(node.id, [nx, ny, nz]);
    }

    this.updateScalesAndPositions();
    return this.currentPositions;
  }

  private resolveColor(node: NodeData, mode: ViewMode, outColor: THREE.Color) {
    // 1. Search filtering
    if (this.searchMatchIds !== null) {
      if (this.searchMatchIds.has(node.id)) {
        outColor.setHex(0xf59e0b); // Neon amber glow for search matches
        return;
      } else {
        outColor.setHex(0x131a28); // Deeply dimmed non-matches
        return;
      }
    }

    // 2. Entity filtering
    if (this.entityFilter !== 'all') {
      const ent = node.entityType || 'CodeSymbol';
      if (ent !== this.entityFilter) {
        outColor.setHex(0x131a28);
        return;
      }
    }

    // 3. Selection & Hover
    if (this.selectedId === node.id) {
      outColor.setHex(0xffffff); // Selected node glows white
      return;
    }
    if (this.hoveredId === node.id) {
      outColor.setHex(0x38bdf8); // Hovered node glows cyan
      return;
    }

    // 4. Base modes
    if (mode === 'entity') {
      outColor.setHex(node.colorRgb);
    } else if (mode === 'degree') {
      outColor.setHex(stellarColorForDegree(node.degree));
    } else if (mode === 'community') {
      outColor.setHex(communityColor(node.community));
    }
  }

  public setViewMode(mode: ViewMode) {
    this.currentMode = mode;
    this.refreshColors();
  }

  public setHoveredId(id: number | null) {
    if (this.hoveredId === id) return;
    this.hoveredId = id;
    this.refreshColors();
  }

  public setSelectedId(id: number | null) {
    if (this.selectedId === id) return;
    this.selectedId = id;
    this.refreshColors();
  }

  public setSearchMatches(matchIds: Set<number> | null) {
    this.searchMatchIds = matchIds;
    this.refreshColors();
    this.updateScalesAndPositions();
  }

  public setEntityFilter(filter: string) {
    this.entityFilter = filter;
    this.refreshColors();
    this.updateScalesAndPositions();
  }

  public setParticleScale(scale: number) {
    this.particleScale = scale / 6.0;
    this.updateScalesAndPositions();
    if (this.pointsMesh) {
      (this.pointsMesh.material as THREE.PointsMaterial).size = 4.5 * this.particleScale;
    }
  }

  private updateScalesAndPositions() {
    if (this.instancedMesh) {
      for (let i = 0; i < this.nodes.length; i++) {
        const node = this.nodes[i];
        let s = Math.max(1.2, node.size * 0.45 * this.particleScale);
        if (this.searchMatchIds !== null) {
          if (this.searchMatchIds.has(node.id)) {
            s *= 1.8;
          } else {
            s *= 0.4;
          }
        } else if (this.entityFilter !== 'all' && node.entityType !== this.entityFilter) {
          s *= 0.4;
        }
        const pos = this.currentPositions.get(node.id) || node.position;
        this.dummy.position.set(pos[0], pos[1], pos[2]);
        this.dummy.scale.set(s, s, s);
        this.dummy.updateMatrix();
        this.instancedMesh.setMatrixAt(i, this.dummy.matrix);
      }
      this.instancedMesh.instanceMatrix.needsUpdate = true;
    } else if (this.pointsMesh) {
      const posAttr = this.pointsMesh.geometry.getAttribute('position') as THREE.BufferAttribute;
      if (posAttr) {
        for (let i = 0; i < this.nodes.length; i++) {
          const pos = this.currentPositions.get(this.nodes[i].id) || this.nodes[i].position;
          posAttr.setXYZ(i, pos[0], pos[1], pos[2]);
        }
        posAttr.needsUpdate = true;
      }
    }
  }

  public refreshColors() {
    if (this.instancedMesh && this.instancedMesh.instanceColor) {
      for (let i = 0; i < this.nodes.length; i++) {
        this.resolveColor(this.nodes[i], this.currentMode, this.color);
        this.instancedMesh.setColorAt(i, this.color);
      }
      this.instancedMesh.instanceColor.needsUpdate = true;
    } else if (this.pointsMesh) {
      const colorAttr = this.pointsMesh.geometry.getAttribute('color') as THREE.BufferAttribute;
      if (colorAttr) {
        for (let i = 0; i < this.nodes.length; i++) {
          this.resolveColor(this.nodes[i], this.currentMode, this.color);
          colorAttr.setXYZ(i, this.color.r, this.color.g, this.color.b);
        }
        colorAttr.needsUpdate = true;
      }
    }
  }

  public raycast(raycaster: THREE.Raycaster): NodeData | null {
    if (this.instancedMesh) {
      const intersects = raycaster.intersectObject(this.instancedMesh);
      if (intersects.length > 0 && intersects[0].instanceId !== undefined) {
        return this.nodes[intersects[0].instanceId] || null;
      }
    } else if (this.pointsMesh) {
      const intersects = raycaster.intersectObject(this.pointsMesh);
      if (intersects.length > 0 && intersects[0].index !== undefined) {
        return this.nodes[intersects[0].index] || null;
      }
    }
    return null;
  }
}
