import { EdgeClass, EdgeData, NodeData } from '../types.ts';
import { EDGE_CLASS_COLORS, hexToCss, getEdgeColor, getCategoryColor } from '../lib/colors.ts';

export class FilterPanel {
  private container: HTMLElement;
  public onEntityFilter?: (category: string) => void;
  public onEdgeClassToggle?: (edgeClass: EdgeClass, enabled: boolean) => void;
  public onEdgeTypeToggle?: (edgeType: string, enabled: boolean) => void;
  public onParticleSizeChange?: (val: number) => void;
  public onBloomChange?: (val: number) => void;
  public onEdgeDensityChange?: (val: number) => void;
  public onClusterDistChange?: (val: number) => void;
  public onNodeDispChange?: (val: number) => void;
  public onAutoOrbitToggle?: (enabled: boolean) => void;
  public onOrbitSpeedChange?: (speed: number) => void;

  private selectedEntity: string = 'all';
  private visualControlsCollapsed: boolean = true;

  private classStates: Map<EdgeClass, boolean> = new Map([
    [EdgeClass.Structural, true],
    [EdgeClass.Code, true],
    [EdgeClass.Semantic, true],
    [EdgeClass.CrossModal, true],
    [EdgeClass.Hybrid, true],
  ]);

  private typeStates: Map<string, boolean> = new Map();

  constructor(container: HTMLElement) {
    this.container = container;
  }

