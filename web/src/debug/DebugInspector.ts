/**
 * Debug Inspector for RCode E2E Tests
 *
 * Exposes a read-only JavaScript debug API on window.__RCODE_DEBUG__
 * that E2E tests can call to get instant visibility into app state.
 */

// ─── Types ───────────────────────────────────────────────────────────────────

export interface ComponentInfo {
  tag: string;
  dataComponent: string;
  dataTab: string;
  dataNodeId: string;
  dataActive: string;
  dataSessionId: string;
  dataRole: string;
  text: string;
  rect: { x: number; y: number; w: number; h: number };
  styles: Record<string, string>;
  children: number;
  visible: boolean;
  classes: string[];
}

export interface LayoutSnapshot {
  viewport: { width: number; height: number; devicePixelRatio: number };
  scroll: { x: number; y: number };
  documentHeight: number;
}

export interface AppSnapshot {
  timestamp: string;
  layout: LayoutSnapshot;
  route: 'welcome' | 'recent-projects' | 'session' | 'no-session';
  project: {
    activeId: string | null;
    activeName: string | null;
    activePath: string | null;
    totalProjects: number;
  };
  workspace: {
    sessions: Array<{ id: string; title: string; status: string; model_id: string; updated_at: string }>;
    activeSessionId: string | null;
    sessionCount: number;
    isLoading: boolean;
    sseStatus: string;
    messageCount: number;
  };
  explorer: {
    activeFilePath: string | null;
    focusedNodeId: string | null;
    treeNodes: number;
    expandedPaths: string[];
  };
  ui: {
    terminalOpen: boolean;
    settingsOpen: boolean;
    currentModel: string;
  };
  components: ComponentInfo[];
}

// ─── Constants ───────────────────────────────────────────────────────────────

const STYLE_PROPS = [
  'display',
  'visibility',
  'opacity',
  'position',
  'width',
  'height',
  'min-width',
  'max-width',
  'overflow',
  'overflow-x',
  'overflow-y',
  'padding',
  'margin',
  'gap',
  'font-size',
  'font-weight',
  'line-height',
  'color',
  'background-color',
  'border',
  'border-radius',
  'transform',
  'z-index',
  'pointer-events',
];

const MRU_KEY = 'rcode:active-project';

// ─── DebugInspector class ───────────────────────────────────────────────────

class DebugInspector {
  /**
   * Check if debug mode is active.
   */
  isDev(): boolean {
    return (
      typeof import.meta !== 'undefined' &&
      (import.meta as any).env?.DEV === true
    ) || typeof window !== 'undefined' && Boolean((window as any).__TAURI_DEBUG__);
  }

  /**
   * Get a full app snapshot from all available sources.
   */
  getSnapshot(): AppSnapshot {
    const layout = this.getLayout();
    const route = this.getRoute();
    const project = this.getProjectInfo();
    const workspace = this.getWorkspaceInfo();
    const explorer = this.getExplorerInfo();
    const ui = this.getUiInfo();
    const components = this.getComponents('[data-component]');

    return {
      timestamp: new Date().toISOString(),
      layout,
      route,
      project,
      workspace,
      explorer,
      ui,
      components,
    };
  }

  /**
   * Get info about a specific component by selector.
   */
  getComponent(selector: string): ComponentInfo | null {
    const elements = this.getComponents(selector);
    return elements.length > 0 ? elements[0] : null;
  }

  /**
   * Get all elements matching a selector and return ComponentInfo array.
   */
  getComponents(selector: string): ComponentInfo[] {
    if (typeof document === 'undefined') return [];

    try {
      const elements = document.querySelectorAll(selector);
      return Array.from(elements).map((el) => this.elementToComponentInfo(el));
    } catch {
      return [];
    }
  }

  /**
   * Get computed styles for an element matching the selector.
   */
  getStyles(selector: string): Record<string, string> {
    if (typeof document === 'undefined') return {};

    try {
      const el = document.querySelector(selector);
      if (!el) return {};

      const computed = window.getComputedStyle(el);
      const styles: Record<string, string> = {};
      for (const prop of STYLE_PROPS) {
        styles[prop] = computed.getPropertyValue(prop);
      }
      return styles;
    } catch {
      return {};
    }
  }

