import { NodeData } from '../types.ts';
import { hexToCss } from '../lib/colors.ts';

export class Inspector {
  private container: HTMLElement;
  private currentNode: NodeData | null = null;
  private isSourceOpen: boolean = false;
  private isEgoFocused: boolean = false;
  private currentSourceContent: string = '';

  public onFocusEgo?: (node: NodeData) => void;
  public onUnfocus?: () => void;
  public onReadSource?: (node: NodeData) => void;
  public onClose?: () => void;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  public showSourceCode(content: string, filePath?: string, startLine?: number, language?: string) {
    this.isSourceOpen = true;
    this.currentSourceContent = content;

    const viewer = this.container.querySelector('#inspector-code-viewer') as HTMLElement;
    const body = this.container.querySelector('#code-viewer-body') as HTMLElement;
    const pathEl = this.container.querySelector('#code-viewer-path') as HTMLElement;
    const langEl = this.container.querySelector('#code-viewer-lang') as HTMLElement;
    const readBtn = this.container.querySelector('#read-source-btn') as HTMLElement;

    if (!viewer || !body) return;

    viewer.style.display = 'flex';

    if (pathEl) {
      pathEl.textContent = filePath ? `${filePath}${startLine ? `:${startLine}` : ''}` : (this.currentNode?.path || 'Source Definition');
      pathEl.title = pathEl.textContent;
    }

    if (langEl) {
      const ext = filePath ? filePath.split('.').pop() || '' : (language || 'txt');
      langEl.textContent = ext.toUpperCase();
    }

    // Format lines with line numbers
    const lines = content.split('\n');
    const startNum = startLine || 1;
    const formatted = lines
      .map((line, idx) => {
        const num = (startNum + idx).toString().padStart(4, ' ');
        return `<span style="color: rgba(56, 189, 248, 0.45); user-select: none; margin-right: 12px; font-variant-numeric: tabular-nums;">${num}</span>${this.escapeHtml(line)}`;
      })
      .join('\n');

    body.innerHTML = formatted;

    if (readBtn) {
      readBtn.classList.add('active');
      const span = readBtn.querySelector('span');
      if (span) span.textContent = 'Hide Source';
    }
  }

  public hideSourceCode() {
    this.isSourceOpen = false;
    const viewer = this.container.querySelector('#inspector-code-viewer') as HTMLElement;
    const readBtn = this.container.querySelector('#read-source-btn') as HTMLElement;

    if (viewer) viewer.style.display = 'none';
    if (readBtn) {
      readBtn.classList.remove('active');
      const span = readBtn.querySelector('span');
      if (span) span.textContent = 'Read Source';
    }
  }

  public showNode(node: NodeData | null, isEgoFocused: boolean = false) {
    this.currentNode = node;
    this.isEgoFocused = isEgoFocused;

    if (!node) {
      this.isSourceOpen = false;
      this.currentSourceContent = '';
      this.container.innerHTML = '';
      this.container.style.display = 'none';
      return;
    }

    this.container.style.display = 'block';
    const colorHex = hexToCss(node.colorRgb);

    this.container.innerHTML = `
      <div class="inspector-panel glass-panel">
        <div class="inspector-summary-row">
          <div class="inspector-meta">
            <div>
              <span class="node-badge" style="border-color: ${colorHex}; color: ${colorHex}">
                ${node.entityType}
              </span>
            </div>
            <div class="node-title">${node.title || node.path.split('/').pop() || node.path}</div>
            <div class="node-path" title="${node.path}">${node.path}</div>
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
            <button class="hud-action-btn ${this.isSourceOpen ? 'active' : ''}" id="read-source-btn" title="Toggle source code viewer" style="display: flex; align-items: center; gap: 6px;">
              <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path>
                <polyline points="14 2 14 8 20 8"></polyline>
                <line x1="16" y1="13" x2="8" y2="13"></line>
                <line x1="16" y1="17" x2="8" y2="17"></line>
              </svg>
              <span>${this.isSourceOpen ? 'Hide Source' : 'Read Source'}</span>
            </button>

            ${
              this.isEgoFocused
                ? `
              <button class="hud-action-btn active" id="unfocus-ego-btn" title="Exit ego subgraph and return to full graph" style="display: flex; align-items: center; gap: 6px; background: rgba(239, 68, 68, 0.2); border-color: rgba(239, 68, 68, 0.45); color: #f87171;">
                <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="9 14 4 9 9 4"></polyline>
                  <path d="M20 20v-7a4 4 0 0 0-4-4H4"></path>
                </svg>
                <span>Unfocus Subgraph</span>
              </button>
            `
                : `
              <button class="hud-action-btn" id="focus-ego-btn" title="Focus 2-hop ego subgraph" style="display: flex; align-items: center; gap: 6px;">
                <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="11" cy="11" r="8"/>
                  <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                </svg>
                <span>Focus Subgraph</span>
              </button>
            `
            }

            <button class="close-btn" id="inspector-close" title="${this.isEgoFocused ? 'Close and Unfocus Graph' : 'Dismiss'}" style="display: flex; align-items: center; justify-content: center; width: 22px; height: 22px; padding: 0;">
              <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <line x1="18" y1="6" x2="6" y2="18"/>
                <line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>
        </div>

        <!-- Row 2: Source Code Viewer positioned below in stylish translucent blue -->
        <div id="inspector-code-viewer" class="inspector-code-viewer" style="display: ${this.isSourceOpen ? 'flex' : 'none'};">
          <div class="code-viewer-header">
            <div class="code-viewer-path" id="code-viewer-path"></div>
            <div class="code-viewer-actions">
              <span class="code-lang-tag" id="code-viewer-lang"></span>
              <button class="code-copy-btn" id="code-copy-btn" title="Copy source code">
                <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                </svg>
                <span>Copy</span>
              </button>
            </div>
          </div>
          <div class="code-viewer-body" id="code-viewer-body"></div>
        </div>
      </div>
    `;

    // Bind event listeners
    const closeBtn = this.container.querySelector('#inspector-close') as HTMLButtonElement;
    closeBtn?.addEventListener('click', () => {
      this.onClose?.();
      if (this.isEgoFocused) {
        this.onUnfocus?.();
      }
      this.showNode(null);
    });

    const focusBtn = this.container.querySelector('#focus-ego-btn') as HTMLButtonElement;
    focusBtn?.addEventListener('click', () => {
      this.onFocusEgo?.(node);
    });

    const unfocusBtn = this.container.querySelector('#unfocus-ego-btn') as HTMLButtonElement;
    unfocusBtn?.addEventListener('click', () => {
      this.onUnfocus?.();
    });

    const readBtn = this.container.querySelector('#read-source-btn') as HTMLButtonElement;
    readBtn?.addEventListener('click', () => {
      if (this.isSourceOpen) {
        this.hideSourceCode();
      } else {
        this.onReadSource?.(node);
      }
    });

    const copyBtn = this.container.querySelector('#code-copy-btn') as HTMLButtonElement;
    copyBtn?.addEventListener('click', () => {
      if (this.currentSourceContent) {
        navigator.clipboard.writeText(this.currentSourceContent);
        const span = copyBtn.querySelector('span');
        if (span) {
          const prev = span.textContent;
          span.textContent = 'Copied!';
          setTimeout(() => {
            span.textContent = prev;
          }, 1500);
        }
      }
    });
  }

  private escapeHtml(str: string): string {
    return str
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }
}
