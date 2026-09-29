import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js';

import { CorpusMetadata, EdgeData, GraphPayload, NodeData, ViewMode } from '../types.ts';
import { NodeCloud } from './NodeCloud.ts';
import { EdgeLines } from './EdgeLines.ts';
import { FlowParticles } from './FlowParticles.ts';
import { CorpusLabels } from './CorpusLabels.ts';
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

  private raycaster = new THREE.Raycaster();
  private mouse = new THREE.Vector2(-1000, -1000);
  private container: HTMLElement;
  private animationFrameId: number | null = null;
  private lastTime = performance.now();

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

    this.scene.add(this.nodeCloud.group);
    this.scene.add(this.edgeLines.group);
    this.scene.add(this.flowParticles.group);
    this.scene.add(this.corpusLabels.group);

    // 7. Event listeners
    window.addEventListener('resize', this.onResize);
    this.renderer.domElement.addEventListener('mousemove', this.onMouseMove);
    this.renderer.domElement.addEventListener('click', this.onClick);

    // 8. Start loop
    this.render();
  }

  public setData(payload: GraphPayload, corporaMeta: CorpusMetadata[], mode: ViewMode = 'entity') {
    this.nodeCloud.setNodes(payload.nodes, mode);
    this.edgeLines.setData(payload.nodes, payload.edges);
    this.flowParticles.setEdges(payload.nodes, payload.edges);

    // Update Bloom strength dynamically based on node count
    const bloom = calcBloomStrength(payload.nodes.length);
    this.bloomPass.strength = bloom.strength;
    this.bloomPass.radius = bloom.radius;
    this.bloomPass.threshold = bloom.threshold;

    // Calculate corpus centers for floating labels if needed
    if (payload.corpus === 'all' || payload.corpus === 'overview') {
      const centers = this.calculateCorpusCenters(payload.nodes);
      for (const meta of corporaMeta) {
        if (centers.has(meta.name)) {
          meta.center = centers.get(meta.name);
        }
      }
      this.corpusLabels.setCorpora(corporaMeta);
    } else {
      this.corpusLabels.clear();
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

  private calculateCorpusCenters(nodes: NodeData[]): Map<string, [number, number, number]> {
    const sums = new Map<string, [number, number, number, number]>();
    for (const n of nodes) {
      const c = n.corpus || 'default';
      const entry = sums.get(c) || [0, 0, 0, 0];
      entry[0] += n.position[0];
      entry[1] += n.position[1];
      entry[2] += n.position[2];
      entry[3] += 1;
      sums.set(c, entry);
    }

    const centers = new Map<string, [number, number, number]>();
    for (const [c, val] of sums.entries()) {
      if (val[3] > 0) {
        centers.set(c, [val[0] / val[3], val[1] / val[3], val[2] / val[3]]);
      }
    }
    return centers;
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
    this.bloomPass.strength = val / 30.0;
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

  public updateClusterScales(clusterDistScale: number, nodeDispScale: number) {
    const updatedPositions = this.nodeCloud.updateClusterScales(clusterDistScale, nodeDispScale);
    this.edgeLines.updatePositions(updatedPositions);
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

    this.composer.render();
  };

  public destroy() {
    if (this.animationFrameId) cancelAnimationFrame(this.animationFrameId);
    window.removeEventListener('resize', this.onResize);
    this.renderer.domElement.removeEventListener('mousemove', this.onMouseMove);
    this.renderer.domElement.removeEventListener('click', this.onClick);
    this.renderer.dispose();
  }
}