  /**
   * Get layout information about the viewport and document.
   */
  getLayout(): LayoutSnapshot {
    if (typeof document === 'undefined') {
      return {
        viewport: { width: 0, height: 0, devicePixelRatio: 1 },
        scroll: { x: 0, y: 0 },
        documentHeight: 0,
      };
    }

    return {
      viewport: {
        width: window.innerWidth,
        height: window.innerHeight,
        devicePixelRatio: window.devicePixelRatio || 1,
      },
      scroll: {
        x: window.scrollX,
        y: window.scrollY,
      },
      documentHeight: document.documentElement.scrollHeight,
    };
  }

  /**
   * Determine the current route based on what's rendered.
   */
  getRoute(): 'welcome' | 'recent-projects' | 'session' | 'no-session' {
    if (typeof document === 'undefined') return 'no-session';

    // Check for welcome screen (no projects)
    const welcomeEl = document.querySelector('[data-component="welcome-screen"]');
    if (welcomeEl) return 'welcome';

    // Check for recent projects view
    const recentProjectsEl = document.querySelector('[data-component="recent-projects"]');
    if (recentProjectsEl) return 'recent-projects';

    // Check for session view (has textarea or chat content)
    const sessionEl = document.querySelector('[data-component="session-view"]');
    if (sessionEl) return 'session';

    // Check for no-session placeholder
    const noSessionEl = document.querySelector('[data-component="no-session"]');
    if (noSessionEl) return 'no-session';

    // Fallback: check for any data-session-id to know we're in session context
    const sessionItems = document.querySelectorAll('[data-session-id]');
    if (sessionItems.length > 0) return 'session';

    return 'no-session';
  }

  /**
   * Get context value by name.
   * Note: SolidJS contexts are reactive and only accessible inside components,
   * so this reads from DOM attributes and localStorage as a proxy.
   */
  getContext(name: string): any {
    if (typeof document === 'undefined') return undefined;

    switch (name) {
      case 'project':
        return this.getProjectInfo();
      case 'workspace':
        return this.getWorkspaceInfo();
      case 'explorer':
        return this.getExplorerInfo();
      case 'ui':
        return this.getUiInfo();
      default:
        return undefined;
    }
  }

  /**
   * Find elements matching a data attribute query.
   * Query format: "data-component=explorer-tree" or "data-tab=sessions"
   */
  findElements(query: string): ComponentInfo[] {
    if (typeof document === 'undefined' || !query) return [];

    // Parse query like "data-component=explorer-tree"
    const match = query.match(/^(data-[\w-]+)=(.+)$/);
    if (!match) return [];

    const [, attrName, attrValue] = match;
    const selector = `[${attrName}="${attrValue}"]`;

    try {
      const elements = document.querySelectorAll(selector);
      return Array.from(elements).map((el) => this.elementToComponentInfo(el));
    } catch {
      return [];
    }
  }

  /**
   * Trigger a session refresh by dispatching a custom event.
   */
  triggerRefreshSessions(): void {
    window.dispatchEvent(new CustomEvent('rcode:debug-refresh-sessions'));
  }

  /**
   * Trigger a project switch by setting localStorage and dispatching event.
   */
  triggerProjectSwitch(projectId: string): void {
    localStorage.setItem(MRU_KEY, projectId);
    window.dispatchEvent(
      new CustomEvent('rcode:debug-switch-project', { detail: { projectId } }),
    );
  }

  /**
   * Get all visible toast notifications currently in the DOM.
   */
  getToasts(): Array<{ id: string; type: string; message: string; visible: boolean }> {
    const container = document.querySelector('[data-component="toast-container"]') || document.body;
    const toasts = container.querySelectorAll('[data-component="toast"], [role="alert"]');
    return Array.from(toasts).map((el) => ({
      id: el.getAttribute('data-toast-id') || '',
      type: el.getAttribute('data-toast-type') || el.className || '',
      message: el.textContent?.trim().slice(0, 200) || '',
      visible: (el as HTMLElement).offsetParent !== null,
    }));
  }

  /**
   * Get rendered messages from the transcript DOM.
   * Returns message metadata (role, has tool_call, has tool_result, text preview).
   */
  getMessages(): Array<{ role: string; hasToolCall: boolean; hasToolResult: boolean; textPreview: string; turnIndex: number }> {
    const messages = document.querySelectorAll('[data-component="message"]');
    return Array.from(messages).map((el, idx) => ({
      role: el.getAttribute('data-role') || el.getAttribute('data-turn-role') || 'unknown',
      hasToolCall: el.querySelector('[data-part="tool_call"]') !== null,
      hasToolResult: el.querySelector('[data-part="tool_result"]') !== null,
      textPreview: el.textContent?.trim().slice(0, 150) || '',
      turnIndex: idx,
    }));
  }

