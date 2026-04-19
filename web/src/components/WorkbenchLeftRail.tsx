import { createSignal, createEffect, For, Show, createMemo } from "solid-js";
import { createVirtualizer } from "@tanstack/solid-virtual";
import type { Session } from "../stores";
import { fetchExplorerBootstrap, fetchExplorerTree, type ExplorerBootstrap, type TreeNode, type ExplorerFilter } from "../api/explorer";
import { renameSession } from "../api/session";
import { useProjectContext } from "../context/ProjectContext";
import { useWorkspace, type WorkspaceContextValue } from "../context/WorkspaceContext";
import { formatTime, formatCompactDate, getSessionGroup, type SessionGroup } from "../lib/dateUtils";
import { FilterBar, type FilterCounts } from "./left-rail/FilterBar";
import { TreeNodeRow, buildFlatTree } from "./left-rail/TreeNodeRow";
import { ExplorerContext } from "./left-rail/ExplorerContext";

// SUPPORTS_VIRTUALIZATION: true only in real DOM (not jsdom test environment)
// jsdom has window.matchMedia as a non-callable property, so we check typeof
const SUPPORTS_VIRTUALIZATION = typeof window !== "undefined" && typeof window.matchMedia === "function";

interface WorkbenchLeftRailProps {
  sessions: Session[];
  currentSessionId?: string;
  onSelect: (session: Session) => void;
  onNewSession: () => void;
  onSelectFile?: (path: string) => void;
  // T4.4: Active file path for highlight/reveal in explorer
  activeFilePath?: string | null;
  // Resizable column width
  width?: number;
}

type RailTab = "sessions" | "explorer";

interface SessionGrouped {
  group: SessionGroup;
  sessions: Session[];
}

