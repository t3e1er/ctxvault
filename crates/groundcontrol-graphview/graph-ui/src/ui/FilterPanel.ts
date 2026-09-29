import { EdgeClass, EdgeData, NodeData } from '../types.ts';
import { EDGE_CLASS_COLORS, hexToCss, getEdgeColor, getCategoryColor } from '../lib/colors.ts';

export class FilterPanel {
  private container: HTMLElement;
  public onEntityFilter?: (category: string) => void;
  public onEdgeClassToggle?: (edgeClass: EdgeClass, enabled: boolean) => void;
  public onEdgeTypeToggle?: (edgeType: string, enabled: boolean) => void;
  public onParticleSizeChange?: (val: number) => void;
  public onBloomChange?: (val: number) => void;
  public onBloomThresholdChange?: (val: number) => void;
  public onEdgeDensityChange?: (val: number) => void;
  public onExposureChange?: (val: number) => void;
  public onGradientContrastToggle?: (enabled: boolean) => void;
  public onAutoOrbitToggle?: (enabled: boolean) => void;
  public onOrbitSpeedChange?: (speed: number) => void;
  public onLabelSizeChange?: (val: number) => void;
  public onLabelBrightnessChange?: (val: number) => void;
  public onCommunityGravityChange?: (val: number) => void;
  public onCorpusGravityChange?: (val: number) => void;
  public onInterCorpusAttractionChange?: (val: number) => void;

  private selectedEntity: string = 'all';
  private entityCollapsed: boolean = false;
  private edgeClassCollapsed: boolean = false;
  private edgeTypeCollapsed: boolean = false;
  private visualControlsCollapsed: boolean = false;

  private autoOrbitState: boolean = true;
  private orbitSpeedState: number = 8;
  private exposureState: number = 110;
  private gradientContrastState: boolean = true;
  private labelSizeState: number = 100;
  private labelBrightnessState: number = 85;
  private communityGravityState: number = 100;
  private corpusGravityState: number = 100;
  private interCorpusAttractionState: number = 100;

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
        <!-- Section 1: Entity Classes -->
        <div class="panel-section ${this.entityCollapsed ? 'collapsed' : ''}" id="section-entity">
          <div class="panel-section-header" id="header-entity" title="Toggle Entity Classes">
            <span class="section-title">ENTITY CLASSES</span>
            <div style="display: flex; align-items: center; gap: 6px;">
              <span style="font-size: 10px; color: var(--text-muted)">(${nodes.length})</span>
              <svg class="panel-section-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </div>
          </div>
          <div class="panel-section-body">
            <div class="chips-container scrollable" id="entity-chips" style="max-height: 120px;">
              ${entityChipsHtml}
            </div>
          </div>
        </div>

        <!-- Section 2: Edge Classes -->
        <div class="panel-section ${this.edgeClassCollapsed ? 'collapsed' : ''}" id="section-class">
          <div class="panel-section-header" id="header-class" title="Toggle Edge Classes">
            <span class="section-title">EDGE CLASSES</span>
            <div style="display: flex; align-items: center; gap: 6px;">
              <span style="font-size: 10px; color: var(--text-muted)">(${edges.length})</span>
              <svg class="panel-section-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </div>
          </div>
          <div class="panel-section-body">
            <div class="chips-container" id="class-chips">
              ${classChipsHtml}
            </div>
          </div>
        </div>

        <!-- Section 3: Edge Types -->
        <div class="panel-section ${this.edgeTypeCollapsed ? 'collapsed' : ''}" id="section-type">
          <div class="panel-section-header" id="header-type" title="Toggle Edge Types">
            <span class="section-title">EDGE TYPES</span>
            <div style="display: flex; align-items: center; gap: 6px;">
              <span style="font-size: 10px; color: var(--text-muted)">(${sortedTypes.length})</span>
              <svg class="panel-section-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </div>
          </div>
          <div class="panel-section-body">
            <div class="chips-container scrollable" id="type-chips" style="max-height: 130px;">
              ${typeChipsHtml}
            </div>
          </div>
        </div>