  /**
   * Get the current streaming state from the DOM.
   */
  getStreamingState(): { isStreaming: boolean; hasDraft: boolean; hasSkeleton: boolean; hasAbort: boolean; toolCalls: Array<{ name: string; status: string }> } {
    const streamingShell = document.querySelector('[data-streaming="streaming"], [data-streaming="optimistic"]');
    const draftParts = document.querySelector('[data-component="draft-parts"]');
    const skeleton = document.querySelector('[data-component="skeleton-content"]');
    const abortBtn = document.querySelector('[data-component="shell-abort"]');
    const toolCards = document.querySelectorAll('[data-component="streaming-tool-call-card"]');

    return {
      isStreaming: streamingShell !== null,
      hasDraft: draftParts !== null,
      hasSkeleton: skeleton !== null,
      hasAbort: abortBtn !== null,
      toolCalls: Array.from(toolCards).map((el) => ({
        name: el.textContent?.trim().slice(0, 50) || '',
        status: el.getAttribute('data-status') || 'unknown',
      })),
    };
  }

  /**
   * Dispatch a custom event to toggle the settings panel.
   */
  triggerToggleSettings(open?: boolean): void {
    window.dispatchEvent(new CustomEvent('rcode:debug-toggle-settings', { detail: { open } }));
  }

  /**
   * Dispatch a custom event to toggle the terminal panel.
   */
  triggerToggleTerminal(open?: boolean): void {
    window.dispatchEvent(new CustomEvent('rcode:debug-toggle-terminal', { detail: { open } }));
  }

  /**
   * Dispatch a custom event to switch between sessions/explorer tab.
   */
  triggerSwitchTab(tab: 'sessions' | 'explorer'): void {
    window.dispatchEvent(new CustomEvent('rcode:debug-switch-tab', { detail: { tab } }));
  }

  /**
   * Dispatch a custom event to abort the current streaming response.
   */
  triggerAbort(): void {
    window.dispatchEvent(new CustomEvent('rcode:debug-abort'));
  }

  // ─── Private helpers ───────────────────────────────────────────────────────

  private elementToComponentInfo(el: Element): ComponentInfo {
    const rect = el.getBoundingClientRect();
    const computed = window.getComputedStyle(el);
    const textContent = el.textContent || '';

    return {
      tag: el.tagName.toLowerCase(),
      dataComponent: el.getAttribute('data-component') || '',
      dataTab: el.getAttribute('data-tab') || '',
      dataNodeId: el.getAttribute('data-node-id') || '',
      dataActive: el.getAttribute('data-active') || '',
      dataSessionId: el.getAttribute('data-session-id') || '',
      dataRole: el.getAttribute('data-role') || '',
      text: textContent.substring(0, 100),
      rect: {
        x: rect.x,
        y: rect.y,
        w: rect.width,
        h: rect.height,
      },
      styles: this.getElementStyles(el),
      children: el.children.length,
      visible: this.isElementVisible(el, computed),
      classes: Array.from(el.classList),
    };
  }

  private getElementStyles(el: Element): Record<string, string> {
    const computed = window.getComputedStyle(el);
    const styles: Record<string, string> = {};
    for (const prop of STYLE_PROPS) {
      styles[prop] = computed.getPropertyValue(prop);
    }
    return styles;
  }

  private isElementVisible(el: Element, computed: CSSStyleDeclaration): boolean {
    const display = computed.getPropertyValue('display');
    const visibility = computed.getPropertyValue('visibility');
    const opacity = computed.getPropertyValue('opacity');

    if (display === 'none') return false;
    if (visibility === 'hidden') return false;
    if (visibility === 'collapse') return false;
    if (parseFloat(opacity) === 0) return false;

    // Check if element has zero size
    const rect = el.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) return false;

