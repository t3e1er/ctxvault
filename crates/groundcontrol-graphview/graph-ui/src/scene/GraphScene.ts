import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';

import { CorpusMetadata, EdgeData, GraphPayload, NodeData, ViewMode, AgentActivation } from '../types.ts';
import { NodeCloud } from './NodeCloud.ts';
import { EdgeLines } from './EdgeLines.ts';
import { FlowParticles } from './FlowParticles.ts';
import { CorpusLabels } from './CorpusLabels.ts';
import { ActivationEffects } from './ActivationEffects.ts';
import { resolveAgentVisual } from '../lib/colors.ts';
import { calcBloomStrength } from '../lib/density.ts';

export class GraphScene {
  public scene: THREE.Scene;
  public camera: THREE.PerspectiveCamera;
  public renderer: THREE.WebGLRenderer;
  public controls: OrbitControls;
  public composer: EffectComposer;
  public bloomPass: UnrealBloomPass;

  public nodeCloud: NodeCloud;
  public edgeLines: EdgeLines;
  public flowParticles: FlowParticles;
  public corpusLabels: CorpusLabels;
  public activationEffects: ActivationEffects;

  private raycaster = new THREE.Raycaster();
  private mouse = new THREE.Vector2(-1000, -1000);
  private container: HTMLElement;
  private animationFrameId: number | null = null;
  private lastTime = performance.now();

  /** The "calm" bloom strength set by the slider / data-driven logic. */
  private bloomBaseStrength: number = 0.38;
  /** Extra bloom added by activation bursts; decays exponentially each frame. */
  private bloomBoostCurrent: number = 0.0;

  public onNodeHover?: (node: NodeData | null) => void;
  public onNodeClick?: (node: NodeData) => void;
  public onBackgroundClick?: () => void;

  constructor(container: HTMLElement) {
    this.container = container;

    // 1. Scene
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x07090e);
    this.scene.fog = new THREE.FogExp2(0x07090e, 0.00003);

    // 2. Camera
    const aspect = container.clientWidth / container.clientHeight;
    this.camera = new THREE.PerspectiveCamera(55, aspect, 1, 100000);
    this.camera.position.set(0, 450, 1200);

    // 3. Renderer
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
    this.renderer.setSize(container.clientWidth, container.clientHeight);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.15;
    container.appendChild(this.renderer.domElement);

    // 4. Controls
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.06;
    this.controls.maxDistance = 25000;
    this.controls.minDistance = 20;
    this.controls.autoRotate = true;
    this.controls.autoRotateSpeed = 1.0;

    // 5. Postprocessing (Bloom)
    const renderScene = new RenderPass(this.scene, this.camera);
    this.bloomPass = new UnrealBloomPass(
      new THREE.Vector2(container.clientWidth, container.clientHeight),
      0.38,
      0.30,
      0.48
    );

    this.composer = new EffectComposer(this.renderer);
    this.composer.addPass(renderScene);
    this.composer.addPass(this.bloomPass);

    // 6. Sub-components
    this.nodeCloud = new NodeCloud();
    this.edgeLines = new EdgeLines();
    this.flowParticles = new FlowParticles();
    this.corpusLabels = new CorpusLabels();
    this.activationEffects = new ActivationEffects();

    // Wire bloom surge from activation effects
    this.activationEffects.onBloomSurge = (magnitude: number) => {
      this.bloomBoostCurrent = Math.max(this.bloomBoostCurrent, magnitude);
    };

    this.scene.add(this.nodeCloud.group);
    this.scene.add(this.edgeLines.group);
    this.scene.add(this.flowParticles.group);
    this.scene.add(this.corpusLabels.group);
    this.scene.add(this.activationEffects.group);

    // 7. Event listeners
    window.addEventListener('resize', this.onResize);
    this.renderer.domElement.addEventListener('mousemove', this.onMouseMove);
    this.renderer.domElement.addEventListener('click', this.onClick);

