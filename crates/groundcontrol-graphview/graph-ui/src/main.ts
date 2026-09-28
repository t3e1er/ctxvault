import { GraphScene } from './scene/GraphScene.ts';
import { decodeBinaryGraph } from './wire/decoder.ts';
import { HeaderControls } from './ui/HeaderControls.ts';
import { FilterPanel } from './ui/FilterPanel.ts';
import { Inspector } from './ui/Inspector.ts';
import { TelemetryFeed } from './ui/TelemetryFeed.ts';
import { CorpusMetadata, GraphPayload, NodeData, ViewMode } from './types.ts';

class GraphViewApp {
  private scene: GraphScene;
  private header: HeaderControls;
  private filterPanel: FilterPanel;
  private inspector: Inspector;
  private telemetry: TelemetryFeed;

  private corpora: CorpusMetadata[] = [];
  private activeCorpus = 'all';
  private currentViewMode: ViewMode = 'entity';
  private currentClusterMode: 'community' | 'directory' = 'community';
  private currentPayload: GraphPayload | null = null;
  private currentSearchMatches: Set<number> | null = null;

  constructor() {
    const canvasContainer = document.getElementById('canvas-container')!;
    this.scene = new GraphScene(canvasContainer);

    this.header = new HeaderControls(document.getElementById('header-mount')!);
    this.filterPanel = new FilterPanel(document.getElementById('filter-mount')!);
    this.inspector = new Inspector(document.getElementById('inspector-mount')!);
    this.telemetry = new TelemetryFeed(document.getElementById('telemetry-mount')!);

    this.bindEvents();
  }

  private bindEvents() {
    // 1. Header Events
    this.header.onViewModeChange = (mode) => {
      this.currentViewMode = mode;
      this.scene.nodeCloud.setViewMode(mode);
    };

    this.header.onCorpusChange = (corpus) => {
      this.activeCorpus = corpus;
      this.currentSearchMatches = null;
      this.loadGraph();
    };

    this.header.onClusterModeChange = (clMode) => {
      this.currentClusterMode = clMode;
      this.loadGraph();
    };

    this.header.onReload = async () => {
      const target = this.activeCorpus === 'all' ? 'overview' : this.activeCorpus;
      try {
        await fetch(`/api/graph/reload/${target}`, { method: 'POST' });
        console.log(`[GraphView] Invalidated cache for ${target}`);
        await this.loadGraph();
      } catch (err) {
        console.error('Failed to reload corpus:', err);
      }
    };

    this.header.onQuery = (query) => {
      this.performSearch(query);
    };

    this.header.onQuerySubmit = (query) => {
      this.performSearchSubmit(query);
    };

    // 2. FilterPanel Events
    this.filterPanel.onEntityFilter = (category) => {
      this.scene.setEntityFilter(category);
    };

    this.filterPanel.onEdgeClassToggle = (cls, enabled) => {
      this.scene.edgeLines.toggleClass(cls, enabled);
    };

    this.filterPanel.onEdgeTypeToggle = (type, enabled) => {
      this.scene.edgeLines.toggleType(type, enabled);
    };

    this.filterPanel.onParticleSizeChange = (val) => {
      this.scene.setParticleScale(val);
    };

    this.filterPanel.onBloomChange = (val) => {
      this.scene.setBloomStrength(val);
    };

    this.filterPanel.onEdgeDensityChange = (val) => {
      this.scene.setEdgeDensity(val);
    };

    this.filterPanel.onAutoOrbitToggle = (enabled) => {
      this.scene.setAutoRotate(enabled);
    };

    this.filterPanel.onOrbitSpeedChange = (speed) => {
      this.scene.setAutoRotate(true, speed);
    };

    // 3. Scene Interaction
    this.scene.onNodeClick = (node: NodeData) => {
      this.inspector.showNode(node);
      this.scene.flyTo(node.position, 200);
    };

    this.inspector.onFocusEgo = (node: NodeData) => {
      this.loadEgoSubgraph(node.path);
    };

    // 4. Telemetry Stream
    this.telemetry.onActivation = (act) => {
      if (this.currentPayload && act.paths.length > 0) {
        const touchedPaths = new Set(act.paths.map((p) => p.replace(/\\/g, '/')));
        const match = this.currentPayload.nodes.find((n) => touchedPaths.has(n.path.replace(/\\/g, '/')));
        if (match) {
          this.scene.nodeCloud.setHoveredId(match.id);
        }
      }
    };

    this.telemetry.onSelectPaths = (paths) => {
      if (!this.currentPayload || paths.length === 0) return;
      const targetPaths = new Set(paths.map((p) => p.replace(/\\/g, '/')));
      const match = this.currentPayload.nodes.find((n) => targetPaths.has(n.path.replace(/\\/g, '/')));
      if (match) {
        this.scene.nodeCloud.setSelectedId(match.id);
        this.scene.flyTo(match.position, 220);
        this.inspector.showNode(match);
      }
    };
  }

