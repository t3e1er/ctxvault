import { NodeData } from '../types.ts';
import { hexToCss } from '../lib/colors.ts';

export class Inspector {
  private container: HTMLElement;
  public onFocusEgo?: (node: NodeData) => void;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  public showNode(node: NodeData | null) {
    if (!node) {
      this.container.innerHTML = '';
      this.container.style.display = 'none';
      return;
    }

    this.container.style.display = 'block';
    const colorHex = hexToCss(node.colorRgb);

    this.container.innerHTML = `
      <div class="inspector-panel glass-panel">
        <div class="inspector-meta">
          <div>
            <span class="node-badge" style="border-color: ${colorHex}; color: ${colorHex}">
              ${node.entityType}
            </span>
          </div>
          <div class="node-title">${node.title || node.path.split('/').pop() || node.path}</div>
          <div class="node-path">${node.path}</div>
        </div>

        <div class="node-metrics">
          <div class="metric-item">
            <span class="metric-val">${node.degree}</span>
            <span class="metric-lbl">Degree</span>
          </div>
          <div class="metric-item">
            <span class="metric-val">#${node.community}</span>
            <span class="metric-lbl">Community</span>
          </div>
          <div class="metric-item">
            <span class="metric-val">[${node.position.map((v) => Math.round(v)).join(', ')}]</span>
            <span class="metric-lbl">Coords</span>
          </div>
        </div>

        <div class="inspector-actions">
          <button class="hud-action-btn" id="focus-ego-btn" title="Focus 2-hop ego subgraph" style="display: flex; align-items: center; gap: 6px;">
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="11" cy="11" r="8"/>
              <line x1="21" y1="21" x2="16.65" y2="16.65"/>
            </svg>
            <span>Focus Subgraph</span>
          </button>
          <button class="close-btn" id="inspector-close" title="Dismiss" style="display: flex; align-items: center; justify-content: center; width: 22px; height: 22px; padding: 0;">
            <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>
    `;

    const closeBtn = this.container.querySelector('#inspector-close') as HTMLButtonElement;
    closeBtn?.addEventListener('click', () => {
      this.showNode(null);
    });

    const egoBtn = this.container.querySelector('#focus-ego-btn') as HTMLButtonElement;
    egoBtn?.addEventListener('click', () => {
      this.onFocusEgo?.(node);
    });
  }
}
