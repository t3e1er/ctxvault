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
}

interface SparkParticle {
  position: THREE.Vector3;
  velocity: THREE.Vector3;
  life: number;
  maxLife: number;
  color: THREE.Color;
  size: number;
}

interface ActiveTraversal {
  startPos: THREE.Vector3;
  endPos: THREE.Vector3;
  currentPos: THREE.Vector3;
  progress: number;
  duration: number;
  color: THREE.Color;
  agentName: string;
  tracerMesh: THREE.Mesh;
  tracerMaterial: THREE.MeshBasicMaterial;
  trailLine: THREE.Line;
  nextSteps: THREE.Vector3[];
}

export class ActivationEffects {
  public group: THREE.Group;
  private shockwaves: ActiveShockwave[] = [];
  private traversals: ActiveTraversal[] = [];
  private sparks: SparkParticle[] = [];

  // Reusable spark point cloud
  private sparkPoints: THREE.Points | null = null;
  private sparkGeometry: THREE.BufferGeometry | null = null;
  private sparkMaterial: THREE.PointsMaterial | null = null;
  private maxSparks = 2000;
  private sparkPositions: Float32Array;
  private sparkColors: Float32Array;

  // Shared geometries
  private ringGeometry: THREE.RingGeometry;
  private sphereGeometry: THREE.SphereGeometry;

  constructor() {
    this.group = new THREE.Group();
    this.group.name = 'ActivationEffects';

    this.ringGeometry = new THREE.RingGeometry(0.85, 1.0, 36);
    this.sphereGeometry = new THREE.SphereGeometry(1.0, 16, 12);

    // Initialize particle pool
    this.sparkPositions = new Float32Array(this.maxSparks * 3);
    this.sparkColors = new Float32Array(this.maxSparks * 3);

    this.sparkGeometry = new THREE.BufferGeometry();
    this.sparkGeometry.setAttribute('position', new THREE.BufferAttribute(this.sparkPositions, 3));
    this.sparkGeometry.setAttribute('color', new THREE.BufferAttribute(this.sparkColors, 3));

    // Glow canvas texture for sparks
    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 64;
    const ctx = canvas.getContext('2d')!;
    const grad = ctx.createRadialGradient(32, 32, 0, 32, 32, 32);
    grad.addColorStop(0, 'rgba(255, 255, 255, 1)');
    grad.addColorStop(0.3, 'rgba(255, 255, 255, 0.9)');
    grad.addColorStop(0.65, 'rgba(255, 255, 255, 0.3)');
    grad.addColorStop(1, 'rgba(0, 0, 0, 0)');
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, 64, 64);
    const texture = new THREE.CanvasTexture(canvas);

    this.sparkMaterial = new THREE.PointsMaterial({
      size: 9.0,
      map: texture,
      vertexColors: true,
      transparent: true,
      opacity: 0.95,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    this.sparkPoints = new THREE.Points(this.sparkGeometry, this.sparkMaterial);
    this.group.add(this.sparkPoints);
  }

  /**
   * Main dispatch: trigger appropriate animation based on tool type and agent profile.
   */
  public triggerActivation(
    act: AgentActivation,
    nodes: NodeData[],
    fallbackPosition?: [number, number, number]
  ) {
    const visual = resolveAgentVisual(act);
    const color = new THREE.Color(visual.colorHex);
    const tool = act.tool.toLowerCase();

    // Collect 3D positions of all involved nodes
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
      // 1. Graph Traversal: pulse on start, edge glow traversal to target nodes, impact pulses
      this.triggerTraversalSequence(positions, color, visual.name);
    } else if (tool.includes('read') || tool.includes('snippet')) {
      // 2. Read / Inspect: focused scanner pulse with concentric ripples & vertical beacon
      for (const pos of positions) {
        this.triggerScannerPulse(pos, color, visual.name);
      }
    } else if (tool.includes('write') || tool.includes('create') || tool.includes('append')) {
      // 3. Write / Crystallize: constructive pulse burst + sparkling fountain
      for (const pos of positions) {
        this.triggerConstructiveBurst(pos, color, visual.name);
      }
    } else {
      // 4. Search Match / Default: explosive pulse burst + radial sparks
      for (const pos of positions.slice(0, 8)) {
        this.triggerMatchExplosion(pos, color, visual.name);
      }
    }
  }