    // 8. Start loop
    this.render();
  }

  public setData(payload: GraphPayload, corporaMeta: CorpusMetadata[], mode: ViewMode = 'entity') {
    this.nodeCloud.setNodes(payload.nodes, mode, corporaMeta, payload.corpus);
    this.edgeLines.setData(payload.nodes, payload.edges);
    this.flowParticles.setEdges(payload.nodes, payload.edges);

    // Update Bloom strength dynamically based on node count
    const bloom = calcBloomStrength(payload.nodes.length);
    this.bloomBaseStrength = bloom.strength;
    this.bloomPass.strength = bloom.strength;
    this.bloomPass.radius = bloom.radius;
    this.bloomPass.threshold = bloom.threshold;

    // Position floating labels at the exact space center (center of sphere), not density centroid
    if (payload.corpus === 'all' || payload.corpus === 'overview') {
      const activeMeta = corporaMeta && corporaMeta.length > 0 ? [...corporaMeta] : [];
      if (activeMeta.length === 0) {
        const corpusNames = Array.from(new Set(payload.nodes.map((n) => n.corpus || 'default')));
        for (const cName of corpusNames) {
          activeMeta.push({ name: cName, nodes: 0, edges: 0, graph_mtime: 0 });
        }
      }
      for (const meta of activeMeta) {
        if (!meta.center) {
          const cNodes = payload.nodes.filter((n) => n.corpus === meta.name);
          if (cNodes.length > 0) {
            let cx = 0, cy = 0, cz = 0;
            for (const cn of cNodes) {
              cx += cn.position[0];
              cy += cn.position[1];
              cz += cn.position[2];
            }
            meta.center = [cx / cNodes.length, cy / cNodes.length, cz / cNodes.length];
            meta.nodes = meta.nodes || cNodes.length;
          } else {
            meta.center = [0, 0, 0];
          }
        }
      }
      this.corpusLabels.setCorpora(activeMeta);
    } else {
      const singleMeta: CorpusMetadata = {
        name: payload.corpus,
        nodes: payload.nodes.length,
        edges: payload.edges.length,
        graph_mtime: 0,
        center: [0, 0, 0], // Space center of single corpus sphere is origin
      };
      this.corpusLabels.setCorpora([singleMeta]);
    }

    // Automatically frame all items on first load / data update
    this.fitToBounds(payload.nodes);
  }

  public fitToBounds(nodes: NodeData[]) {
    if (!nodes || nodes.length === 0) return;

    let minX = Infinity, maxX = -Infinity;
    let minY = Infinity, maxY = -Infinity;
    let minZ = Infinity, maxZ = -Infinity;

    for (let i = 0; i < nodes.length; i++) {
      const p = nodes[i].position;
      if (p[0] < minX) minX = p[0];
      if (p[0] > maxX) maxX = p[0];
      if (p[1] < minY) minY = p[1];
      if (p[1] > maxY) maxY = p[1];
      if (p[2] < minZ) minZ = p[2];
      if (p[2] > maxZ) maxZ = p[2];
    }

    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;
    const cz = (minZ + maxZ) / 2;

    let maxDistSq = 0;
    for (let i = 0; i < nodes.length; i++) {
      const p = nodes[i].position;
      const dx = p[0] - cx;
      const dy = p[1] - cy;
      const dz = p[2] - cz;
      const dSq = dx * dx + dy * dy + dz * dz;
      if (dSq > maxDistSq) maxDistSq = dSq;
    }

    const radius = Math.max(250, Math.sqrt(maxDistSq));
    const halfFov = (this.camera.fov / 2) * (Math.PI / 180);
    const fitDistance = (radius / Math.sin(halfFov)) * 1.15;

    this.controls.target.set(cx, cy, cz);
    this.camera.position.set(cx, cy + fitDistance * 0.22, cz + fitDistance);
    this.camera.lookAt(cx, cy, cz);
    this.controls.update();

    // Adapt fog density dynamically based on scene scale so distant overview is never blacked out
    if (this.scene.fog instanceof THREE.FogExp2) {
      this.scene.fog.density = Math.max(0.000008, Math.min(0.0001, 0.35 / fitDistance));
    }
  }

  public flyTo(target: [number, number, number], distance: number = 400) {
    const startPos = this.camera.position.clone();
    const endPos = new THREE.Vector3(target[0], target[1] + distance * 0.4, target[2] + distance);
    const startTarget = this.controls.target.clone();
    const endTarget = new THREE.Vector3(target[0], target[1], target[2]);

    const startTime = performance.now();
    const duration = 1000;

    const animate = () => {
      const elapsed = performance.now() - startTime;
      const progress = Math.min(1.0, elapsed / duration);
      const ease = 0.5 - Math.cos(progress * Math.PI) / 2; // smooth easeInOut

      this.camera.position.lerpVectors(startPos, endPos, ease);
      this.controls.target.lerpVectors(startTarget, endTarget, ease);

      if (progress < 1.0) {
        requestAnimationFrame(animate);
      }
    };
    animate();
  }

  public setParticleScale(scale: number) {
    this.nodeCloud.setParticleScale(scale);
  }

  public setBloomStrength(val: number) {
    this.bloomBaseStrength = val / 30.0;
    this.bloomPass.strength = this.bloomBaseStrength + this.bloomBoostCurrent;
  }

  /** Spike bloom up; decays automatically in the render loop. */
  public triggerBloomSurge(boost: number) {
    this.bloomBoostCurrent = Math.max(this.bloomBoostCurrent, boost);
  }

  public setBloomThreshold(val: number) {
    this.bloomPass.threshold = val;
  }

  public setExposure(val: number) {
    this.renderer.toneMappingExposure = val;
  }

  public setGradientContrast(enabled: boolean) {
    this.nodeCloud.setGradientContrast(enabled);
    this.edgeLines.setGradientContrast(enabled);
  }

  public setEdgeDensity(val: number) {
    this.edgeLines.setOpacity(val / 100.0);
  }

  public setAutoRotate(enabled: boolean, speed?: number) {
    this.controls.autoRotate = enabled;
    if (speed !== undefined) {
      this.controls.autoRotateSpeed = speed / 8.0;
    }
  }

  public setSearchMatches(matchIds: Set<number> | null) {
    this.nodeCloud.setSearchMatches(matchIds);
  }

  public setEntityFilter(category: string) {
    this.nodeCloud.setEntityFilter(category);
  }

  public updateGravity(params: {
    communityGravity?: number;
    corpusGravity?: number;
    interCorpusAttraction?: number;
  }) {
    const res = this.nodeCloud.updateGravityAndScales(params);
    this.edgeLines.updatePositions(res.nodePositions);
    this.corpusLabels.updatePositions(res.corpusCenters);
  }

  private onResize = () => {
    const w = this.container.clientWidth;
    const h = this.container.clientHeight;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h);
    this.composer.setSize(w, h);
  };

  private onMouseMove = (e: MouseEvent) => {
    const rect = this.renderer.domElement.getBoundingClientRect();
    this.mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
    this.mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
  };

  private onClick = () => {
    this.raycaster.setFromCamera(this.mouse, this.camera);
    const hit = this.nodeCloud.raycast(this.raycaster);
    if (hit) {
      this.nodeCloud.setSelectedId(hit.id);
      this.onNodeClick?.(hit);
    } else {
      this.onBackgroundClick?.();
    }
  };

  private render = () => {
    this.animationFrameId = requestAnimationFrame(this.render);

    const now = performance.now();
    const dt = Math.min(0.1, (now - this.lastTime) / 1000);
    this.lastTime = now;

    this.controls.update();

    // Hover raycasting
    this.raycaster.setFromCamera(this.mouse, this.camera);
    const hovered = this.nodeCloud.raycast(this.raycaster);
    this.nodeCloud.setHoveredId(hovered ? hovered.id : null);
    this.onNodeHover?.(hovered);

    // Update animated edge particle flows
    this.flowParticles.update(dt);

    // Update agent activation animations & node glow decays
    this.activationEffects.update(dt, this.camera);
    this.nodeCloud.update(dt);

    // Exponential decay of activation bloom boost (~95% per frame at 60fps ≈ 0.5s half-life)
    this.bloomBoostCurrent *= 0.962;
    if (this.bloomBoostCurrent < 0.005) this.bloomBoostCurrent = 0;
    this.bloomPass.strength = this.bloomBaseStrength + this.bloomBoostCurrent;

    this.composer.render();
  };

  public setActivationScale(scale: number) {
    this.activationEffects.setActivationScale(scale);
    this.nodeCloud.setActivationScale(scale);
  }

  public triggerActivationEffect(
    act: AgentActivation,
    nodes: NodeData[],
    fallbackPosition?: [number, number, number]
  ) {
    const visual = resolveAgentVisual(act);
    const color = new THREE.Color(visual.colorHex);
    this.activationEffects.triggerActivation(act, nodes, fallbackPosition);
    if (nodes.length > 0) {
      this.nodeCloud.flashNodes(nodes.map((n) => n.id), color, 4200);
    }
  }

  public destroy() {
    if (this.animationFrameId) cancelAnimationFrame(this.animationFrameId);
    window.removeEventListener('resize', this.onResize);
    this.renderer.domElement.removeEventListener('mousemove', this.onMouseMove);
    this.renderer.domElement.removeEventListener('click', this.onClick);
    this.renderer.dispose();
  }
}
