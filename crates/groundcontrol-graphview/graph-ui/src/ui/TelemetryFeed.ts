import { AgentActivation } from '../types.ts';

export class TelemetryFeed {
  private container: HTMLElement;
  private eventSource: EventSource | null = null;
  private activations: AgentActivation[] = [];
  public onActivation?: (act: AgentActivation) => void;
  public onSelectPaths?: (paths: string[]) => void;
  public onSelectActivation?: (act: AgentActivation) => void;
  private isCollapsed: boolean = false;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  public recordActivation(act: AgentActivation) {
    this.addActivation(act);
  }

  public start() {
    if (this.eventSource) return;

    this.eventSource = new EventSource('/api/events/activations');

    this.eventSource.addEventListener('activation', (event) => {
      try {
        const act: AgentActivation = JSON.parse(event.data);
        this.addActivation(act);
        this.onActivation?.(act);
      } catch (err) {
        console.warn('Failed to parse SSE activation:', err);
      }
    });

    this.render();
  }

  private addActivation(act: AgentActivation) {
    this.activations.unshift(act);
    if (this.activations.length > 25) {
      this.activations.pop();
    }
    this.render();
  }

  public render() {
    const cardsHtml =
      this.activations.length === 0
        ? `<div class="telemetry-idle">Awaiting agent swarm activity...</div>`
        : this.activations
            .map((act, idx) => {
              const color = act.client_color || '#38bdf8';
              const client = act.client_name || act.client_id || 'Agent';
              const queryOrPaths = act.query
                ? `"${act.query.slice(0, 40)}"`
                : act.paths.length > 0
                ? act.paths.map((p) => p.split(/[/\\]/).pop()).join(', ')
                : 'no target paths';

              return `
                <div class="telemetry-card" data-idx="${idx}" style="cursor: pointer;">
                  <div class="telemetry-card-top">
                    <span class="agent-tag" style="background: ${color}22; color: ${color}; border-color: ${color}55">
                      ${client}
                    </span>
                    <span class="telemetry-tool">${act.tool}</span>
                    <span class="telemetry-time">${Math.round(act.duration_ms)}ms</span>
                  </div>
                  <div class="telemetry-card-desc">${queryOrPaths}</div>
                </div>
              `;
            })
            .join('');

    this.container.innerHTML = `
      <div class="agent-hud glass-panel ${this.isCollapsed ? 'collapsed' : ''}" id="agent-hud">
        <div class="agent-hud-header">
          <div class="agent-hud-title">
            <span class="telemetry-live-dot pulse"></span>
            <span>MULTI-AGENT ACTIVATIONS</span>
            <span style="font-size: 10px; color: var(--text-muted)">(${this.activations.length})</span>
          </div>
          <button id="hud-collapse-btn" class="hud-toggle-btn" title="Toggle Agent Activations">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <line x1="18" y1="6" x2="6" y2="18"></line>
              <line x1="6" y1="6" x2="18" y2="18"></line>
            </svg>
          </button>
        </div>
        <div class="telemetry-stream">
          ${cardsHtml}
        </div>
      </div>
    `;

    // Wire collapse toggle
    const collapseBtn = this.container.querySelector('#hud-collapse-btn');
    collapseBtn?.addEventListener('click', () => {
      this.isCollapsed = !this.isCollapsed;
      const hud = this.container.querySelector('#agent-hud');
      hud?.classList.toggle('collapsed', this.isCollapsed);
    });

    // Wire card clicks
    this.container.querySelectorAll('.telemetry-card').forEach((card) => {
      card.addEventListener('click', () => {
        const idx = Number(card.getAttribute('data-idx'));
        const act = this.activations[idx];
        if (act) {
          this.onSelectActivation?.(act);
          if (act.paths && act.paths.length > 0) {
            this.onSelectPaths?.(act.paths);
          }
        }
      });
    });
  }

  public destroy() {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
  }
}
