import { AgentActivation } from '../types.ts';

interface AgentStats {
  name: string;
  color: string;
  count: number;
  lastActive: number;
}

interface ToolStats {
  tool: string;
  count: number;
  totalDuration: number;
}

export class TelemetryFeed {
  private container: HTMLElement;
  private eventSource: EventSource | null = null;
  private activations: AgentActivation[] = [];
  public onActivation?: (act: AgentActivation) => void;
  public onSelectPaths?: (paths: string[]) => void;
  public onSelectActivation?: (act: AgentActivation) => void;
  public onCollapseToggle?: (collapsed: boolean) => void;
  private isCollapsed: boolean = false;

  private streamCollapsed: boolean = false;
  private agentsCollapsed: boolean = false;
  private metricsCollapsed: boolean = true;

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
    if (this.activations.length > 30) {
      this.activations.pop();
    }
    this.render();
  }

  public render() {
    // 1. Compute Agent Swarm stats
    const agentMap = new Map<string, AgentStats>();
    // Pre-populate core swarm clients
    const coreAgents = [
      { name: 'antigravity', color: '#38bdf8' },
      { name: 'claude', color: '#f97316' },
      { name: 'gemini', color: '#ec4899' },
      { name: 'cursor', color: '#f59e0b' },
      { name: 'roo', color: '#10b981' },
    ];
    for (const a of coreAgents) {
      agentMap.set(a.name.toLowerCase(), { name: a.name, color: a.color, count: 0, lastActive: 0 });
    }

    // 2. Compute Tool Metrics
    const toolMap = new Map<string, ToolStats>();

    const now = performance.now();
    for (const act of this.activations) {
      const clientName = (act.client_name || act.client_id || 'default').toLowerCase();
      const clientColor = act.client_color || '#a855f7';
      const existingAgent = agentMap.get(clientName);
      if (existingAgent) {
        existingAgent.count++;
        existingAgent.lastActive = now;
      } else {
        agentMap.set(clientName, {
          name: act.client_name || act.client_id || 'agent',
          color: clientColor,
          count: 1,
          lastActive: now,
        });
      }

      const toolName = act.tool || 'unknown';
      const existingTool = toolMap.get(toolName) || { tool: toolName, count: 0, totalDuration: 0 };
      existingTool.count++;
      existingTool.totalDuration += act.duration_ms || 0;
      toolMap.set(toolName, existingTool);
    }

    // 3. Render Cards HTML
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

    // 4. Render Agents HTML
    const activeAgentsList = Array.from(agentMap.values()).filter((a) => a.count > 0 || coreAgents.some((c) => c.name.toLowerCase() === a.name.toLowerCase()));
    const agentsHtml = activeAgentsList
      .map((a) => {
        const isActive = a.count > 0;
        return `
          <div class="agent-item-row">
            <div class="agent-item-left">
              <span class="agent-item-dot" style="background: ${a.color}; box-shadow: 0 0 6px ${a.color};"></span>
              <span style="font-weight: 600; text-transform: capitalize; color: #f1f5f9;">${a.name}</span>
            </div>
            <div style="display: flex; align-items: center; gap: 6px;">
              <span style="font-size: 10px; color: ${isActive ? '#38bdf8' : 'var(--text-muted)'}; font-weight: 600;">
                ${a.count} calls
              </span>
              <span style="font-size: 9px; padding: 1px 4px; border-radius: 3px; background: ${isActive ? 'rgba(16, 185, 129, 0.15)' : 'rgba(255, 255, 255, 0.05)'}; color: ${isActive ? '#10b981' : '#64748b'};">
                ${isActive ? 'Active' : 'Standby'}
              </span>
            </div>
          </div>
        `;
      })
      .join('');

    // 5. Render Tools HTML
    const totalCalls = this.activations.length;
    const sortedTools = Array.from(toolMap.values()).sort((a, b) => b.count - a.count);
    const toolsHtml = sortedTools.length === 0
      ? `<div class="telemetry-idle" style="margin: 10px 0;">No tool calls recorded yet</div>`
      : sortedTools
          .map((t) => {
            const pct = totalCalls > 0 ? Math.round((t.count / totalCalls) * 100) : 0;
            const avgMs = Math.round(t.totalDuration / Math.max(1, t.count));
            return `
              <div class="tool-metric-row">
                <div class="tool-metric-header">
                  <span>${t.tool}</span>
                  <span style="color: var(--text-muted); font-size: 10px;">${t.count} (${avgMs}ms)</span>
                </div>
                <div class="tool-metric-bar">
                  <div class="tool-metric-fill" style="width: ${pct}%;"></div>
                </div>
              </div>
            `;
          })
          .join('');

    this.container.innerHTML = `
      <div class="agent-hud glass-panel ${this.isCollapsed ? 'collapsed' : ''}" id="agent-hud">
        <div class="agent-hud-header">
          <div class="agent-hud-title">
            <span class="telemetry-live-dot pulse"></span>
            <span>MULTI-AGENT SUBSURFACE</span>
          </div>
          <button id="hud-collapse-btn" class="hud-toggle-btn" title="Toggle Sidebar">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <line x1="18" y1="6" x2="6" y2="18"></line>
              <line x1="6" y1="6" x2="18" y2="18"></line>
            </svg>
          </button>
        </div>

        <!-- Section 1: Activation Stream -->
        <div class="hud-collapsible-section stream-section ${this.streamCollapsed ? 'collapsed' : ''}" id="section-stream">
          <div class="hud-collapsible-header" id="header-stream" title="Toggle Live Activations">
            <div class="hud-section-title">
              <span class="telemetry-live-dot"></span>
              <span>LIVE ACTIVATIONS</span>
            </div>
            <div style="display: flex; align-items: center; gap: 6px;">
              <span style="font-size: 10px; color: var(--text-muted)">(${this.activations.length})</span>
              <svg class="hud-collapsible-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </div>
          </div>
          <div class="hud-collapsible-body" id="body-stream">
            <div class="telemetry-stream">
              ${cardsHtml}
            </div>
          </div>
        </div>

        <!-- Section 2: Swarm Agents Registry -->
        <div class="hud-collapsible-section ${this.agentsCollapsed ? 'collapsed' : ''}" id="section-agents">
          <div class="hud-collapsible-header" id="header-agents" title="Toggle Swarm Agents">
            <div class="hud-section-title">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="#38bdf8" stroke-width="2">
                <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"></path>
                <circle cx="9" cy="7" r="4"></circle>
                <path d="M23 21v-2a4 4 0 0 0-3-3.87"></path>
                <path d="M16 3.13a4 4 0 0 1 0 7.75"></path>
              </svg>
              <span>SWARM AGENTS</span>
            </div>
            <div style="display: flex; align-items: center; gap: 6px;">
              <span style="font-size: 10px; color: var(--neon-emerald)">${activeAgentsList.filter(a => a.count > 0).length} active</span>
              <svg class="hud-collapsible-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </div>
          </div>
          <div class="hud-collapsible-body" id="body-agents">
            ${agentsHtml}
          </div>
        </div>

        <!-- Section 3: Tool Invocation Metrics -->
        <div class="hud-collapsible-section ${this.metricsCollapsed ? 'collapsed' : ''}" id="section-tools">
          <div class="hud-collapsible-header" id="header-tools" title="Toggle Tool Metrics">
            <div class="hud-section-title">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="#a855f7" stroke-width="2">
                <line x1="18" y1="20" x2="18" y2="10"></line>
                <line x1="12" y1="20" x2="12" y2="4"></line>
                <line x1="6" y1="20" x2="6" y2="14"></line>
              </svg>
              <span>TOOL METRICS</span>
            </div>
            <div style="display: flex; align-items: center; gap: 6px;">
              <span style="font-size: 10px; color: var(--text-muted)">${sortedTools.length} tools</span>
              <svg class="hud-collapsible-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </div>
          </div>
          <div class="hud-collapsible-body" id="body-tools">
            ${toolsHtml}
          </div>
        </div>
      </div>
    `;

    // Wire main sidebar collapse toggle
    const collapseBtn = this.container.querySelector('#hud-collapse-btn');
    collapseBtn?.addEventListener('click', () => {
      this.setCollapsed(!this.isCollapsed);
      this.onCollapseToggle?.(this.isCollapsed);
    });

    // Wire individual section collapsibles
    const headerStream = this.container.querySelector('#header-stream');
    const sectionStream = this.container.querySelector('#section-stream');
    headerStream?.addEventListener('click', () => {
      this.streamCollapsed = !this.streamCollapsed;
      sectionStream?.classList.toggle('collapsed', this.streamCollapsed);
    });

    const headerAgents = this.container.querySelector('#header-agents');
    const sectionAgents = this.container.querySelector('#section-agents');
    headerAgents?.addEventListener('click', () => {
      this.agentsCollapsed = !this.agentsCollapsed;
      sectionAgents?.classList.toggle('collapsed', this.agentsCollapsed);
    });

    const headerTools = this.container.querySelector('#header-tools');
    const sectionTools = this.container.querySelector('#section-tools');
    headerTools?.addEventListener('click', () => {
      this.metricsCollapsed = !this.metricsCollapsed;
      sectionTools?.classList.toggle('collapsed', this.metricsCollapsed);
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
