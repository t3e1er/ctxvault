import { CorpusMetadata, ViewMode } from '../types.ts';

export type SearchMode = 'symbol' | 'graph' | 'read';

export class HeaderControls {
  private container: HTMLElement;
  public onViewModeChange?: (mode: ViewMode) => void;
  public onCorpusChange?: (corpus: string) => void;
  public onReload?: () => void;
  public onQuery?: (query: string) => void;
  public onQuerySubmit?: (query: string) => void;
  public onSearchModeChange?: (mode: SearchMode) => void;
  public onUnfocus?: () => void;
  public currentSearchMode: SearchMode = 'symbol';

  constructor(container: HTMLElement) {
    this.container = container;
  }

  private getPlaceholder(): string {
    if (this.currentSearchMode === 'graph') return 'Graph: calls:x, defines:y, edge:t...';
    if (this.currentSearchMode === 'read') return 'Read: file path or symbol name...';
    return 'Search symbols, paths, types...';
  }

  public setSearchQuery(query: string) {
    const input = this.container.querySelector('#query-input') as HTMLInputElement;
    if (input) {
      input.value = query;
      this.onQuery?.(query);
    }
  }

  public setSearchMode(mode: SearchMode) {
    this.currentSearchMode = mode;
    const select = this.container.querySelector('#search-mode-select') as HTMLSelectElement;
    if (select) select.value = mode;
    const input = this.container.querySelector('#query-input') as HTMLInputElement;
    if (input) input.placeholder = this.getPlaceholder();
  }

