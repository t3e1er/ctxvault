import * as THREE from 'three';
import { AgentActivation, NodeData } from '../types.ts';
import { resolveAgentVisual } from '../lib/colors.ts';

interface ExpandingOrb {
  coreMesh: THREE.Mesh;
  coreMaterial: THREE.MeshBasicMaterial;
  auraMesh: THREE.Mesh;
  auraMaterial: THREE.MeshBasicMaterial;
  position: THREE.Vector3;
  progress: number;
  duration: number;
  startRadius: number;
  maxRadius: number;
  color: THREE.Color;
}

interface SparkParticle {
  position: THREE.Vector3;
  velocity: THREE.Vector3;
  life: number;
  maxLife: number;
  color: THREE.Color;
  size: number;
}

const MAX_TRAIL_PTS = 60;

interface TronTraversal {
  startPos: THREE.Vector3;
  endPos: THREE.Vector3;
  currentPos: THREE.Vector3;
  progress: number;
  duration: number;
  color: THREE.Color;
  agentName: string;
  cometMesh: THREE.Mesh;
  cometMaterial: THREE.MeshBasicMaterial;
  trailGeo: THREE.BufferGeometry;
  trailLine: THREE.Line;
  trailMat: THREE.LineBasicMaterial;
  trailPts: THREE.Vector3[];
  edgeTubeMesh: THREE.Mesh | null;
  edgeTubeMaterial: THREE.MeshBasicMaterial | null;
  edgeTubeFadeTimer: number;
  nextSteps: THREE.Vector3[];
}

export class ActivationEffects {
  public group: THREE.Group;
  public activationScale: number = 1.0;
  public onBloomSurge?: (magnitude: number) => void;

  private orbs: ExpandingOrb[] = [];
  private traversals: TronTraversal[] = [];
  private sparks: SparkParticle[] = [];

  private sparkPoints: THREE.Points | null = null;
  private sparkGeometry: THREE.BufferGeometry | null = null;
  private sparkMaterial: THREE.PointsMaterial | null = null;
  private readonly maxSparks = 5000;
  private sparkPositions: Float32Array;
  private sparkColors: Float32Array;

  private readonly sphereGeometry: THREE.SphereGeometry;

  constructor() {
    this.group = new THREE.Group();
    this.group.name = 'ActivationEffects';

    this.sphereGeometry = new THREE.SphereGeometry(1.0, 32, 24);

    this.sparkPositions = new Float32Array(this.maxSparks * 3);
    this.sparkColors = new Float32Array(this.maxSparks * 3);

    this.sparkGeometry = new THREE.BufferGeometry();
    this.sparkGeometry.setAttribute('position', new THREE.BufferAttribute(this.sparkPositions, 3));
    this.sparkGeometry.setAttribute('color', new THREE.BufferAttribute(this.sparkColors, 3));

    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 64;
    const ctx = canvas.getContext('2d')!;
    const grad = ctx.createRadialGradient(32, 32, 0, 32, 32, 32);
    grad.addColorStop(0.0, 'rgba(255,255,255,1.0)');
    grad.addColorStop(0.3, 'rgba(255,255,255,0.9)');
    grad.addColorStop(0.65, 'rgba(255,255,255,0.3)');
    grad.addColorStop(1.0, 'rgba(0,0,0,0)');
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, 64, 64);
    const texture = new THREE.CanvasTexture(canvas);

    this.sparkMaterial = new THREE.PointsMaterial({
      size: 16.0,
      map: texture,
      vertexColors: true,
      transparent: true,
      opacity: 0.95,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      sizeAttenuation: true,
    });