  public render(nodes: NodeData[], edges: EdgeData[]) {
    // 1. Entity type counts
    const entityCounts: Record<string, number> = {
      all: nodes.length,
      DocNode: 0,
      Function: 0,
      Struct: 0,
      Trait: 0,
      Enum: 0,
      Module: 0,
      Macro: 0,
      CodeSymbol: 0,
    };

    for (const n of nodes) {
      const type = n.entityType || 'CodeSymbol';
      if (entityCounts[type] !== undefined) {
        entityCounts[type]++;
      } else {
        entityCounts.CodeSymbol++;
      }
    }

    const entityChips = [
      { key: 'all', label: 'All Entities', color: '#ffffff' },
      { key: 'DocNode', label: 'Doc Notes / ADR', color: hexToCss(getCategoryColor('DocNode')) },
      { key: 'Function', label: 'Functions / Methods', color: hexToCss(getCategoryColor('Function')) },
      { key: 'Struct', label: 'Structs / Classes', color: hexToCss(getCategoryColor('Struct')) },
      { key: 'Trait', label: 'Traits / Interfaces', color: hexToCss(getCategoryColor('Trait')) },
      { key: 'Enum', label: 'Enums / Types', color: hexToCss(getCategoryColor('Enum')) },
      { key: 'Module', label: 'Modules / Files', color: hexToCss(getCategoryColor('Module')) },
      { key: 'Macro', label: 'Macros', color: hexToCss(getCategoryColor('Macro')) },
      { key: 'CodeSymbol', label: 'Symbols / References', color: hexToCss(getCategoryColor('CodeSymbol')) },
    ];

    const entityChipsHtml = entityChips
      .map(
        (ec) => `
        <div class="filter-chip ${this.selectedEntity === ec.key ? 'active' : ''}" data-entity="${ec.key}">
          <span class="chip-dot" style="background: ${ec.color}; box-shadow: 0 0 6px ${ec.color}88"></span>
          <span class="chip-label">${ec.label}</span>
          <span class="chip-count">${(entityCounts[ec.key] || 0).toLocaleString()}</span>
        </div>
      `
      )
      .join('');

    // 2. Edge classes
    const classCounts: Record<EdgeClass, number> = {
      [EdgeClass.Structural]: 0,
      [EdgeClass.Code]: 0,
      [EdgeClass.Semantic]: 0,
      [EdgeClass.CrossModal]: 0,
      [EdgeClass.Hybrid]: 0,
    };

    const typeCounts: Record<string, number> = {};
    for (const e of edges) {
      classCounts[e.edgeClass] = (classCounts[e.edgeClass] || 0) + 1;
      const t = e.edgeType.toLowerCase();
      typeCounts[t] = (typeCounts[t] || 0) + 1;
      if (!this.typeStates.has(t)) {
        this.typeStates.set(t, true);
      }
    }

    const classEntries: Array<{ cls: EdgeClass; label: string }> = [
      { cls: EdgeClass.Code, label: 'Code' },
      { cls: EdgeClass.Structural, label: 'Structural' },
      { cls: EdgeClass.Semantic, label: 'Semantic' },
      { cls: EdgeClass.CrossModal, label: 'CrossModal' },
      { cls: EdgeClass.Hybrid, label: 'Hybrid' },
    ];

    const classChipsHtml = classEntries
      .map(({ cls, label }) => {
        const color = hexToCss(EDGE_CLASS_COLORS[cls]);
        const active = this.classStates.get(cls) !== false;
        const count = classCounts[cls] || 0;
        return `
          <div class="filter-chip ${active ? 'active' : ''}" data-class="${cls}">
            <span class="chip-dot" style="background: ${color}; box-shadow: 0 0 8px ${color}"></span>
            <span class="chip-label">${label}</span>
            <span class="chip-count">${count.toLocaleString()}</span>
          </div>
        `;
      })
      .join('');

    // 3. Edge types
    const sortedTypes = Object.entries(typeCounts).sort((a, b) => b[1] - a[1]);
    const typeChipsHtml = sortedTypes
      .slice(0, 14)
      .map(([type, count]) => {
        const active = this.typeStates.get(type) !== false;
        const color = hexToCss(getEdgeColor(type, EdgeClass.Structural));
        return `
          <div class="filter-chip type-chip ${active ? 'active' : ''}" data-type="${type}">
            <span class="chip-dot" style="background: ${color}"></span>
            <span class="chip-label">${type}</span>
            <span class="chip-count">${count.toLocaleString()}</span>
          </div>
        `;
      })
      .join('');

    this.container.innerHTML = `
      <div class="filter-panel glass-panel">
        <!-- Entity Classes -->
        <div class="panel-section">
          <div class="section-title">ENTITY CLASSES</div>
          <div class="chips-container scrollable" id="entity-chips" style="max-height: 160px;">
            ${entityChipsHtml}
          </div>
        </div>

        <!-- Edge Classes -->
        <div class="panel-section">
          <div class="section-title">EDGE CLASSES</div>
          <div class="chips-container" id="class-chips">
            ${classChipsHtml}
          </div>
        </div>

        <!-- Edge Types -->
        <div class="panel-section" style="flex: 1 1 auto; min-height: 0; overflow: hidden; display: flex; flex-direction: column;">
          <div class="section-title">EDGE TYPES</div>
          <div class="chips-container scrollable" id="type-chips" style="flex: 1 1 auto;">
            ${typeChipsHtml}
          </div>
        </div>

        <!-- Collapsible Visual Controls Panel -->
        <div class="visual-controls-panel ${this.visualControlsCollapsed ? 'collapsed' : ''}" id="visual-controls-panel">
          <div class="visual-controls-header" id="visual-controls-header" title="Toggle Visual Controls">
            <div class="section-title" style="margin: 0;">VISUAL CONTROLS</div>
            <div style="display: flex; align-items: center; gap: 6px;">
              <span style="font-size: 10px; color: var(--text-muted)">Physics & Light</span>
              <svg class="visual-controls-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </div>
          </div>
          <div class="visual-controls-body" id="visual-controls-body">
            <div class="slider-row">
              <span>Particle Size</span>
              <input type="range" id="size-slider" min="1" max="15" value="6">
            </div>
            <div class="slider-row">
              <span>Bloom Strength</span>
              <input type="range" id="bloom-slider" min="0" max="30" value="18">
            </div>
            <div class="slider-row">
              <span>Edge Density</span>
              <input type="range" id="edge-slider" min="0" max="100" value="70">
            </div>
            <div class="slider-row">
              <span>Cluster Spacing</span>
              <input type="range" id="cluster-dist-slider" min="1" max="40" value="10" title="Adjust distance between community clusters">
            </div>
            <div class="slider-row">
              <span>Node Spread</span>
              <input type="range" id="node-disp-slider" min="1" max="30" value="10" title="Adjust local dispersion of nodes">
            </div>
            <div class="slider-row" style="padding-top: 4px; border-top: 1px solid rgba(255,255,255,0.06);">
              <label style="display: flex; align-items: center; gap: 6px; cursor: pointer;">
                <input type="checkbox" id="auto-orbit-toggle" checked style="accent-color: var(--neon-blue);">
                <span>Auto Orbit</span>
              </label>
              <input type="range" id="orbit-speed-slider" min="1" max="30" value="8" style="width: 75px;">
            </div>
          </div>
        </div>
      </div>
    `;

    // Wire entity filter chips
    this.container.querySelectorAll('#entity-chips .filter-chip').forEach((chip) => {
      chip.addEventListener('click', () => {
        const ent = chip.getAttribute('data-entity')!;
        this.selectedEntity = ent;
        this.container.querySelectorAll('#entity-chips .filter-chip').forEach((c) => c.classList.remove('active'));
        chip.classList.add('active');
        this.onEntityFilter?.(ent);
      });
    });

    // Wire class toggles
    this.container.querySelectorAll('#class-chips .filter-chip').forEach((chip) => {
      chip.addEventListener('click', () => {
        const cls = Number(chip.getAttribute('data-class')) as EdgeClass;
        const current = this.classStates.get(cls) !== false;
        const next = !current;
        this.classStates.set(cls, next);
        chip.classList.toggle('active', next);
        this.onEdgeClassToggle?.(cls, next);
      });
    });

    // Wire type toggles
    this.container.querySelectorAll('#type-chips .type-chip').forEach((chip) => {
      chip.addEventListener('click', () => {
        const type = chip.getAttribute('data-type')!;
        const current = this.typeStates.get(type) !== false;
        const next = !current;
        this.typeStates.set(type, next);
        chip.classList.toggle('active', next);
        this.onEdgeTypeToggle?.(type, next);
      });
    });

    // Wire Visual Controls accordion
    const vcHeader = this.container.querySelector('#visual-controls-header') as HTMLElement;
    const vcPanel = this.container.querySelector('#visual-controls-panel') as HTMLElement;
    vcHeader.addEventListener('click', () => {
      this.visualControlsCollapsed = !this.visualControlsCollapsed;
      vcPanel.classList.toggle('collapsed', this.visualControlsCollapsed);
    });

    // Wire sliders
    const sizeSlider = this.container.querySelector('#size-slider') as HTMLInputElement;
    sizeSlider?.addEventListener('input', () => {
      this.onParticleSizeChange?.(parseFloat(sizeSlider.value));
    });

    const bloomSlider = this.container.querySelector('#bloom-slider') as HTMLInputElement;
    bloomSlider?.addEventListener('input', () => {
      this.onBloomChange?.(parseFloat(bloomSlider.value));
    });

    const edgeSlider = this.container.querySelector('#edge-slider') as HTMLInputElement;
    edgeSlider?.addEventListener('input', () => {
      this.onEdgeDensityChange?.(parseFloat(edgeSlider.value));
    });

    const clusterDistSlider = this.container.querySelector('#cluster-dist-slider') as HTMLInputElement;
    clusterDistSlider?.addEventListener('input', () => {
      this.onClusterDistChange?.(parseFloat(clusterDistSlider.value));
    });

    const nodeDispSlider = this.container.querySelector('#node-disp-slider') as HTMLInputElement;
    nodeDispSlider?.addEventListener('input', () => {
      this.onNodeDispChange?.(parseFloat(nodeDispSlider.value));
    });

    const orbitToggle = this.container.querySelector('#auto-orbit-toggle') as HTMLInputElement;
    orbitToggle?.addEventListener('change', () => {
      this.onAutoOrbitToggle?.(orbitToggle.checked);
    });

    const orbitSpeedSlider = this.container.querySelector('#orbit-speed-slider') as HTMLInputElement;
    orbitSpeedSlider?.addEventListener('input', () => {
      this.onOrbitSpeedChange?.(parseFloat(orbitSpeedSlider.value));
    });
  }
}
