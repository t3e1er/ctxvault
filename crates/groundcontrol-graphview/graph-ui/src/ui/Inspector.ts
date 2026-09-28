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
          <button class="hud-action-btn" id="focus-ego-btn" title="Focus 2-hop ego subgraph">
            🔍 Subgraph Focus
          </button>
          <button class="close-btn" id="inspector-close" title="Dismiss">×</button>
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
