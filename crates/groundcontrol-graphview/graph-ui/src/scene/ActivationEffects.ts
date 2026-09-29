import * as THREE from 'three';
import { AgentActivation, NodeData } from '../types.ts';
import { resolveAgentVisual } from '../lib/colors.ts';

interface ActiveShockwave {
  mesh: THREE.Mesh;
  material: THREE.MeshBasicMaterial;
  position: THREE.Vector3;
  progress: number;
  duration: number;
  startRadius: number;
  maxRadius: number;
  color: THREE.Color;
  billboard: boolean;
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

  private shockwaves: ActiveShockwave[] = [];
  private traversals: TronTraversal[] = [];
  private sparks: SparkParticle[] = [];

  private sparkPoints: THREE.Points | null = null;
  private sparkGeometry: THREE.BufferGeometry | null = null;
  private sparkMaterial: THREE.PointsMaterial | null = null;
  private readonly maxSparks = 5000;
  private sparkPositions: Float32Array;
  private sparkColors: Float32Array;

  private readonly ringGeometry: THREE.RingGeometry;
  private readonly sphereGeometry: THREE.SphereGeometry;

  constructor() {
    this.group = new THREE.Group();
    this.group.name = 'ActivationEffects';

    this.ringGeometry = new THREE.RingGeometry(0.85, 1.0, 48);
    this.sphereGeometry = new THREE.SphereGeometry(1.0, 16, 12);

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
    this.onBloomSurge?.(1.8 * s);

    this.createShockwave(position, color, 5.0 * s, 70.0 * s, 0.55, false, Math.PI / 2, 0);

    const ringConfigs = [
      { delay: 0,   maxR: 230.0, dur: 2.9 },
      { delay: 90,  maxR: 185.0, dur: 2.6 },
      { delay: 180, maxR: 150.0, dur: 2.2 },
      { delay: 270, maxR: 110.0, dur: 1.8 },
    ] as const;

    for (const cfg of ringConfigs) {
      const spawnRing = () => {
        const rx = (Math.random() - 0.5) * Math.PI;
        const ry = (Math.random() - 0.5) * Math.PI;
        this.createShockwave(position, color, 10.0 * s, cfg.maxR * s, cfg.dur, false, rx, ry);
      };
      if (cfg.delay === 0) {
        spawnRing();
      } else {
        setTimeout(spawnRing, cfg.delay);
      }
    }

    const count = Math.min(200, Math.floor(160 * Math.max(0.5, s)));
    for (let i = 0; i < count; i++) {
      if (this.sparks.length >= this.maxSparks) break;
      const phi = Math.random() * Math.PI * 2;
      const theta = Math.acos(Math.random() * 2 - 1);
      const speed = (80.0 + Math.random() * 200.0) * s;
      this.sparks.push({
        position: position.clone(),
        velocity: new THREE.Vector3(
          Math.sin(theta) * Math.cos(phi) * speed,
          Math.sin(theta) * Math.sin(phi) * speed,
          Math.cos(theta) * speed
        ),
        life: 0,
        maxLife: 1.8 + Math.random() * 1.5,
        color: color.clone().multiplyScalar(2.0 + Math.random() * 2.5),
        size: (14.0 + Math.random() * 20.0) * s,
      });
    }
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

    this.onBloomSurge?.(1.1 * s);
    this.createShockwave(startPos, color, 8.0 * s, 140.0 * s, 2.0, false,
      (Math.random() - 0.5) * Math.PI, (Math.random() - 0.5) * Math.PI);
    this.createShockwave(startPos, color, 5.0 * s, 90.0 * s, 1.6, false,
      (Math.random() - 0.5) * Math.PI, (Math.random() - 0.5) * Math.PI);
    this.spawnSparkBurst(startPos, color, Math.floor(80 * Math.max(0.5, s)), 55, 140, 1.8);

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
    this.onBloomSurge?.(0.55 * s);
    this.createShockwave(position, color, 10.0 * s, 180.0 * s, 2.2, true);
    setTimeout(() => this.createShockwave(position, color, 10.0 * s, 150.0 * s, 2.0, true), 180);
    setTimeout(() => this.createShockwave(position, color, 10.0 * s, 120.0 * s, 1.8, true), 360);

    for (let i = 0; i < 35; i++) {
      if (this.sparks.length >= this.maxSparks) break;
      const spread = 12.0 * s;
      this.sparks.push({
        position: position.clone().add(new THREE.Vector3(
          (Math.random() - 0.5) * spread,
          (Math.random() - 0.5) * spread,
          (Math.random() - 0.5) * spread
        )),
        velocity: new THREE.Vector3(
          (Math.random() - 0.5) * 12.0,
          (45.0 + Math.random() * 85.0) * s,
          (Math.random() - 0.5) * 12.0
        ),
        life: 0,
        maxLife: 1.8 + Math.random() * 0.8,
        color: color.clone().multiplyScalar(2.6),
        size: (18.0 + Math.random() * 8.0) * s,
      });
    }
  }