export default function WorkbenchLeftRail(props: WorkbenchLeftRailProps) {
  const [activeTab, setActiveTab] = createSignal<RailTab>("sessions");
  const projectContext = useProjectContext();

  // Try to use workspace context, fall back to no-op if not available (tests)
  let workspaceCtx: WorkspaceContextValue | undefined;
  try {
    workspaceCtx = useWorkspace();
  } catch {
    workspaceCtx = undefined;
  }

  const safeWorkspace = (): WorkspaceContextValue | undefined => workspaceCtx;

  // Explorer state
  const [bootstrap, setBootstrap] = createSignal<ExplorerBootstrap | null>(null);
  const [rootChildren, setRootChildren] = createSignal<TreeNode[]>([]);
  const [expandedPaths, setExpandedPaths] = createSignal<Set<string>>(new Set());
  const [loadedPaths, setLoadedPaths] = createSignal<Map<string, TreeNode[]>>(new Map());
  const [explorerError, setExplorerError] = createSignal<string | null>(null);
  const [isLoading, setIsLoading] = createSignal(false);
  const [activeFilter, setActiveFilter] = createSignal<ExplorerFilter>("all");
  const [filterCounts, setFilterCounts] = createSignal<FilterCounts>({ changed: 0, staged: 0, untracked: 0, conflicted: 0 });

  // T4.3: Keyboard navigation state
  const [focusedNodeId, setFocusedNodeId] = createSignal<string | null>(null);

  // Normalize activeFilePath prop to accessor for context
  const activeFilePath = () => props.activeFilePath ?? null;

  // Phase 2: Inline rename state
  const [renamingSessionId, setRenamingSessionId] = createSignal<string | null>(null);
  const [renameValue, setRenameValue] = createSignal("");
  const [renameError, setRenameError] = createSignal<string | null>(null);

  // Session list — show limited recents by default, expand on demand
  const RECENT_LIMIT = 8;
  const [showAllSessions, setShowAllSessions] = createSignal(false);

  // Phase 2+3: Search state with debounce
  const [searchInput, setSearchInput] = createSignal("");
  const [debouncedSearch, setDebouncedSearch] = createSignal("");

  let searchTimeout: ReturnType<typeof setTimeout> | undefined;
  const handleSearchInput = (value: string) => {
    setSearchInput(value);
    if (searchTimeout) clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
      setDebouncedSearch(value);
      safeWorkspace()?.workspace.setSessionSearch?.(value);
    }, 300);
  };

  // Phase 2+3: Computed grouped sessions
  const groupedSessions = createMemo((): SessionGrouped[] => {
    const query = debouncedSearch().toLowerCase().trim();
    const sessions = props.sessions;
    const activeId = props.currentSessionId;

    const filtered = query
      ? sessions.filter(s => s.title?.toLowerCase().includes(query))
      : sessions;

    const activeSession = sessions.find(s => s.id === activeId);
    if (activeSession && !filtered.find(s => s.id === activeId)) {
      filtered.unshift(activeSession);
    }

    const groups: Record<SessionGroup, Session[]> = {
      "Today": [],
      "Yesterday": [],
      "This Week": [],
      "Older": [],
    };

    for (const session of filtered) {
      const group = getSessionGroup(session.updated_at);
      groups[group].push(session);
    }

    const result: SessionGrouped[] = [];
    for (const [group, sessions] of Object.entries(groups)) {
      if (sessions.length > 0) {
        result.push({ group: group as SessionGroup, sessions });
      }
    }
    return result;
  });

  const totalSessionCount = createMemo(() =>
    groupedSessions().reduce((sum, g) => sum + g.sessions.length, 0)
  );

  const visibleGroupedSessions = createMemo((): SessionGrouped[] => {
    const query = debouncedSearch().toLowerCase().trim();
    if (query || showAllSessions()) return groupedSessions();

    let remaining = RECENT_LIMIT;
    const result: SessionGrouped[] = [];
    for (const { group, sessions } of groupedSessions()) {
      if (remaining <= 0) break;
      const slice = sessions.slice(0, remaining);
      result.push({ group, sessions: slice });
      remaining -= slice.length;
    }
    return result;
  });

  // ── Scroll container ref (declared early so virtualizer can capture it) ─────
  let scrollContainerRef: HTMLDivElement | undefined;

  // ── Flat visible node list (memoized) ─────────────────────────────────────
  // Used for both keyboard navigation and the virtualizer.
  const flatNodes = createMemo(() =>
    buildFlatTree(rootChildren(), expandedPaths(), loadedPaths())
  );

  // ── Virtualizer (only active in real DOM) ─────────────────────────────────
  const virtualizer = createVirtualizer({
    get count() { return flatNodes().length; },
    getScrollElement: () => scrollContainerRef ?? null,
    estimateSize: () => 26,
    overscan: 10,
  });

  // ── Filter counts derived from flatNodes (no extra fetch needed) ──────────
  // We recompute when rootChildren / loadedPaths change.
  // We iterate the FULL loaded tree regardless of the active filter because
  // the backend always sends git status even when filter="all".
  createEffect(() => {
    const counts = { changed: 0, staged: 0, untracked: 0, conflicted: 0 };
    // Walk all loaded nodes (not just visible), so counts stay accurate
    function walk(nodes: TreeNode[]) {
      for (const n of nodes) {
        const g = n.git;
        if (g) {
          if (g.is_changed && !g.is_staged)  counts.changed++;
          if (g.is_staged)                    counts.staged++;
          if (g.is_untracked)                 counts.untracked++;
          if (g.is_conflicted)                counts.conflicted++;
        }
        const ch = loadedPaths().get(n.id);
        if (ch) walk(ch);
      }
    }
    walk(rootChildren());
    setFilterCounts({ ...counts });
  });

  function scrollNodeIntoView(nodeId: string) {
    if (SUPPORTS_VIRTUALIZATION) {
      const idx = flatNodes().findIndex(r => r.node.id === nodeId);
      if (idx >= 0) virtualizer.scrollToIndex(idx, { align: "nearest" });
    } else {
      const el = scrollContainerRef?.querySelector(`[data-node-id="${nodeId}"]`);
      if (el && typeof (el as HTMLElement).scrollIntoView === "function") {
        (el as HTMLElement).scrollIntoView({ behavior: "smooth", block: "nearest" });
      }
    }
  }

  // Fetch ExplorerBootstrap when project changes (for git_branch in header)
  createEffect(() => {
    const projectId = projectContext.activeProject()?.id;
    if (!projectId) {
      setBootstrap(null);
      return;
    }
    fetchExplorerBootstrap(undefined, projectId)
      .then(boot => setBootstrap(boot))
      .catch(() => setBootstrap(null));
  });

  // Phase 2: Inline rename handlers
  const startRename = (session: Session, e: MouseEvent) => {
    e.stopPropagation();
    setRenamingSessionId(session.id);
    setRenameValue(session.title || "");
    setRenameError(null);
  };

  const cancelRename = () => {
    setRenamingSessionId(null);
    setRenameValue("");
    setRenameError(null);
  };

  const submitRename = async () => {
    const sessionId = renamingSessionId();
    if (!sessionId) return;

    const newTitle = renameValue().trim();
    if (!newTitle) {
      setRenameError("Title cannot be empty");
      return;
    }

    try {
      await renameSession(sessionId, newTitle);
      const updated = props.sessions.map(s =>
        s.id === sessionId ? { ...s, title: newTitle } : s
      );
      safeWorkspace()?.workspace.setSessions?.(updated);
      cancelRename();
    } catch (err) {
      setRenameError(err instanceof Error ? err.message : "Failed to rename");
    }
  };

  const handleRenameKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void submitRename();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelRename();
    }
  };

  const toggleCompactMode = () => {
    const newValue = !(safeWorkspace()?.workspace.compactMode ?? false);
    safeWorkspace()?.workspace.setCompactMode?.(newValue);
  };

  const toggleGroup = (group: SessionGroup) => {
    safeWorkspace()?.workspace.toggleCollapsedGroup?.(group);
  };

  const isGroupCollapsed = (group: SessionGroup) =>
    (safeWorkspace()?.workspace.collapsedGroups ?? new Set()).has(group);

  // T4.3: Handle keyboard navigation using the memoized flat list
  function handleExplorerKeyDown(e: KeyboardEvent) {
    const nodes = flatNodes();
    if (nodes.length === 0) return;

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        {
          const idx = nodes.findIndex(r => r.node.id === focusedNodeId());
          const next = idx < 0 ? 0 : Math.min(idx + 1, nodes.length - 1);
          setFocusedNodeId(nodes[next].node.id);
          scrollNodeIntoView(nodes[next].node.id);
        }
        break;
      case "ArrowUp":
        e.preventDefault();
        {
          const idx = nodes.findIndex(r => r.node.id === focusedNodeId());
          const prev = idx < 0 ? 0 : Math.max(idx - 1, 0);
          setFocusedNodeId(nodes[prev].node.id);
          scrollNodeIntoView(nodes[prev].node.id);
        }
        break;
      case "Enter":
      case " ":
      case "ArrowRight":
        e.preventDefault();
        {
          const focusedId = focusedNodeId();
          if (!focusedId) return;
          const row = nodes.find(r => r.node.id === focusedId);
          if (!row) return;
          if (row.node.kind === "dir") {
            if (!expandedPaths().has(row.node.id)) loadChildren(row.node);
            handleToggle(row.node);
          } else {
            props.onSelectFile?.(row.node.relative_path);
          }
        }
        break;
      case "ArrowLeft":
        e.preventDefault();
        {
          const focusedId = focusedNodeId();
          if (!focusedId) return;
          const row = nodes.find(r => r.node.id === focusedId);
          if (!row) return;
          if (row.node.kind === "dir" && expandedPaths().has(row.node.id)) {
            handleToggle(row.node);
          }
        }
        break;
    }
  }

  // Fetch explorer data when tab is activated
  createEffect(() => {
    if (activeTab() === "explorer" && props.currentSessionId) {
      loadExplorerData(props.currentSessionId, activeFilter());
    } else if (activeTab() === "explorer" && projectContext.activeProject()) {
      loadExplorerData(undefined, activeFilter());
    }
  });

  // T4.4: Auto-expand parent directories when activeFilePath changes
  createEffect(() => {
    // activeFilePath prop is string | null — access reactively via props proxy
    const filePath = props.activeFilePath ?? null;
    if (!filePath || !props.currentSessionId) return;

    const parts = filePath.split('/');
    if (parts.length < 2) return;

    const dirsToExpand = parts.slice(0, -1);

    function expandDirs(nodeList: TreeNode[], pathParts: string[]): boolean {
      if (pathParts.length === 0) return true;
      const [currentPart, ...remainingParts] = pathParts;
      const node = nodeList.find(n => n.name === currentPart);
      if (!node || node.kind !== "dir") return false;

      if (!loadedPaths().has(node.id)) {
        loadChildren(node);
      }
      setExpandedPaths((prev) => {
        const next = new Set(prev);
        next.add(node.id);
        return next;
      });

      if (remainingParts.length > 0) {
        const children = loadedPaths().get(node.id) ?? [];
        return expandDirs(children, remainingParts);
      }
      return true;
    }

    expandDirs(rootChildren(), dirsToExpand);
  });

  // REQ-4.5: Set focusedNodeId and scroll when activeFilePath changes
  createEffect(() => {
    const filePath = props.activeFilePath;
    if (!filePath) return;

    function findNode(nodeList: TreeNode[], path: string): TreeNode | null {
      for (const node of nodeList) {
        if (node.kind !== "dir" && node.relative_path === path) return node;
        if (node.kind === "dir") {
          const ch = loadedPaths().get(node.id) ?? [];
          const found = findNode(ch, path);
          if (found) return found;
        }
      }
      return null;
    }

    const foundNode = findNode(rootChildren(), filePath);
    if (foundNode) {
      setFocusedNodeId(foundNode.id);
      if (activeTab() === "explorer") {
        const idx = flatNodes().findIndex(r => r.node.id === foundNode.id);
        if (idx >= 0) {
          setTimeout(() => scrollNodeIntoView(foundNode.id), 50);
        }
      }
    }
  });

  // REQ-4.5: Scroll to focusedNode when Explorer tab becomes active
  createEffect(() => {
    if (activeTab() === "explorer" && focusedNodeId()) {
      const nodeId = focusedNodeId();
      if (nodeId) {
        const idx = flatNodes().findIndex(r => r.node.id === nodeId);
        if (idx >= 0) {
          setTimeout(() => scrollNodeIntoView(nodeId), 50);
        }
      }
    }
  });

  // ── loadExplorerData: single fetch (no more double request) ───────────────
  async function loadExplorerData(sessionId: string | undefined, filter: ExplorerFilter) {
    setIsLoading(true);
    setExplorerError(null);
    const projectId = projectContext.activeProject()?.id ?? null;

    try {
      const [boot, tree] = await Promise.all([
        fetchExplorerBootstrap(sessionId, projectId),
        fetchExplorerTree(sessionId, ".", 1, filter, false, false, projectId),
      ]);
      setBootstrap(boot);
      setRootChildren(tree.children);
      // filterCounts are derived reactively via the createEffect above
    } catch (err) {
      console.error("Failed to load explorer:", err);
      setExplorerError(err instanceof Error ? err.message : "Failed to load explorer");
    } finally {
      setIsLoading(false);
    }
  }

  async function loadChildren(node: TreeNode) {
    const projectId = projectContext.activeProject()?.id ?? null;
    if (!props.currentSessionId && !projectId) return;

    try {
      const tree = await fetchExplorerTree(props.currentSessionId, node.path, 1, activeFilter(), false, false, projectId);
      setLoadedPaths((prev) => {
        const next = new Map(prev);
        next.set(node.id, tree.children);
        return next;
      });
    } catch (err) {
      console.error("Failed to load children:", err);
    }
  }

  function handleToggle(node: TreeNode) {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(node.id)) {
        next.delete(node.id);
      } else {
        next.add(node.id);
      }
      return next;
    });
  }

  function handleFilterChange(filter: ExplorerFilter) {
    setActiveFilter(filter);
    setExpandedPaths(new Set<string>());
    setLoadedPaths(new Map<string, TreeNode[]>());
    if (props.currentSessionId) {
      loadExplorerData(props.currentSessionId, filter);
    } else if (projectContext.activeProject()) {
      loadExplorerData(undefined, filter);
    }
  }

  return (
    <aside
      aria-label="Sessions and file explorer"
      data-component="workbench-left-rail"
      class="flex flex-col h-full shrink-0 border-r border-outline-variant/20"
      style={{
        width: `${props.width ?? 272}px`,
        "min-width": "200px",
        background: "var(--surface-container-low)",
      }}
    >
      {/* Workspace header */}
      <div class="shrink-0" style={{ background: "var(--surface-container-low)" }}>
        <Show
          when={projectContext.activeProject()}
          fallback={
            <div class="px-3 py-2.5 flex items-center justify-between gap-2">
              <div class="flex items-center gap-2 min-w-0">
                <span class="material-symbols-outlined" style={{ "font-size": "16px", color: "var(--outline)" }}>workspaces</span>
                <span class="text-xs italic truncate" style={{ color: "var(--outline)" }}>No workspace</span>
              </div>
              <button
                data-component="new-session-button"
                onClick={props.onNewSession}
                title="New session"
                class="shrink-0 w-7 h-7 flex items-center justify-center rounded-lg transition-all duration-150 active:scale-95"
                style={{ color: "var(--on-surface-variant)" }}
                onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--surface-container-high)"; (e.currentTarget as HTMLElement).style.color = "var(--primary)"; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; (e.currentTarget as HTMLElement).style.color = "var(--on-surface-variant)"; }}
              >
                <span class="material-symbols-outlined" style={{ "font-size": "18px" }}>edit_square</span>
              </button>
            </div>
          }
        >
          {(project) => (
            <div class="px-3 pt-2.5 pb-2">
              <div class="flex items-center gap-2">
                <div
                  class="w-7 h-7 rounded-lg shrink-0 flex items-center justify-center text-[12px] font-bold select-none"
                  style={{
                    background: "var(--primary-container)",
                    color: "var(--on-primary-container)",
                  }}
                >
                  {project().name.replace(/^[^a-zA-Z0-9]+/, "").charAt(0).toUpperCase() || "P"}
                </div>
                <div class="flex-1 min-w-0">
                  <div
                    class="text-[13px] font-semibold leading-snug truncate"
                    style={{ color: "var(--on-surface)" }}
                    title={project().canonical_path ?? project().name}
                  >
                    {project().name}
                  </div>
                  <Show when={bootstrap()?.git_branch}>
                    <div class="flex items-center gap-0.5 mt-0.5">
                      <span class="material-symbols-outlined" style={{ "font-size": "9px", color: "var(--secondary)" }}>call_split</span>
                      <span
                        class="text-[10px] truncate leading-tight"
                        style={{ color: "var(--secondary)", "max-width": "120px" }}
                        title={bootstrap()?.git_branch ?? ""}
                      >
                        {bootstrap()?.git_branch}
                      </span>
                    </div>
                  </Show>
                </div>
                <button
                  data-component="new-session-button"
                  onClick={props.onNewSession}
                  title="New session"
                  class="shrink-0 w-7 h-7 flex items-center justify-center rounded-lg transition-all duration-150 active:scale-95"
                  style={{ color: "var(--on-surface-variant)" }}
                  onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--surface-container-high)"; (e.currentTarget as HTMLElement).style.color = "var(--primary)"; }}
                  onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; (e.currentTarget as HTMLElement).style.color = "var(--on-surface-variant)"; }}
                >
                  <span class="material-symbols-outlined" style={{ "font-size": "18px" }}>edit_square</span>
                </button>
              </div>
            </div>
          )}
        </Show>

        {/* Divider */}
        <div class="h-px" style={{ background: "var(--outline-variant)", opacity: "0.4" }} />

        {/* Search + compact toggle */}
        <Show when={activeTab() === "sessions"}>
          <div class="px-3 py-2 flex items-center gap-1.5">
            <div class="relative flex-1">
              <span
                class="material-symbols-outlined absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none"
                style={{ "font-size": "13px", color: "var(--on-surface-variant)", opacity: "0.45" }}
              >search</span>
              <input
                type="text"
                aria-label="Filter sessions"
                placeholder="Search sessions…"
                value={searchInput()}
                onInput={(e) => handleSearchInput(e.currentTarget.value)}
                class="w-full text-[12px] pl-7 pr-3 py-1.5 rounded-lg focus:outline-none transition-all placeholder:opacity-40"
                style={{
                  background: "var(--surface-container)",
                  color: "var(--on-surface)",
                  "box-shadow": "inset 0 0 0 1px transparent",
                }}
                onFocus={(e) => { e.currentTarget.style.background = "var(--surface-container-high)"; e.currentTarget.style.boxShadow = `inset 0 0 0 1.5px var(--primary)`; }}
                onBlur={(e) => { e.currentTarget.style.background = "var(--surface-container)"; e.currentTarget.style.boxShadow = "inset 0 0 0 1px transparent"; }}
              />
            </div>
            <button
              onClick={toggleCompactMode}
              class="shrink-0 w-7 h-7 flex items-center justify-center rounded-lg transition-all duration-150"
              style={{ color: "var(--on-surface-variant)", opacity: "0.5" }}
              onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.opacity = "1"; (e.currentTarget as HTMLElement).style.background = "var(--surface-container-high)"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.opacity = "0.5"; (e.currentTarget as HTMLElement).style.background = "transparent"; }}
              title={safeWorkspace()?.workspace.compactMode ? "Normal view" : "Compact view"}
              aria-label={safeWorkspace()?.workspace.compactMode ? "Switch to normal mode" : "Switch to compact mode"}
            >
              <span class="material-symbols-outlined" style={{ "font-size": "15px" }}>
                {safeWorkspace()?.workspace.compactMode ? "density_small" : "density_medium"}
              </span>
            </button>
          </div>
        </Show>

        {/* Tab switcher */}
        <div
          role="tablist"
          data-component="rail-tabs"
          class="flex border-b"
          style={{ "border-color": "var(--outline-variant)", opacity: "1" }}
        >
          <button
            role="tab"
            aria-selected={activeTab() === "sessions"}
            aria-controls="sessions-panel"
            onClick={() => setActiveTab("sessions")}
            data-tab="sessions"
            class="flex-1 flex items-center justify-center gap-1.5 py-2 text-[11px] font-semibold transition-all duration-150 border-b-2 -mb-px"
            style={{
              color: activeTab() === "sessions" ? "var(--primary)" : "var(--on-surface-variant)",
              opacity: activeTab() === "sessions" ? "1" : "0.5",
              "border-bottom-color": activeTab() === "sessions" ? "var(--primary)" : "transparent",
              background: "transparent",
            }}
          >
            <span class="material-symbols-outlined" style={{ "font-size": "13px" }}>chat_bubble</span>
            <span>Sessions</span>
          </button>
          <button
            role="tab"
            aria-selected={activeTab() === "explorer"}
            aria-controls="explorer-panel"
            onClick={() => setActiveTab("explorer")}
            data-tab="explorer"
            class="flex-1 flex items-center justify-center gap-1.5 py-2 text-[11px] font-semibold transition-all duration-150 border-b-2 -mb-px"
            style={{
              color: activeTab() === "explorer" ? "var(--primary)" : "var(--on-surface-variant)",
              opacity: activeTab() === "explorer" ? "1" : "0.5",
              "border-bottom-color": activeTab() === "explorer" ? "var(--primary)" : "transparent",
              background: "transparent",
            }}
          >
            <span class="material-symbols-outlined" style={{ "font-size": "13px" }}>folder_open</span>
            <span>Explorer</span>
          </button>
        </div>
      </div>

      {/* Tab content */}
      <div class="flex-1 overflow-hidden">
        {/* Sessions tab */}
        <Show when={activeTab() === "sessions"}>
          <div
            role="tabpanel"
            id="sessions-panel"
            data-component="sessions-list"
            class="h-full overflow-y-auto py-2 px-2 custom-scrollbar"
          >
            <Show when={groupedSessions().length === 0}>
              <div class="flex flex-col items-center justify-center py-10 px-4 text-center gap-3">
                <div
                  class="w-12 h-12 rounded-2xl flex items-center justify-center"
                  style={{ background: "var(--surface-container-high)" }}
                >
                  <span class="material-symbols-outlined" style={{ "font-size": "24px", color: "var(--on-surface-variant)", opacity: "0.5" }}>
                    {debouncedSearch() ? "search_off" : "forum"}
                  </span>
                </div>
                <div>
                  <p class="text-[12px] font-medium" style={{ color: "var(--on-surface-variant)" }}>
                    {debouncedSearch() ? "No matches" : "No sessions yet"}
                  </p>
                  <Show when={!debouncedSearch()}>
                    <p class="text-[11px] mt-0.5" style={{ color: "var(--outline)" }}>
                      Start a new session to begin
                    </p>
                  </Show>
                </div>
                <Show when={!debouncedSearch()}>
                  <button
                    onClick={props.onNewSession}
                    class="flex items-center gap-1.5 px-4 py-2 rounded-full text-[11px] font-semibold transition-all duration-150 active:scale-95 hover:brightness-105"
                    style={{
                      background: "var(--primary-container)",
                      color: "var(--on-primary-container)",
                    }}
                  >
                    <span class="material-symbols-outlined" style={{ "font-size": "14px" }}>add</span>
                    New Session
                  </button>
                </Show>
              </div>
            </Show>

            <For each={visibleGroupedSessions()}>
              {({ group, sessions }) => {
                const isCollapsed = () => isGroupCollapsed(group);
                const isCompact = () => safeWorkspace()?.workspace.compactMode ?? false;

                return (
                  <div class="mb-1">
                    <button
                      onClick={() => toggleGroup(group)}
                      aria-expanded={!isGroupCollapsed(group)}
                      aria-label={`${group} sessions group`}
                      class="w-full flex items-center gap-1.5 px-2 py-1 text-[11px] font-medium hover:bg-surface-container-low rounded transition-colors mt-2 mb-0.5" style={{ color: "var(--outline)", opacity: "0.8" }}
                    >
                      <span class={`material-symbols-outlined text-[10px] transition-transform ${isCollapsed() ? "" : "rotate-90"}`}>
                        chevron_right
                      </span>
                      <span>{group}</span>
                      <span class="ml-auto text-[9px]" style={{ color: "var(--outline)", opacity: "0.6" }}>
                        {sessions.length}
                      </span>
                    </button>

                    <Show when={!isCollapsed()}>
                      <For each={sessions}>
                        {(session) => {
                          const isActive = () => session.id === props.currentSessionId;
                          const isRenaming = () => renamingSessionId() === session.id;

                          return (
                            <div
                              onClick={() => !isRenaming() && props.onSelect(session)}
                              onDblClick={(e) => !isRenaming() && startRename(session, e)}
                              data-session-id={session.id}
                              class={`rounded-lg text-xs font-medium flex items-center gap-2 cursor-pointer transition-all duration-200 mb-0.5 ${
                                isCompact() ? "px-2 py-1" : "px-2.5 py-2"
                              }`}
                              style={{
                                background: isActive() ? "var(--secondary-container)" : "transparent",
                                color: isActive() ? "var(--on-secondary-container)" : "var(--on-surface-variant)",
                              }}
                              onMouseEnter={(e) => {
                                if (!isActive() && !isRenaming()) {
                                  (e.currentTarget as HTMLElement).style.background = "var(--surface-container)";
                                  (e.currentTarget as HTMLElement).style.color = "var(--on-surface)";
                                }
                              }}
                              onMouseLeave={(e) => {
                                if (!isActive() && !isRenaming()) {
                                  (e.currentTarget as HTMLElement).style.background = "transparent";
                                  (e.currentTarget as HTMLElement).style.color = "var(--on-surface-variant)";
                                }
                              }}
                            >
                              <span class="material-symbols-outlined text-[14px] shrink-0" style={{ color: isActive() ? "var(--secondary)" : "var(--outline)", opacity: isActive() ? "1" : "0.5" }}>chat</span>

                              <Show when={isRenaming()} fallback={
                                <>
                                  <span class="truncate flex-1 text-[12px]">
                                    {session.title || "Untitled"}
                                  </span>
                                  <Show when={!isCompact()}>
                                    <span class="text-[10px] shrink-0" style={{ color: "var(--outline)", opacity: "0.5" }}>
                                      {formatTime(session.updated_at)}
                                    </span>
                                  </Show>
                                  <Show when={isCompact()}>
                                    <span class="text-[10px] shrink-0" style={{ color: "var(--outline)", opacity: "0.5" }}>
                                      {formatCompactDate(session.updated_at)}
                                    </span>
                                  </Show>
                                </>
                              }>
                                <input
                                  type="text"
                                  value={renameValue()}
                                  onInput={(e) => setRenameValue(e.currentTarget.value)}
                                  onKeyDown={handleRenameKeyDown}
                                  onBlur={() => submitRename()}
                                  autofocus
                                  class="flex-1 bg-surface-container-low text-on-surface text-xs px-1 py-0.5 rounded border border-primary/50 focus:outline-none"
                                  onClick={(e) => e.stopPropagation()}
                                />
                              </Show>
                            </div>
                          );
                        }}
                      </For>
                    </Show>
                  </div>
                );
              }}
            </For>

            {/* Rename error toast */}
            <Show when={renameError()}>
              <div class="fixed bottom-4 left-1/2 -translate-x-1/2 bg-error-container text-error px-3 py-1.5 rounded-lg text-xs shadow-lg">
                {renameError()}
              </div>
            </Show>

            {/* Show all / collapse footer */}
            <Show when={!debouncedSearch() && totalSessionCount() > RECENT_LIMIT}>
              <button
                onClick={() => setShowAllSessions(v => !v)}
                class="w-full flex items-center justify-center gap-1.5 py-2 mt-1 text-[11px] transition-all duration-150 rounded-lg"
                style={{ color: "var(--outline)" }}
                onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--surface-container)"; (e.currentTarget as HTMLElement).style.color = "var(--on-surface-variant)"; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; (e.currentTarget as HTMLElement).style.color = "var(--outline)"; }}
              >
                <span class="material-symbols-outlined" style={{ "font-size": "13px" }}>
                  {showAllSessions() ? "expand_less" : "expand_more"}
                </span>
                <span>
                  {showAllSessions()
                    ? "Show less"
                    : `${totalSessionCount() - RECENT_LIMIT} more sessions`}
                </span>
              </button>
            </Show>
          </div>
        </Show>

        {/* Explorer tab */}
        <Show when={activeTab() === "explorer"}>
          <div
            role="tabpanel"
            id="explorer-panel"
            data-component="explorer-tree"
            class="h-full flex flex-col outline-none"
            onKeyDown={handleExplorerKeyDown}
            tabIndex={0}
          >
            {/* Loading state */}
            <Show when={isLoading()}>
              <div class="flex flex-col items-center justify-center flex-1 p-4">
                <div class="w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin mb-3" />
                <p class="text-xs text-outline">Loading explorer...</p>
              </div>
            </Show>

            {/* Error state */}
            <Show when={!isLoading() && explorerError()}>
              <div class="flex flex-col items-center justify-center flex-1 p-4 text-center">
                <span class="material-symbols-outlined text-3xl text-error mb-2">error</span>
                <p class="text-xs text-error mb-1">Failed to load explorer</p>
                <p class="text-xs text-outline">{explorerError()}</p>
                <Show when={props.currentSessionId || projectContext.activeProject()}>
                  <button
                    onClick={() => loadExplorerData(props.currentSessionId, activeFilter())}
                    class="mt-3 px-3 py-1.5 text-xs bg-surface-container-high rounded hover:bg-surface-container-highest transition-colors"
                  >
                    Retry
                  </button>
                </Show>
              </div>
            </Show>

            {/* No session selected */}
            <Show when={!isLoading() && !explorerError() && !props.currentSessionId && !projectContext.activeProject()}>
              <div class="flex flex-col items-center justify-center flex-1 p-4 text-center">
                <span class="material-symbols-outlined text-3xl text-outline mb-2">folder_open</span>
                <p class="text-xs text-outline">Select a session to view files</p>
              </div>
            </Show>

            {/* Explorer content */}
            <Show when={!isLoading() && !explorerError() && (props.currentSessionId || projectContext.activeProject())}>
              {/* Git status bar */}
              <Show when={bootstrap()}>
                <div class="flex items-center gap-2 px-2 py-1.5 text-[10px] text-outline border-b border-outline-variant/20 shrink-0">
                  <Show when={bootstrap()?.git_available}>
                    <span class="flex items-center gap-1">
                      <span class="material-symbols-outlined text-xs text-secondary">code</span>
                      <span>Git</span>
                    </span>
                    <Show when={bootstrap()?.watching}>
                      <span class="flex items-center gap-1 text-tertiary">
                        <span class="material-symbols-outlined text-xs">visibility</span>
                        <span>Watching</span>
                      </span>
                    </Show>
                  </Show>
                  <Show when={!bootstrap()?.git_available}>
                    <span class="flex items-center gap-1">
                      <span class="material-symbols-outlined text-xs text-outline-variant">code_off</span>
                      <span>No git</span>
                    </span>
                  </Show>
                </div>
              </Show>

              {/* Filter bar */}
              <div class="shrink-0 px-2 pt-1">
                <FilterBar
                  activeFilter={activeFilter()}
                  onFilterChange={handleFilterChange}
                  counts={filterCounts()}
                />
              </div>

              {/* Virtualized tree */}
              <Show when={flatNodes().length === 0 && !isLoading()}>
                <div class="flex flex-col items-center justify-center flex-1 p-4 text-center">
                  <span class="material-symbols-outlined text-3xl text-outline mb-2">folder_open</span>
                  <p class="text-xs text-outline">No files match this filter</p>
                </div>
              </Show>

              <div
                ref={scrollContainerRef}
                class="flex-1 overflow-y-auto custom-scrollbar px-1"
              >
                <ExplorerContext.Provider value={{ activeFilePath, focusedNodeId }}>
                  {SUPPORTS_VIRTUALIZATION ? (
                    <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative", width: "100%" }}>
                      <For each={virtualizer.getVirtualItems()}>
                        {(virtualItem) => {
                          const row = () => flatNodes()[virtualItem.index];
                          return (
                            <div
                              data-index={virtualItem.index}
                              style={{
                                position: "absolute",
                                top: 0, left: 0, width: "100%",
                                height: `${virtualItem.size}px`,
                                transform: `translateY(${virtualItem.start}px)`,
                              }}
                            >
                              <TreeNodeRow
                                node={row().node}
                                depth={row().depth}
                                isExpanded={expandedPaths().has(row().node.id)}
                                onToggle={handleToggle}
                                onLoadChildren={loadChildren}
                                onSelectFile={props.onSelectFile}
                              />
                            </div>
                          );
                        }}
                      </For>
                    </div>
                  ) : (
                    <For each={flatNodes()}>
                      {(row) => (
                        <TreeNodeRow
                          node={row.node}
                          depth={row.depth}
                          isExpanded={expandedPaths().has(row.node.id)}
                          onToggle={handleToggle}
                          onLoadChildren={loadChildren}
                          onSelectFile={props.onSelectFile}
                        />
                      )}
                    </For>
                  )}
                </ExplorerContext.Provider>
              </div>
            </Show>
          </div>
        </Show>
      </div>
    </aside>
  );
}