    this.sparkPoints = new THREE.Points(this.sparkGeometry, this.sparkMaterial);
    this.sparkPoints.renderOrder = 9999;
    this.group.add(this.sparkPoints);
  }

  public setActivationScale(scale: number) {
    this.activationScale = Math.max(0.2, scale);
    if (this.sparkMaterial) {
      this.sparkMaterial.size = 16.0 * this.activationScale;
    }
  }

  public triggerActivation(
    act: AgentActivation,
    nodes: NodeData[],
    fallbackPosition?: [number, number, number]
  ) {
    const visual = resolveAgentVisual(act);
    const color = new THREE.Color(visual.colorHex);
    const tool = act.tool.toLowerCase();

    const positions: THREE.Vector3[] = [];
    if (nodes.length > 0) {
      for (const n of nodes) {
        positions.push(new THREE.Vector3(n.position[0], n.position[1], n.position[2]));
      }
    } else if (fallbackPosition) {
      positions.push(new THREE.Vector3(fallbackPosition[0], fallbackPosition[1], fallbackPosition[2]));
    }
    if (positions.length === 0) return;

    if (tool.includes('graph_match') || tool === 'graph' || tool.includes('trace')) {
      this.triggerTronTraversal(positions, color, visual.name);
    } else if (tool.includes('read') || tool.includes('snippet')) {
      for (const pos of positions) {
        this.triggerScannerPulse(pos, color, visual.name);
      }
    } else if (tool.includes('write') || tool.includes('create') || tool.includes('append')) {
      for (const pos of positions) {
        this.triggerConstructiveBurst(pos, color, visual.name);
      }
    } else {
      for (const pos of positions.slice(0, 8)) {
        this.triggerSupernova(pos, color, visual.name);
      }
    }
  }

  public triggerSupernova(position: THREE.Vector3, color: THREE.Color, _agentName: string) {
    const s = this.activationScale;
    this.onBloomSurge?.(0.9 * s);

    // Primary slow 3D orb expanding and fading gracefully
    this.createExpandingOrb(position, color, 3.0 * s, 60.0 * s, 2.5);
    // Subtle secondary inner wave with slight offset
    setTimeout(() => {
      this.createExpandingOrb(position, color, 2.0 * s, 42.0 * s, 2.0);
    }, 160);
  }

  public triggerTronTraversal(positions: THREE.Vector3[], color: THREE.Color, agentName: string) {
    if (positions.length === 0) return;
    if (positions.length === 1) {
      this.triggerSupernova(positions[0], color, agentName);
      return;
    }

    const startPos = positions[0];
    const endPos = positions[1];
    const nextSteps = positions.slice(2);
    const s = this.activationScale;

    this.onBloomSurge?.(0.75 * s);
    this.createExpandingOrb(startPos, color, 2.5 * s, 36.0 * s, 1.8);

    const { tubeMesh, tubeMat } = this.buildEdgeTube(startPos, endPos, color, s);

    const cometMat = new THREE.MeshBasicMaterial({
      color: color.clone().multiplyScalar(4.5),
      transparent: true,
      opacity: 1.0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const cometMesh = new THREE.Mesh(this.sphereGeometry, cometMat);
    const headSize = 14.0 * s;
    cometMesh.scale.set(headSize, headSize, headSize * 2.8);
    cometMesh.position.copy(startPos);
    const initDir = new THREE.Vector3().subVectors(endPos, startPos);
    if (initDir.lengthSq() > 0.0001) {
      cometMesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), initDir.normalize());
    }
    cometMesh.renderOrder = 9999;
    this.group.add(cometMesh);

    const trailPosArr = new Float32Array(MAX_TRAIL_PTS * 3);
    const trailColArr = new Float32Array(MAX_TRAIL_PTS * 3);
    const trailGeo = new THREE.BufferGeometry();
    trailGeo.setAttribute('position', new THREE.BufferAttribute(trailPosArr, 3));
    trailGeo.setAttribute('color', new THREE.BufferAttribute(trailColArr, 3));
    trailGeo.setDrawRange(0, 0);
    const trailMat = new THREE.LineBasicMaterial({
      vertexColors: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      transparent: true,
      opacity: 0.92,
    });
    const trailLine = new THREE.Line(trailGeo, trailMat);
    trailLine.renderOrder = 9999;
    this.group.add(trailLine);

    const dist = startPos.distanceTo(endPos);
    const duration = Math.max(0.7, Math.min(2.8, dist / 750.0));

    this.traversals.push({
      startPos: startPos.clone(),
      endPos: endPos.clone(),
      currentPos: startPos.clone(),
      progress: 0,
      duration,
      color,
      agentName,
      cometMesh,
      cometMaterial: cometMat,
      trailGeo,
      trailLine,
      trailMat,
      trailPts: [],
      edgeTubeMesh: tubeMesh,
      edgeTubeMaterial: tubeMat,
      edgeTubeFadeTimer: -1,
      nextSteps,
    });
  }

  public triggerScannerPulse(position: THREE.Vector3, color: THREE.Color, _agentName: string) {
    const s = this.activationScale;
    this.onBloomSurge?.(0.6 * s);
    this.createExpandingOrb(position, color, 2.0 * s, 38.0 * s, 1.8);
    setTimeout(() => this.createExpandingOrb(position, color, 1.5 * s, 26.0 * s, 1.4), 180);
  }

  public triggerConstructiveBurst(position: THREE.Vector3, color: THREE.Color, _agentName: string) {
    const s = this.activationScale;
    this.onBloomSurge?.(0.8 * s);
    this.createExpandingOrb(position, color, 3.0 * s, 48.0 * s, 2.2);
    setTimeout(() => this.createExpandingOrb(position, color, 2.0 * s, 32.0 * s, 1.7), 160);
  }

  public update(dt: number, _camera?: THREE.Camera) {
    for (let i = this.orbs.length - 1; i >= 0; i--) {
      const orb = this.orbs[i];
      orb.progress += dt / orb.duration;
      if (orb.progress >= 1.0) {
        this.group.remove(orb.coreMesh);
        this.group.remove(orb.auraMesh);
        orb.coreMaterial.dispose();
        orb.auraMaterial.dispose();
        this.orbs.splice(i, 1);
        continue;
      }
      const ease = 1 - Math.pow(1 - orb.progress, 3);
      const r = orb.startRadius + (orb.maxRadius - orb.startRadius) * ease;
      const fade = Math.max(0, 1.0 - orb.progress);

      orb.auraMesh.scale.set(r, r, r);
      orb.auraMaterial.opacity = Math.pow(fade, 1.3) * 0.5;

      const coreR = Math.max(0.1, r * 0.42);
      orb.coreMesh.scale.set(coreR, coreR, coreR);
      orb.coreMaterial.opacity = Math.pow(fade, 1.8) * 0.85;
    }

    for (let i = this.traversals.length - 1; i >= 0; i--) {
      const tr = this.traversals[i];

      if (tr.edgeTubeFadeTimer >= 0) {
        tr.edgeTubeFadeTimer -= dt;
        if (tr.edgeTubeMaterial) {
          tr.edgeTubeMaterial.opacity = Math.max(0, tr.edgeTubeFadeTimer / 1.2) * 0.55;
        }
        if (tr.edgeTubeFadeTimer <= 0) {
          this.cleanupTraversal(i);
        }
        continue;
      }

      tr.progress += dt / tr.duration;
      const t = Math.min(1.0, tr.progress);
      const ease = t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
      tr.currentPos.lerpVectors(tr.startPos, tr.endPos, ease);

      const tDir = new THREE.Vector3().subVectors(tr.endPos, tr.startPos).normalize();
      tr.cometMesh.position.copy(tr.currentPos);
      if (tDir.lengthSq() > 0.0001) {
        tr.cometMesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), tDir);
      }

      tr.trailPts.unshift(tr.currentPos.clone());
      if (tr.trailPts.length > MAX_TRAIL_PTS) {
        tr.trailPts.length = MAX_TRAIL_PTS;
      }

      const posAttr = tr.trailGeo.getAttribute('position') as THREE.BufferAttribute;
      const colAttr = tr.trailGeo.getAttribute('color') as THREE.BufferAttribute;
      const n = tr.trailPts.length;
      for (let j = 0; j < n; j++) {
        const pt = tr.trailPts[j];
        posAttr.setXYZ(j, pt.x, pt.y, pt.z);
        const fade = 1.0 - j / MAX_TRAIL_PTS;
        const br = 3.5 * fade;
        colAttr.setXYZ(j, tr.color.r * br, tr.color.g * br, tr.color.b * br);
      }
      posAttr.needsUpdate = true;
      colAttr.needsUpdate = true;
      tr.trailGeo.setDrawRange(0, n);

      if (tr.edgeTubeMaterial) {
        tr.edgeTubeMaterial.opacity = ease * 0.5;
      }

      if (Math.random() < 0.25 && this.sparks.length < this.maxSparks) {
        const s = this.activationScale;
        this.sparks.push({
          position: tr.currentPos.clone(),
          velocity: new THREE.Vector3(
            (Math.random() - 0.5) * 3.0 * s,
            (Math.random() - 0.5) * 3.0 * s,
            (Math.random() - 0.5) * 3.0 * s
          ),
          life: 0,
          maxLife: 0.15 + Math.random() * 0.1,
          color: tr.color.clone().multiplyScalar(2.5),
          size: 5.0 * s,
        });
      }

      if (tr.progress >= 1.0) {
        this.triggerSupernova(tr.endPos, tr.color, tr.agentName);
        if (tr.nextSteps.length > 0) {
          this.advanceTraversalChain(tr);
        } else {
          tr.edgeTubeFadeTimer = 1.2;
          tr.cometMesh.visible = false;
          tr.trailPts = [];
          tr.trailGeo.setDrawRange(0, 0);
        }
      }
    }

    let active = 0;
    for (let i = this.sparks.length - 1; i >= 0; i--) {
      const sp = this.sparks[i];
      sp.life += dt;
      if (sp.life >= sp.maxLife) {
        this.sparks.splice(i, 1);
        continue;
      }
      sp.velocity.multiplyScalar(Math.max(0, 1.0 - 5.5 * dt));
      sp.position.addScaledVector(sp.velocity, dt);
      const fade = 1.0 - sp.life / sp.maxLife;
      this.sparkPositions[active * 3]     = sp.position.x;
      this.sparkPositions[active * 3 + 1] = sp.position.y;
      this.sparkPositions[active * 3 + 2] = sp.position.z;
      this.sparkColors[active * 3]         = sp.color.r * fade;
      this.sparkColors[active * 3 + 1]     = sp.color.g * fade;
      this.sparkColors[active * 3 + 2]     = sp.color.b * fade;
      active++;
    }

    for (let i = active; i < Math.min(active + 30, this.maxSparks); i++) {
      this.sparkPositions[i * 3] = 0; this.sparkPositions[i * 3 + 1] = 0; this.sparkPositions[i * 3 + 2] = 0;
      this.sparkColors[i * 3] = 0;    this.sparkColors[i * 3 + 1] = 0;    this.sparkColors[i * 3 + 2] = 0;
    }

    if (this.sparkGeometry) {
      (this.sparkGeometry.getAttribute('position') as THREE.BufferAttribute).needsUpdate = true;
      (this.sparkGeometry.getAttribute('color') as THREE.BufferAttribute).needsUpdate = true;
      this.sparkGeometry.setDrawRange(0, active);
    }
  }

  public clear() {
    for (const orb of this.orbs) {
      this.group.remove(orb.coreMesh);
      this.group.remove(orb.auraMesh);
      orb.coreMaterial.dispose();
      orb.auraMaterial.dispose();
    }
    this.orbs = [];
    for (let i = this.traversals.length - 1; i >= 0; i--) {
      this.cleanupTraversal(i);
    }
    this.sparks = [];
  }

  private createExpandingOrb(
    position: THREE.Vector3,
    color: THREE.Color,
    startRadius: number,
    maxRadius: number,
    duration: number
  ) {
    const coreMat = new THREE.MeshBasicMaterial({
      color: color.clone().multiplyScalar(3.2),
      transparent: true,
      opacity: 0.9,
      blending: THREE.AdditiveBlending,
      side: THREE.FrontSide,
      depthWrite: false,
    });
    const coreMesh = new THREE.Mesh(this.sphereGeometry, coreMat);
    coreMesh.renderOrder = 9999;
    coreMesh.position.copy(position);
    coreMesh.scale.setScalar(startRadius * 0.42);
    this.group.add(coreMesh);

    const auraMat = new THREE.MeshBasicMaterial({
      color: color.clone().multiplyScalar(1.6),
      transparent: true,
      opacity: 0.55,
      blending: THREE.AdditiveBlending,
      side: THREE.DoubleSide,
      depthWrite: false,
    });
    const auraMesh = new THREE.Mesh(this.sphereGeometry, auraMat);
    auraMesh.renderOrder = 9998;
    auraMesh.position.copy(position);
    auraMesh.scale.setScalar(startRadius);
    this.group.add(auraMesh);

    this.orbs.push({
      coreMesh,
      coreMaterial: coreMat,
      auraMesh,
      auraMaterial: auraMat,
      position: position.clone(),
      progress: 0,
      duration,
      startRadius,
      maxRadius,
      color,
    });
  }

  private buildEdgeTube(
    startPos: THREE.Vector3,
    endPos: THREE.Vector3,
    color: THREE.Color,
    s: number
  ): { tubeMesh: THREE.Mesh; tubeMat: THREE.MeshBasicMaterial } {
    const dir = new THREE.Vector3().subVectors(endPos, startPos);
    const dist = dir.length();
    const radius = Math.max(2.5, 6.0 * s);
    const tubeMat = new THREE.MeshBasicMaterial({
      color: color.clone().multiplyScalar(2.5),
      transparent: true,
      opacity: 0.0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      side: THREE.DoubleSide,
    });
    const tubeGeo = new THREE.CylinderGeometry(radius, radius, dist, 8, 1, true);
    const tubeMesh = new THREE.Mesh(tubeGeo, tubeMat);
    const mid = new THREE.Vector3().addVectors(startPos, endPos).multiplyScalar(0.5);
    tubeMesh.position.copy(mid);
    if (dist > 0.001) {
      tubeMesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), dir.normalize());
    }
    tubeMesh.renderOrder = 9998;
    this.group.add(tubeMesh);
    return { tubeMesh, tubeMat };
  }

  private advanceTraversalChain(tr: TronTraversal) {
    const nextEnd = tr.nextSteps[0];
    const remaining = tr.nextSteps.slice(1);
    const s = this.activationScale;
    if (tr.edgeTubeMesh) {
      this.group.remove(tr.edgeTubeMesh);
      tr.edgeTubeMesh.geometry.dispose();
    }
    tr.edgeTubeMaterial?.dispose();
    const { tubeMesh, tubeMat } = this.buildEdgeTube(tr.endPos, nextEnd, tr.color, s);
    tr.startPos.copy(tr.endPos);
    tr.endPos.copy(nextEnd);
    tr.currentPos.copy(tr.startPos);
    tr.progress = 0;
    tr.duration = Math.max(0.7, Math.min(2.8, tr.startPos.distanceTo(nextEnd) / 750.0));
    tr.nextSteps = remaining;
    tr.trailPts = [];
    tr.edgeTubeMesh = tubeMesh;
    tr.edgeTubeMaterial = tubeMat;
    tr.edgeTubeFadeTimer = -1;
    tr.cometMesh.visible = true;
  }

  private cleanupTraversal(index: number) {
    const tr = this.traversals[index];
    this.group.remove(tr.cometMesh);
    tr.cometMaterial.dispose();
    this.group.remove(tr.trailLine);
    tr.trailGeo.dispose();
    tr.trailMat.dispose();
    if (tr.edgeTubeMesh) {
      this.group.remove(tr.edgeTubeMesh);
      tr.edgeTubeMesh.geometry.dispose();
    }
    tr.edgeTubeMaterial?.dispose();
    this.traversals.splice(index, 1);
  }
}
