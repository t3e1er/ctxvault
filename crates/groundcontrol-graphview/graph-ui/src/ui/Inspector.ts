import { NodeData } from '../types.ts';
import { hexToCss } from '../lib/colors.ts';

export class Inspector {
  private container: HTMLElement;
  public onFocusEgo?: (node: NodeData) => void;
  public onReadSource?: (node: NodeData) => void;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  public showSourceCode(content: string, filePath?: string, startLine?: number) {
    const viewer = this.container.querySelector('#inspector-code-viewer') as HTMLElement;
    if (!viewer) return;
    viewer.style.display = 'block';
    viewer.textContent = (filePath ? `// ${filePath}${startLine ? `:${startLine}` : ''}\n\n` : '') + content;
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
          <button class="hud-action-btn" id="read-source-btn" title="Read source code or definition" style="display: flex; align-items: center; gap: 6px;">
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path>
              <polyline points="14 2 14 8 20 8"></polyline>
              <line x1="16" y1="13" x2="8" y2="13"></line>
              <line x1="16" y1="17" x2="8" y2="17"></line>
            </svg>
            <span>Read Source</span>
          </button>
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
        <div id="inspector-code-viewer" class="inspector-code-viewer" style="display: none; width: 100%; margin-top: 10px; max-height: 220px; overflow: auto; background: rgba(0,0,0,0.6); border: 1px solid rgba(255,255,255,0.1); border-radius: 6px; padding: 10px; font-family: monospace; font-size: 11px; white-space: pre; color: #e2e8f0;"></div>
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

    const readBtn = this.container.querySelector('#read-source-btn') as HTMLButtonElement;
    readBtn?.addEventListener('click', () => {
      this.onReadSource?.(node);
    });
  }
}