        <!-- Section 4: Collapsible Visual & Physics Controls -->
        <div class="panel-section ${this.visualControlsCollapsed ? 'collapsed' : ''}" id="section-visual" style="flex-shrink: 0;">
          <div class="panel-section-header" id="header-visual" title="Toggle Visual & Physics Controls">
            <span class="section-title">VISUAL CONTROLS</span>
            <div style="display: flex; align-items: center; gap: 6px;">
              <span style="font-size: 10px; color: var(--text-muted)">Physics & Light</span>
              <svg class="panel-section-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </div>
          </div>
          <div class="panel-section-body" id="visual-controls-body">
            <div class="slider-row">
              <span>Particle Size</span>
              <input type="range" id="size-slider" min="1" max="15" value="6">
            </div>
            <div class="slider-row">
              <span>Bloom Glow</span>
              <input type="range" id="bloom-slider" min="0" max="30" value="12">
            </div>
            <div class="slider-row">
              <span>Glow Cutoff</span>
              <input type="range" id="bloom-threshold-slider" min="10" max="95" value="48" title="Cutoff luminance for glow (higher = less blowout)">
            </div>
            <div class="slider-row">
              <span>Brightness</span>
              <input type="range" id="exposure-slider" min="30" max="220" value="${this.exposureState}" title="Scene exposure/brightness level">
            </div>
            <div class="slider-row">
              <span>Edge Opacity</span>
              <input type="range" id="edge-slider" min="0" max="100" value="70">
            </div>
            <div class="slider-row" style="padding-top: 4px; border-top: 1px solid rgba(255,255,255,0.06);">
              <span>Label Size</span>
              <input type="range" id="label-size-slider" min="30" max="250" value="${this.labelSizeState}" title="Adjust size of floating corpus labels">
            </div>
            <div class="slider-row">
              <span>Label Brightness</span>
              <input type="range" id="label-brightness-slider" min="10" max="100" value="${this.labelBrightnessState}" title="Adjust opacity and brightness of floating corpus labels">
            </div>
            <div class="slider-row" style="padding-top: 4px; border-top: 1px solid rgba(255,255,255,0.06);">
              <span>Comm Gravity</span>
              <input type="range" id="community-gravity-slider" min="30" max="250" value="${this.communityGravityState}" title="Inward gravitational pull of nodes toward community centroid">
            </div>
            <div class="slider-row">
              <span>Corpus Gravity</span>
              <input type="range" id="corpus-gravity-slider" min="30" max="250" value="${this.corpusGravityState}" title="Radial pull of community clusters toward corpus sphere mantle">
            </div>
            <div class="slider-row">
              <span>Inter-Corpus Pull</span>
              <input type="range" id="inter-corpus-slider" min="30" max="250" value="${this.interCorpusAttractionState}" title="Attraction force pulling linked corpora closer in Galaxy view">
            </div>
            <div class="slider-row" style="padding-top: 4px; border-top: 1px solid rgba(255,255,255,0.06);">
              <label style="display: flex; align-items: center; gap: 6px; cursor: pointer;" title="Attenuates dense clusters and boosts sparse nodes">
                <input type="checkbox" id="gradient-contrast-toggle" ${this.gradientContrastState ? 'checked' : ''} style="accent-color: var(--neon-purple);">
                <span>Gradient Contrast</span>
              </label>
            </div>
            <div class="slider-row">
              <label style="display: flex; align-items: center; gap: 6px; cursor: pointer;">
                <input type="checkbox" id="auto-orbit-toggle" ${this.autoOrbitState ? 'checked' : ''} style="accent-color: var(--neon-blue);">
                <span>Auto Orbit</span>
              </label>
              <input type="range" id="orbit-speed-slider" min="1" max="30" value="${this.orbitSpeedState}" style="width: 75px;">
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

    // Wire Section 1 Collapse
    const headerEntity = this.container.querySelector('#header-entity');
    const sectionEntity = this.container.querySelector('#section-entity');
    headerEntity?.addEventListener('click', () => {
      this.entityCollapsed = !this.entityCollapsed;
      sectionEntity?.classList.toggle('collapsed', this.entityCollapsed);
    });

    // Wire Section 2 Collapse
    const headerClass = this.container.querySelector('#header-class');
    const sectionClass = this.container.querySelector('#section-class');
    headerClass?.addEventListener('click', () => {
      this.edgeClassCollapsed = !this.edgeClassCollapsed;
      sectionClass?.classList.toggle('collapsed', this.edgeClassCollapsed);
    });