  public triggerConstructiveBurst(position: THREE.Vector3, color: THREE.Color, _agentName: string) {
    const s = this.activationScale;
    this.onBloomSurge?.(0.7 * s);
    this.createShockwave(position, color, 8.0 * s, 190.0 * s, 2.2, false,
      (Math.random() - 0.5) * Math.PI, (Math.random() - 0.5) * Math.PI);
    for (let i = 0; i < 40; i++) {
      if (this.sparks.length >= this.maxSparks) break;
      const angle = Math.random() * Math.PI * 2;
      const r = (20.0 + Math.random() * 40.0) * s;
      this.sparks.push({
        position: position.clone(),
        velocity: new THREE.Vector3(
          Math.cos(angle) * r,
          (30.0 + Math.random() * 50.0) * s,
          Math.sin(angle) * r
        ),
        life: 0,
        maxLife: 1.8,
        color: color.clone().multiplyScalar(2.8),
        size: (18.0 + Math.random() * 8.0) * s,
      });
    }
  }

  public update(dt: number, camera?: THREE.Camera) {
    for (let i = this.shockwaves.length - 1; i >= 0; i--) {
      const sw = this.shockwaves[i];
      sw.progress += dt / sw.duration;
      if (sw.progress >= 1.0) {
        this.group.remove(sw.mesh);
        sw.material.dispose();
        this.shockwaves.splice(i, 1);
        continue;
      }
      const ease = 1 - Math.pow(1 - sw.progress, 3);
      const r = sw.startRadius + (sw.maxRadius - sw.startRadius) * ease;
      sw.mesh.scale.set(r, r, r);
      sw.material.opacity = (1.0 - ease) * 0.95;
      if (sw.billboard && camera) {
        sw.mesh.quaternion.copy(camera.quaternion);
      }
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

      if (Math.random() < 0.45 && this.sparks.length < this.maxSparks) {
        const s = this.activationScale;
        this.sparks.push({
          position: tr.currentPos.clone(),
          velocity: new THREE.Vector3(
            (Math.random() - 0.5) * 18.0 * s,
            (Math.random() - 0.5) * 18.0 * s,
            (Math.random() - 0.5) * 18.0 * s
          ),
          life: 0,
          maxLife: 0.35 + Math.random() * 0.3,
          color: tr.color.clone().multiplyScalar(2.8),
          size: 9.0 * s,
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
      sp.velocity.multiplyScalar(Math.max(0, 1.0 - 2.4 * dt));
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
    for (const sw of this.shockwaves) {
      this.group.remove(sw.mesh);
      sw.material.dispose();
    }
    this.shockwaves = [];
    for (let i = this.traversals.length - 1; i >= 0; i--) {
      this.cleanupTraversal(i);
    }
    this.sparks = [];
  }

  private spawnSparkBurst(
    position: THREE.Vector3,
    color: THREE.Color,
    count: number,
    minSpeed: number,
    maxSpeed: number,
    maxLife: number
  ) {
    const s = this.activationScale;
    for (let i = 0; i < count; i++) {
      if (this.sparks.length >= this.maxSparks) break;
      const phi = Math.random() * Math.PI * 2;
      const theta = Math.acos(Math.random() * 2 - 1);
      const speed = minSpeed + Math.random() * (maxSpeed - minSpeed);
      this.sparks.push({
        position: position.clone(),
        velocity: new THREE.Vector3(
          Math.sin(theta) * Math.cos(phi) * speed,
          Math.sin(theta) * Math.sin(phi) * speed,
          Math.cos(theta) * speed
        ),
        life: 0,
        maxLife: maxLife * (0.65 + Math.random() * 0.7),
        color: color.clone().multiplyScalar(2.0 + Math.random() * 1.8),
        size: (13.0 + Math.random() * 13.0) * s,
      });
    }
  }

  private createShockwave(
    position: THREE.Vector3,
    color: THREE.Color,
    startRadius: number,
    maxRadius: number,
    duration: number,
    billboard: boolean,
    rotX: number = Math.PI / 2,
    rotY: number = 0
  ) {
    const mat = new THREE.MeshBasicMaterial({
      color: color.clone().multiplyScalar(2.8),
      transparent: true,
      opacity: 0.95,
      blending: THREE.AdditiveBlending,
      side: THREE.DoubleSide,
      depthWrite: false,
    });
    const mesh = new THREE.Mesh(this.ringGeometry, mat);
    mesh.renderOrder = 9999;
    mesh.position.copy(position);
    mesh.scale.setScalar(startRadius);
    mesh.rotation.set(rotX, rotY, 0);
    this.group.add(mesh);
    this.shockwaves.push({
      mesh, material: mat, position: position.clone(),
      progress: 0, duration, startRadius, maxRadius, color, billboard,
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