  /**
   * Search Match Explosion: shockwave halo + sparkling velocity explosion.
   */
  public triggerMatchExplosion(position: THREE.Vector3, color: THREE.Color, agentName: string) {
    // 1. Expanding shockwave ring
    this.createShockwave(position, color, 4.0, 70.0, 1.2);

    // 2. High-energy wireframe sphere pulse
    this.createSpherePulse(position, color, 3.0, 45.0, 0.9);

    // 3. Particle sparks explosion (30 particles)
    const count = 30;
    for (let i = 0; i < count; i++) {
      if (this.sparks.length >= this.maxSparks) break;
      const phi = Math.random() * Math.PI * 2;
      const theta = Math.acos(Math.random() * 2 - 1);
      const speed = 40.0 + Math.random() * 90.0;

      const vel = new THREE.Vector3(
        Math.sin(theta) * Math.cos(phi) * speed,
        Math.sin(theta) * Math.sin(phi) * speed,
        Math.cos(theta) * speed
      );

      this.sparks.push({
        position: position.clone(),
        velocity: vel,
        life: 0,
        maxLife: 0.9 + Math.random() * 0.6,
        color: color.clone().multiplyScalar(1.8),
        size: 7.0 + Math.random() * 5.0,
      });
    }
  }

  /**
   * Graph Traversal: start node pulse -> traveling glowing tracer beam along edge -> finish node pulse.
   */
  public triggerTraversalSequence(positions: THREE.Vector3[], color: THREE.Color, agentName: string) {
    if (positions.length === 0) return;

    if (positions.length === 1) {
      // Single node traversal match: intense pulse
      this.triggerMatchExplosion(positions[0], color, agentName);
      return;
    }

    const startPos = positions[0];
    const rest = positions.slice(1);

    // Initial pulse on start node
    this.createShockwave(startPos, color, 3.0, 40.0, 0.8);
    this.createSpherePulse(startPos, color, 2.0, 25.0, 0.7);

    // Create traversal tracer from startPos to rest[0]
    const nextTarget = rest[0];
    const remainingSteps = rest.slice(1);

    const tracerMat = new THREE.MeshBasicMaterial({
      color: color.clone().multiplyScalar(2.5),
      transparent: true,
      opacity: 1.0,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const tracerMesh = new THREE.Mesh(this.sphereGeometry, tracerMat);
    tracerMesh.scale.set(4.5, 4.5, 4.5);
    tracerMesh.position.copy(startPos);
    this.group.add(tracerMesh);

    // Glowing trail line geometry
    const lineGeo = new THREE.BufferGeometry().setFromPoints([startPos.clone(), startPos.clone()]);
    const lineMat = new THREE.LineBasicMaterial({
      color: color.clone().multiplyScalar(2.0),
      transparent: true,
      opacity: 0.95,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      linewidth: 2,
    });
    const trailLine = new THREE.Line(lineGeo, lineMat);
    this.group.add(trailLine);

    this.traversals.push({
      startPos: startPos.clone(),
      endPos: nextTarget.clone(),
      currentPos: startPos.clone(),
      progress: 0,
      duration: 0.85,
      color,
      agentName,
      tracerMesh,
      tracerMaterial: tracerMat,
      trailLine,
      nextSteps: remainingSteps,
    });
  }

  /**
   * Read Scanner Pulse: 3 concentric ripples expanding like sonar waves + vertical beacon flare.
   */
  public triggerScannerPulse(position: THREE.Vector3, color: THREE.Color, agentName: string) {
    // 3 successive concentric shockwaves
    this.createShockwave(position, color, 3.0, 55.0, 1.3);
    setTimeout(() => {
      this.createShockwave(position, color, 3.0, 50.0, 1.2);
    }, 180);
    setTimeout(() => {
      this.createShockwave(position, color, 3.0, 45.0, 1.1);
    }, 360);

    // Vertical beacon column (sparks floating upward)
    for (let i = 0; i < 20; i++) {
      if (this.sparks.length >= this.maxSparks) break;
      const spread = 8.0;
      const p = position.clone().add(
        new THREE.Vector3(
          (Math.random() - 0.5) * spread,
          (Math.random() - 0.5) * spread,
          (Math.random() - 0.5) * spread
        )
      );
      const vel = new THREE.Vector3(
        (Math.random() - 0.5) * 8.0,
        35.0 + Math.random() * 45.0,
        (Math.random() - 0.5) * 8.0
      );

      this.sparks.push({
        position: p,
        velocity: vel,
        life: 0,
        maxLife: 1.1 + Math.random() * 0.4,
        color: color.clone().multiplyScalar(2.0),
        size: 8.0,
      });
    }
  }

  /**
   * Constructive burst for writes / note updates.
   */
  public triggerConstructiveBurst(position: THREE.Vector3, color: THREE.Color, agentName: string) {
    this.createShockwave(position, color, 2.0, 60.0, 1.4);
    for (let i = 0; i < 24; i++) {
      if (this.sparks.length >= this.maxSparks) break;
      const angle = Math.random() * Math.PI * 2;
      const r = 15.0 + Math.random() * 25.0;
      const vel = new THREE.Vector3(
        Math.cos(angle) * r,
        20.0 + Math.random() * 30.0,
        Math.sin(angle) * r
      );
      this.sparks.push({
        position: position.clone(),
        velocity: vel,
        life: 0,
        maxLife: 1.2,
        color: color.clone().multiplyScalar(2.2),
        size: 7.5,
      });
    }
  }

  private createShockwave(
    position: THREE.Vector3,
    color: THREE.Color,
    startRadius: number,
    maxRadius: number,
    duration: number
  ) {
    const mat = new THREE.MeshBasicMaterial({
      color: color.clone().multiplyScalar(2.0),
      transparent: true,
      opacity: 0.95,
      blending: THREE.AdditiveBlending,
      side: THREE.DoubleSide,
      depthWrite: false,
    });
    const mesh = new THREE.Mesh(this.ringGeometry, mat);
    mesh.position.copy(position);
    mesh.scale.set(startRadius, startRadius, startRadius);
    // Orient ring horizontally with slight dynamic tilt
    mesh.rotation.x = Math.PI / 2;
    this.group.add(mesh);

    this.shockwaves.push({
      mesh,
      material: mat,
      position: position.clone(),
      progress: 0,
      duration,
      startRadius,
      maxRadius,
      color,
    });
  }

  private createSpherePulse(
    position: THREE.Vector3,
    color: THREE.Color,
    startRadius: number,
    maxRadius: number,
    duration: number
  ) {
    const mat = new THREE.MeshBasicMaterial({
      color: color.clone().multiplyScalar(1.6),
      transparent: true,
      opacity: 0.85,
      wireframe: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const mesh = new THREE.Mesh(this.sphereGeometry, mat);
    mesh.position.copy(position);
    mesh.scale.set(startRadius, startRadius, startRadius);
    this.group.add(mesh);

    this.shockwaves.push({
      mesh,
      material: mat,
      position: position.clone(),
      progress: 0,
      duration,
      startRadius,
      maxRadius,
      color,
    });
  }

  /**
   * 60 FPS update loop.
   */
  public update(dt: number, camera?: THREE.Camera) {
    // 1. Update Shockwaves
    for (let i = this.shockwaves.length - 1; i >= 0; i--) {
      const sw = this.shockwaves[i];
      sw.progress += dt / sw.duration;

      if (sw.progress >= 1.0) {
        this.group.remove(sw.mesh);
        sw.mesh.geometry.dispose();
        sw.material.dispose();
        this.shockwaves.splice(i, 1);
        continue;
      }

      // Smooth easeOutCubic expansion
      const t = sw.progress;
      const ease = 1 - Math.pow(1 - t, 3);
      const r = sw.startRadius + (sw.maxRadius - sw.startRadius) * ease;
      sw.mesh.scale.set(r, r, r);

      // Fade out opacity
      sw.material.opacity = (1.0 - ease) * 0.95;

      // Billboarding towards camera if ring
      if (camera && sw.mesh.geometry === this.ringGeometry) {
        sw.mesh.quaternion.copy(camera.quaternion);
      }
    }

    // 2. Update Active Traversals
    for (let i = this.traversals.length - 1; i >= 0; i--) {
      const tr = this.traversals[i];
      tr.progress += dt / tr.duration;

      const t = Math.min(1.0, tr.progress);
      // Smooth step
      const ease = t * t * (3 - 2 * t);
      tr.currentPos.lerpVectors(tr.startPos, tr.endPos, ease);
      tr.tracerMesh.position.copy(tr.currentPos);

      // Update trailing glow line
      const linePosAttr = tr.trailLine.geometry.getAttribute('position') as THREE.BufferAttribute;
      linePosAttr.setXYZ(0, tr.startPos.x, tr.startPos.y, tr.startPos.z);
      linePosAttr.setXYZ(1, tr.currentPos.x, tr.currentPos.y, tr.currentPos.z);
      linePosAttr.needsUpdate = true;

      // Spawn spark dust along trajectory
      if (Math.random() < 0.4 && this.sparks.length < this.maxSparks) {
        this.sparks.push({
          position: tr.currentPos.clone(),
          velocity: new THREE.Vector3(
            (Math.random() - 0.5) * 8.0,
            (Math.random() - 0.5) * 8.0,
            (Math.random() - 0.5) * 8.0
          ),
          life: 0,
          maxLife: 0.5,
          color: tr.color.clone().multiplyScalar(2.0),
          size: 6.0,
        });
      }

      if (tr.progress >= 1.0) {
        // Destination reached: impact explosion on destination node!
        this.triggerMatchExplosion(tr.endPos, tr.color, tr.agentName);

        // Check if there are further chained hops
        if (tr.nextSteps.length > 0) {
          const nextStart = tr.endPos.clone();
          const nextEnd = tr.nextSteps[0].clone();
          const nextRemaining = tr.nextSteps.slice(1);

          tr.startPos.copy(nextStart);
          tr.endPos.copy(nextEnd);
          tr.currentPos.copy(nextStart);
          tr.progress = 0;
          tr.nextSteps = nextRemaining;
        } else {
          // Cleanup finished traversal
          this.group.remove(tr.tracerMesh);
          this.group.remove(tr.trailLine);
          tr.tracerMesh.geometry.dispose();
          tr.tracerMaterial.dispose();
          tr.trailLine.geometry.dispose();
          (tr.trailLine.material as THREE.Material).dispose();
          this.traversals.splice(i, 1);
        }
      }
    }

    // 3. Update Particle Sparks
    let activeSparkCount = 0;
    for (let i = this.sparks.length - 1; i >= 0; i--) {
      const sp = this.sparks[i];
      sp.life += dt;
      if (sp.life >= sp.maxLife) {
        this.sparks.splice(i, 1);
        continue;
      }

      // Physics damping & movement
      sp.velocity.multiplyScalar(Math.max(0, 1.0 - 2.8 * dt));
      sp.position.addScaledVector(sp.velocity, dt);

      const fade = 1.0 - sp.life / sp.maxLife;

      this.sparkPositions[activeSparkCount * 3] = sp.position.x;
      this.sparkPositions[activeSparkCount * 3 + 1] = sp.position.y;
      this.sparkPositions[activeSparkCount * 3 + 2] = sp.position.z;

      this.sparkColors[activeSparkCount * 3] = sp.color.r * fade;
      this.sparkColors[activeSparkCount * 3 + 1] = sp.color.g * fade;
      this.sparkColors[activeSparkCount * 3 + 2] = sp.color.b * fade;

      activeSparkCount++;
    }

    // Zero out unused particle buffer slots
    for (let i = activeSparkCount; i < Math.min(activeSparkCount + 20, this.maxSparks); i++) {
      this.sparkPositions[i * 3] = 0;
      this.sparkPositions[i * 3 + 1] = 0;
      this.sparkPositions[i * 3 + 2] = 0;
      this.sparkColors[i * 3] = 0;
      this.sparkColors[i * 3 + 1] = 0;
      this.sparkColors[i * 3 + 2] = 0;
    }

    if (this.sparkGeometry) {
      (this.sparkGeometry.getAttribute('position') as THREE.BufferAttribute).needsUpdate = true;
      (this.sparkGeometry.getAttribute('color') as THREE.BufferAttribute).needsUpdate = true;
      this.sparkGeometry.setDrawRange(0, activeSparkCount);
    }
  }

  public clear() {
    for (const sw of this.shockwaves) {
      this.group.remove(sw.mesh);
      sw.mesh.geometry.dispose();
      sw.material.dispose();
    }
    this.shockwaves = [];

    for (const tr of this.traversals) {
      this.group.remove(tr.tracerMesh);
      this.group.remove(tr.trailLine);
      tr.tracerMesh.geometry.dispose();
      tr.tracerMaterial.dispose();
      tr.trailLine.geometry.dispose();
      (tr.trailLine.material as THREE.Material).dispose();
    }
    this.traversals = [];
    this.sparks = [];
  }
}