    // Wire Section 3 Collapse
    const headerType = this.container.querySelector('#header-type');
    const sectionType = this.container.querySelector('#section-type');
    headerType?.addEventListener('click', () => {
      this.edgeTypeCollapsed = !this.edgeTypeCollapsed;
      sectionType?.classList.toggle('collapsed', this.edgeTypeCollapsed);
    });

    // Wire Section 4 Collapse
    const headerVisual = this.container.querySelector('#header-visual');
    const sectionVisual = this.container.querySelector('#section-visual');
    headerVisual?.addEventListener('click', () => {
      this.visualControlsCollapsed = !this.visualControlsCollapsed;
      sectionVisual?.classList.toggle('collapsed', this.visualControlsCollapsed);
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

    const bloomThresholdSlider = this.container.querySelector('#bloom-threshold-slider') as HTMLInputElement;
    bloomThresholdSlider?.addEventListener('input', () => {
      this.onBloomThresholdChange?.(parseFloat(bloomThresholdSlider.value) / 100.0);
    });

    const exposureSlider = this.container.querySelector('#exposure-slider') as HTMLInputElement;
    exposureSlider?.addEventListener('input', () => {
      this.exposureState = parseFloat(exposureSlider.value);
      this.onExposureChange?.(this.exposureState / 100.0);
    });

    const edgeSlider = this.container.querySelector('#edge-slider') as HTMLInputElement;
    edgeSlider?.addEventListener('input', () => {
      this.onEdgeDensityChange?.(parseFloat(edgeSlider.value));
    });

    const labelSizeSlider = this.container.querySelector('#label-size-slider') as HTMLInputElement;
    labelSizeSlider?.addEventListener('input', () => {
      this.labelSizeState = parseFloat(labelSizeSlider.value);
      this.onLabelSizeChange?.(this.labelSizeState / 100.0);
    });

    const labelBrightSlider = this.container.querySelector('#label-brightness-slider') as HTMLInputElement;
    labelBrightSlider?.addEventListener('input', () => {
      this.labelBrightnessState = parseFloat(labelBrightSlider.value);
      this.onLabelBrightnessChange?.(this.labelBrightnessState / 100.0);
    });

    const commGravSlider = this.container.querySelector('#community-gravity-slider') as HTMLInputElement;
    commGravSlider?.addEventListener('input', () => {
      this.communityGravityState = parseFloat(commGravSlider.value);
      this.onCommunityGravityChange?.(this.communityGravityState / 100.0);
    });

    const corpGravSlider = this.container.querySelector('#corpus-gravity-slider') as HTMLInputElement;
    corpGravSlider?.addEventListener('input', () => {
      this.corpusGravityState = parseFloat(corpGravSlider.value);
      this.onCorpusGravityChange?.(this.corpusGravityState / 100.0);
    });

    const interCorpusSlider = this.container.querySelector('#inter-corpus-slider') as HTMLInputElement;
    interCorpusSlider?.addEventListener('input', () => {
      this.interCorpusAttractionState = parseFloat(interCorpusSlider.value);
      this.onInterCorpusAttractionChange?.(this.interCorpusAttractionState / 100.0);
    });

    const gradToggle = this.container.querySelector('#gradient-contrast-toggle') as HTMLInputElement;
    gradToggle?.addEventListener('change', () => {
      this.gradientContrastState = gradToggle.checked;
      this.onGradientContrastToggle?.(gradToggle.checked);
    });

    const orbitToggle = this.container.querySelector('#auto-orbit-toggle') as HTMLInputElement;
    orbitToggle?.addEventListener('change', () => {
      this.autoOrbitState = orbitToggle.checked;
      this.onAutoOrbitToggle?.(orbitToggle.checked);
    });

    const orbitSpeedSlider = this.container.querySelector('#orbit-speed-slider') as HTMLInputElement;
    orbitSpeedSlider?.addEventListener('input', () => {
      this.orbitSpeedState = parseFloat(orbitSpeedSlider.value);
      this.onOrbitSpeedChange?.(this.orbitSpeedState);
    });
  }
}