    return true;
  }

  private getProjectInfo() {
    let activeId = null;
    let activeName = null;
    let activePath = null;

    try {
      activeId = localStorage.getItem(MRU_KEY);
    } catch {}

    // Try to get project name from DOM
    const projectNameEl = document.querySelector('[data-component="project-name"]');
    if (projectNameEl) {
      activeName = projectNameEl.textContent?.trim() || null;
    }

    // Try to get project path from DOM
    const projectPathEl = document.querySelector('[data-component="project-path"]');
    if (projectPathEl) {
      activePath = projectPathEl.getAttribute('title') || projectPathEl.textContent?.trim() || null;
    }

    // Count total projects from DOM
    const projectItems = document.querySelectorAll('[data-component="project-item"]');
    const totalProjects = projectItems.length || 0;

    return {
      activeId,
      activeName,
      activePath,
      totalProjects,
    };
  }

  private getWorkspaceInfo() {
    // Get sessions from DOM
    const sessionItems = document.querySelectorAll('[data-session-id]');
    const sessions: Array<{
      id: string;
      title: string;
      status: string;
      model_id: string;
      updated_at: string;
    }> = [];

    sessionItems.forEach((el) => {
      const id = el.getAttribute('data-session-id') || '';
      const titleEl = el.querySelector('[data-component="session-title"]');
      const title = titleEl?.textContent?.trim() || 'Untitled';
      const status = el.getAttribute('data-status') || 'unknown';
      const model_id = el.getAttribute('data-model') || '';
      const updated_at = el.getAttribute('data-updated') || new Date().toISOString();

      sessions.push({ id, title, status, model_id, updated_at });
    });

    // Determine active session from DOM
    const activeSessionEl = document.querySelector('[data-session-id][data-active="true"]');
    const activeSessionId = activeSessionEl?.getAttribute('data-session-id') || null;

    // Check loading state
    const loadingEl = document.querySelector('[data-component="loading-indicator"]');
    const isLoading = loadingEl !== null && !loadingEl.hasAttribute('hidden');

    // Check SSE status
    let sseStatus = 'idle';
    const sseEl = document.querySelector('[data-component="sse-status"]');
    if (sseEl) {
      sseStatus = sseEl.getAttribute('data-status') || 'idle';
    }

    // Count messages
    const messageItems = document.querySelectorAll('[data-component="message"]');
    const messageCount = messageItems.length;

    return {
      sessions,
      activeSessionId,
      sessionCount: sessions.length,
      isLoading,
      sseStatus,
      messageCount,
    };
  }

  private getExplorerInfo() {
    // Get active file path
    let activeFilePath: string | null = null;
    const activeFileEl = document.querySelector('[data-component="active-file"]');
    if (activeFileEl) {
      activeFilePath = activeFileEl.getAttribute('data-path') || activeFileEl.textContent?.trim() || null;
    }

    // Get focused node
    let focusedNodeId: string | null = null;
    const focusedEl = document.querySelector('[data-node-id][data-focused="true"]');
    if (focusedEl) {
      focusedNodeId = focusedEl.getAttribute('data-node-id');
    }

    // Count tree nodes
    const treeNodes = document.querySelectorAll('[data-node-id]').length;

    // Get expanded paths from DOM
    const expandedPaths: string[] = [];
    const expandedEls = document.querySelectorAll('[data-expanded="true"][data-path]');
    expandedEls.forEach((el) => {
      const path = el.getAttribute('data-path');
      if (path) expandedPaths.push(path);
    });

    return {
      activeFilePath,
      focusedNodeId,
      treeNodes,
      expandedPaths,
    };
  }

  private getUiInfo() {
    // Check terminal state
    const terminalEl = document.querySelector('[data-component="terminal"]');
    let terminalOpen = false;
    if (terminalEl) {
      const style = window.getComputedStyle(terminalEl);
      terminalOpen = style.display !== 'none' && style.visibility !== 'hidden';
    }

    // Check settings state
    const settingsEl = document.querySelector('[data-component="settings"]');
    let settingsOpen = false;
    if (settingsEl) {
      settingsOpen = settingsEl.hasAttribute('open') || !settingsEl.hasAttribute('hidden');
    }

    // Get current model
    let currentModel = '';
    const modelEl = document.querySelector('[data-component="current-model"]');
    if (modelEl) {
      currentModel = modelEl.getAttribute('data-model') || modelEl.textContent?.trim() || '';
    }

    return {
      terminalOpen,
      settingsOpen,
      currentModel,
    };
  }
}

// ─── Install function ───────────────────────────────────────────────────────

export function install(): void {
  if (typeof window === 'undefined') return;
  if ((window as any).__RCODE_DEBUG__) return; // already installed

  const inspector = new DebugInspector();
  (window as any).__RCODE_DEBUG__ = inspector;
  console.log('[RCode Debug] Inspector installed. Use window.__RCODE_DEBUG__');
}
