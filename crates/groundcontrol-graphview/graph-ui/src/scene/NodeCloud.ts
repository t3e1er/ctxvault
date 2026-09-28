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

  constructor() {
    this.group = new THREE.Group();
    this.group.name = 'NodeCloud';
  }

  public setNodes(nodes: NodeData[], mode: ViewMode = 'entity') {
    this.nodes = nodes;
    this.currentMode = mode;
    this.clear();

    const count = nodes.length;
    if (count === 0) return;

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
      toneMapped: false,
    });

    const mesh = new THREE.InstancedMesh(geometry, material, nodes.length);
    mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);

    for (let i = 0; i < nodes.length; i++) {
      const node = nodes[i];
      this.dummy.position.set(node.position[0], node.position[1], node.position[2]);
      const s = Math.max(1.2, node.size * 0.45 * this.particleScale);
      this.dummy.scale.set(s, s, s);
      this.dummy.updateMatrix();
      mesh.setMatrixAt(i, this.dummy.matrix);

      this.resolveColor(node, mode, this.color);
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
      positions[i * 3] = n.position[0];
      positions[i * 3 + 1] = n.position[1];
      positions[i * 3 + 2] = n.position[2];

      this.resolveColor(n, mode, this.color);
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
    this.updateScales();
  }

  public setEntityFilter(filter: string) {
    this.entityFilter = filter;
    this.refreshColors();
    this.updateScales();
  }

  public setParticleScale(scale: number) {
    this.particleScale = scale / 6.0;
    this.updateScales();
    if (this.pointsMesh) {
      (this.pointsMesh.material as THREE.PointsMaterial).size = 4.5 * this.particleScale;
    }
  }

  private updateScales() {
    if (!this.instancedMesh) return;
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
      this.dummy.position.set(node.position[0], node.position[1], node.position[2]);
      this.dummy.scale.set(s, s, s);
      this.dummy.updateMatrix();
      this.instancedMesh.setMatrixAt(i, this.dummy.matrix);
    }
    this.instancedMesh.instanceMatrix.needsUpdate = true;
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