  public render(
    corpora: CorpusMetadata[],
    activeCorpus: string,
    currentViewMode: ViewMode,
    nodeCount: number,
    edgeCount: number
  ) {
    const corpusOptions = [
      '<option value="all">All Corpora (Galaxy)</option>',
      ...(activeCorpus === 'ego' ? [`<option value="ego" selected>Ego Subgraph (${nodeCount.toLocaleString()} n)</option>`] : []),
      ...corpora.map(
        (c) =>
          `<option value="${c.name}" ${c.name === activeCorpus ? 'selected' : ''}>${c.name} (${c.nodes.toLocaleString()} n)</option>`
      ),
    ].join('');

    this.container.innerHTML = `
      <header class="hud-header glass-panel">
        <div class="header-left">
          <div class="brand">
            <svg class="brand-icon" viewBox="0 0 32 32" width="22" height="22">
              <circle cx="16" cy="16" r="14" fill="none" stroke="#38bdf8" stroke-width="2"/>
              <circle cx="16" cy="16" r="4" fill="#38bdf8"/>
              <line x1="16" y1="2" x2="16" y2="10" stroke="#a855f7" stroke-width="2"/>
              <line x1="16" y1="22" x2="16" y2="30" stroke="#10b981" stroke-width="2"/>
            </svg>
            <span class="brand-title">GROUNDCONTROL <span class="brand-sub">GRAPHVIEW</span></span>
          </div>

          <div class="control-group" style="display: flex; align-items: center; gap: 6px;">
            <select id="corpus-select" class="hud-select">
              ${corpusOptions}
            </select>
            <button id="reload-btn" class="hud-btn" title="Reload corpus and invalidate cache" style="display: flex; align-items: center; justify-content: center; width: 28px; height: 28px; padding: 0;">
              <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/>
              </svg>
            </button>
            ${
              activeCorpus === 'ego'
                ? `
              <button id="header-unfocus-btn" class="hud-action-btn active" title="Exit ego subgraph and return to full graph" style="display: flex; align-items: center; gap: 5px; padding: 4px 10px; font-size: 11px; background: rgba(239, 68, 68, 0.2); border-color: rgba(239, 68, 68, 0.45); color: #f87171;">
                <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="9 14 4 9 9 4"></polyline>
                  <path d="M20 20v-7a4 4 0 0 0-4-4H4"></path>
                </svg>
                <span>Unfocus</span>
              </button>
            `
                : ''
            }
          </div>

          <!-- Real Search Bar in Header with MCP Mode Selector -->
          <div class="header-search" id="header-search-box">
            <select id="search-mode-select" class="search-mode-select" title="MCP Search Mode">
              <option value="symbol" ${this.currentSearchMode === 'symbol' ? 'selected' : ''}>Symbol</option>
              <option value="graph" ${this.currentSearchMode === 'graph' ? 'selected' : ''}>Graph</option>
              <option value="read" ${this.currentSearchMode === 'read' ? 'selected' : ''}>Read</option>
            </select>
            <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="#94a3b8" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink: 0;">
              <circle cx="11" cy="11" r="8"/>
              <line x1="21" y1="21" x2="16.65" y2="16.65"/>
            </svg>
            <input type="text" id="query-input" placeholder="${this.getPlaceholder()}" class="hud-input"/>
          </div>
        </div>

        <div class="header-right">
          <div class="pill-group" id="view-mode-pills">
            <button class="pill-btn ${currentViewMode === 'entity' ? 'active' : ''}" data-mode="entity">
              Entity Type
            </button>
            <button class="pill-btn ${currentViewMode === 'degree' ? 'active' : ''}" data-mode="degree">
              Degree
            </button>
            <button class="pill-btn ${currentViewMode === 'community' ? 'active' : ''}" data-mode="community">
              Community
            </button>
          </div>

          <div class="stats-badge">
            <span class="stat-highlight">${nodeCount.toLocaleString()}</span> nodes
            <span class="stat-sep" style="color: rgba(255,255,255,0.2); margin: 0 4px;">/</span>
            <span class="stat-highlight">${edgeCount.toLocaleString()}</span> edges
          </div>
        </div>
      </header>
    `;

    // Wire events
    const select = this.container.querySelector('#corpus-select') as HTMLSelectElement;
    select.addEventListener('change', () => {
      this.onCorpusChange?.(select.value);
    });

    const reloadBtn = this.container.querySelector('#reload-btn') as HTMLButtonElement;
    reloadBtn?.addEventListener('click', () => {
      this.onReload?.();
    });

    const unfocusBtn = this.container.querySelector('#header-unfocus-btn') as HTMLButtonElement;
    unfocusBtn?.addEventListener('click', () => {
      this.onUnfocus?.();
    });

    const modeSelect = this.container.querySelector('#search-mode-select') as HTMLSelectElement;
    if (modeSelect) {
      modeSelect.addEventListener('change', () => {
        this.currentSearchMode = modeSelect.value as SearchMode;
        if (queryInput) queryInput.placeholder = this.getPlaceholder();
        this.onSearchModeChange?.(this.currentSearchMode);
      });
    }

    const queryInput = this.container.querySelector('#query-input') as HTMLInputElement;
    if (queryInput) {
      queryInput.addEventListener('input', () => {
        this.onQuery?.(queryInput.value);
      });
      queryInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
          this.onQuerySubmit?.(queryInput.value);
        } else if (e.key === 'Escape') {
          queryInput.value = '';
          this.onQuery?.('');
        }
      });
    }

    const viewPills = this.container.querySelectorAll('#view-mode-pills .pill-btn');
    viewPills.forEach((btn) => {
      btn.addEventListener('click', () => {
        const mode = btn.getAttribute('data-mode') as ViewMode;
        viewPills.forEach((b) => b.classList.remove('active'));
        btn.classList.add('active');
        this.onViewModeChange?.(mode);
      });
    });
  }

  public setQueryStatus(status: 'idle' | 'matched' | 'nomatch') {
    const box = this.container.querySelector('#header-search-box') as HTMLElement;
    if (!box) return;
    if (status === 'matched') {
      box.style.borderColor = '#f59e0b';
      box.style.boxShadow = '0 0 14px rgba(245, 158, 11, 0.45)';
    } else if (status === 'nomatch') {
      box.style.borderColor = '#ec4899';
      box.style.boxShadow = '0 0 14px rgba(236, 72, 153, 0.45)';
    } else {
      box.style.borderColor = '';
      box.style.boxShadow = '';
    }
  }
}