  private performSearch(rawQuery: string) {
    const q = (rawQuery || '').trim().toLowerCase();
    if (!q || !this.currentPayload) {
      this.currentSearchMatches = null;
      this.scene.setSearchMatches(null);
      this.header.setQueryStatus('idle');
      return;
    }

    const matches = new Set<number>();
    for (const node of this.currentPayload.nodes) {
      const matchPath = node.path && node.path.toLowerCase().includes(q);
      const matchTitle = node.title && node.title.toLowerCase().includes(q);
      const matchType = node.entityType && node.entityType.toLowerCase().includes(q);
      if (matchPath || matchTitle || matchType) {
        matches.add(node.id);
      }
    }

    this.currentSearchMatches = matches;
    this.scene.setSearchMatches(matches);

    if (matches.size > 0) {
      this.header.setQueryStatus('matched');
    } else {
      this.header.setQueryStatus('nomatch');
    }
  }

  private performSearchSubmit(rawQuery: string) {
    this.performSearch(rawQuery);
    if (!this.currentPayload || !this.currentSearchMatches || this.currentSearchMatches.size === 0) return;

    // Find first matching node
    const firstMatch = this.currentPayload.nodes.find((n) => this.currentSearchMatches!.has(n.id));
    if (firstMatch) {
      this.scene.nodeCloud.setSelectedId(firstMatch.id);
      this.scene.flyTo(firstMatch.position, 250);
      this.inspector.showNode(firstMatch);
    }
  }

  public async init() {
    try {
      await this.fetchStatus();
      this.header.render(
        this.corpora,
        this.activeCorpus,
        this.currentViewMode,
        this.currentClusterMode,
        0,
        0
      );
      await this.loadGraph();
      this.telemetry.start();
    } catch (err) {
      console.error('Failed to initialize GraphView:', err);
      this.showLoadingError(String(err));
    }
  }

  private async fetchStatus() {
    const res = await fetch('/api/status');
    if (!res.ok) return;
    const data = await res.json();
    this.corpora = data.corpora || [];
  }

  private showLoading(text: string) {
    const overlay = document.getElementById('loading-overlay');
    const label = document.getElementById('loading-text');
    if (overlay) overlay.classList.remove('hidden');
    if (label) label.textContent = text;
  }

  private hideLoading() {
    const overlay = document.getElementById('loading-overlay');
    if (overlay) overlay.classList.add('hidden');
  }

  private showLoadingError(err: string) {
    const overlay = document.getElementById('loading-overlay');
    const label = document.getElementById('loading-text');
    if (overlay) overlay.classList.remove('hidden');
    if (label) {
      label.innerHTML = `<span style="color:#ef4444">Error loading graph:</span><br/><span style="color:#94a3b8;font-size:11px">${err}</span>`;
    }
  }

  private async loadGraph() {
    const targetLabel = this.activeCorpus === 'all' ? 'All Corpora' : this.activeCorpus;
    this.showLoading(`Loading ${targetLabel} (${this.currentClusterMode} mode)...`);

    try {
      const url =
        this.activeCorpus === 'all'
          ? `/api/graph/overview?cluster_mode=${this.currentClusterMode}`
          : `/api/graph/corpus/${encodeURIComponent(this.activeCorpus)}?cluster_mode=${this.currentClusterMode}`;

      const res = await fetch(url);
      if (!res.ok) throw new Error(`HTTP ${res.status} loading graph`);

      const buffer = await res.arrayBuffer();
      const payload = decodeBinaryGraph(buffer);
      payload.corpus = this.activeCorpus;
      this.currentPayload = payload;

      this.scene.setData(payload, this.corpora, this.currentViewMode);
      this.header.render(
        this.corpora,
        this.activeCorpus,
        this.currentViewMode,
        this.currentClusterMode,
        payload.nodes.length,
        payload.edges.length
      );
      this.filterPanel.render(payload.nodes, payload.edges);
      this.hideLoading();

      // If active search exists, re-apply
      if (this.currentSearchMatches) {
        this.scene.setSearchMatches(this.currentSearchMatches);
      }
    } catch (err) {
      this.showLoadingError(String(err));
      throw err;
    }
  }

  private async loadEgoSubgraph(centerPath: string) {
    const url = `/api/graph/subgraph?center=${encodeURIComponent(centerPath)}&hops=2&budget=120&cluster_mode=${this.currentClusterMode}`;
    const res = await fetch(url);
    if (!res.ok) return;

    const buffer = await res.arrayBuffer();
    const payload = decodeBinaryGraph(buffer);
    payload.corpus = 'ego';
    this.currentPayload = payload;

    this.scene.setData(payload, this.corpora, this.currentViewMode);
    this.filterPanel.render(payload.nodes, payload.edges);
  }
}

window.addEventListener('DOMContentLoaded', () => {
  const app = new GraphViewApp();
  app.init();
});
