import { AgentActivation } from '../types.ts';

export class TelemetryFeed {
  private container: HTMLElement;
  private eventSource: EventSource | null = null;
  private activations: AgentActivation[] = [];
  public onActivation?: (act: AgentActivation) => void;
  public onSelectPaths?: (paths: string[]) => void;
  public onSelectActivation?: (act: AgentActivation) => void;
  public onCollapseToggle?: (collapsed: boolean) => void;
  private isCollapsed: boolean = false;

  private maxActivations: number = 7;

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
    while (this.activations.length > this.maxActivations) {
      this.activations.pop();
    }
    this.render();
  }

  public render() {
    const cardsHtml =
      this.activations.length === 0
        ? `<div class="telemetry-idle">Awaiting agent tool sessions...</div>`
        : this.activations
            .map((act, idx) => {
              const color = act.client_color || '#38bdf8';
              const client = act.client_name || act.client_id || 'Agent';
              const queryOrPaths = act.query
                ? `"${act.query.slice(0, 42)}"`
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
            <span>SESSIONS</span>
            <span style="font-size: 10px; color: var(--text-muted); font-weight: 500;">(${this.activations.length})</span>
          </div>
        </div>
        <div class="telemetry-stream" style="flex: 1; overflow: hidden; display: flex; flex-direction: column; gap: 8px;">
          ${cardsHtml}
        </div>
      </div>
    `;

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

  public setCollapsed(collapsed: boolean) {
    this.isCollapsed = collapsed;
    const hud = this.container.querySelector('#agent-hud');
    hud?.classList.toggle('collapsed', this.isCollapsed);
    this.container.classList.toggle('collapsed', this.isCollapsed);
  }

  public destroy() {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
  }
}
