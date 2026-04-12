import { createSignal, createEffect, For, Show } from "solid-js";
import type { Session } from "../App";
import { fetchExplorerBootstrap, fetchExplorerTree, type ExplorerBootstrap, type TreeNode, type ExplorerFilter } from "../api/explorer";
import { useProjectContext } from "../context/ProjectContext";

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

// Filter counts for badge display
interface FilterCounts {
  changed: number;
  staged: number;
  untracked: number;
  conflicted: number;
}

function formatTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);

  if (minutes < 1) return "Just now";
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  return date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

// Filter bar component
function FilterBar(props: {
  activeFilter: ExplorerFilter;
  onFilterChange: (filter: ExplorerFilter) => void;
  counts: FilterCounts;
}) {
  const filters: { key: ExplorerFilter; label: string }[] = [
    { key: "all", label: "All" },
    { key: "changed", label: "Changed" },
    { key: "staged", label: "Staged" },
    { key: "untracked", label: "Untracked" },
    { key: "conflicted", label: "Conflicted" },
  ];

  return (
    <div class="flex items-center gap-1 px-2 py-1 border-b border-outline-variant/20 overflow-x-auto custom-scrollbar">
      <For each={filters}>
        {(filter) => {
          const count = () => {
            switch (filter.key) {
              case "changed": return props.counts.changed;
              case "staged": return props.counts.staged;
              case "untracked": return props.counts.untracked;
              case "conflicted": return props.counts.conflicted;
              default: return null;
            }
          };

          return (
            <button
              onClick={() => props.onFilterChange(filter.key)}
              class={`flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium transition-all whitespace-nowrap ${
                props.activeFilter === filter.key
                  ? "bg-primary-container text-on-primary-container"
                  : "text-outline hover:bg-surface-container-high hover:text-on-surface"
              }`}
            >
              <span>{filter.label}</span>
              <Show when={count() !== null && count()! > 0}>
                <span class={`px-1 rounded-full text-[9px] ${
                  props.activeFilter === filter.key 
                    ? "bg-on-primary-container/20" 
                    : "bg-surface-container-high"
                }`}>
                  {count()}
                </span>
              </Show>
            </button>
          );
        }}
      </For>
    </div>
  );
}

// Tree node component for explorer
function TreeNodeRow(props: {
  node: TreeNode;
  onToggle: (node: TreeNode) => void;
  expandedPaths: Set<string>;
  loadedPaths: Map<string, TreeNode[]>;
  onLoadChildren: (node: TreeNode) => void;
  onSelectFile?: (path: string) => void;
  isFocused?: boolean;
  focusedNodeId?: string | null;
  // T4.4: Active file path for highlight
  activeFilePath?: string | null;
}) {
  const isExpanded = () => props.expandedPaths.has(props.node.id);
  const isDir = () => props.node.kind === "dir";
  const children = () => props.loadedPaths.get(props.node.id) || [];
  
  const handleClick = () => {
    if (isDir()) {
      if (isExpanded()) {
        props.onToggle(props.node);
      } else {
        props.onLoadChildren(props.node);
        props.onToggle(props.node);
      }
    } else {
      // File node - call onSelectFile with the relative path
      if (props.onSelectFile) {
        props.onSelectFile(props.node.relative_path);
      }
    }
  };

  // Git status indicator
  const gitStatus = () => props.node.git;
  const isChanged = () => gitStatus()?.is_changed === true && gitStatus()?.is_staged !== true;
  const isStaged = () => gitStatus()?.is_staged === true;
  const isUntracked = () => gitStatus()?.is_untracked === true;
  const isConflicted = () => gitStatus()?.is_conflicted === true;
  const isIgnored = () => gitStatus()?.ignored === true;
  const isOutsideRepo = () => gitStatus()?.repo_scope === "outside_repo";

  // Icon color based on git status
  const getIconClass = () => {
    if (isDir()) return "text-secondary";
    if (isConflicted()) return "text-error";
    if (isChanged()) return "text-accent";
    if (isStaged()) return "text-secondary";
    if (isUntracked()) return "text-tertiary";
    if (isIgnored()) return "text-outline-variant opacity-50";
    if (isOutsideRepo()) return "text-outline-variant";
    return "text-outline";
  };

  // Get badge element
  const getBadge = () => {
    if (isConflicted()) return <span class="w-2 h-2 rounded-full bg-error shrink-0" title="Conflicted" />;
    if (isStaged() && isChanged()) return <span class="w-2 h-2 rounded-full bg-secondary shrink-0" title="Staged + Modified" />;
    if (isStaged()) return <span class="w-2 h-2 rounded-full bg-secondary shrink-0" title="Staged" />;
    if (isChanged()) return <span class="w-2 h-2 rounded-full bg-accent shrink-0" title="Modified" />;
    if (isUntracked()) return <span class="w-2 h-2 rounded-full bg-tertiary shrink-0" title="Untracked" />;
    return null;
  };

  // T4.4: Check if this is the active file
  const isActiveFile = () => !isDir() && props.activeFilePath === props.node.relative_path;

  return (
    <div class="tree-node">
      <div
        onClick={handleClick}
        class={`flex items-center gap-1.5 px-2 py-1 rounded cursor-pointer hover:bg-surface-container-high text-xs transition-colors ${
          isDir() ? "font-medium" : ""
        } ${isIgnored() ? "opacity-50" : ""} ${isOutsideRepo() ? "opacity-70" : ""} ${
          props.focusedNodeId === props.node.id ? "ring-1 ring-primary bg-primary/10" : ""
        } ${isActiveFile() && props.focusedNodeId !== props.node.id ? "bg-primary-container/30 text-primary" : ""}`}
        data-node-id={props.node.id}
        data-node-kind={props.node.kind}
        data-active={isActiveFile() ? "true" : undefined}
        data-focused={props.focusedNodeId === props.node.id ? "true" : "false"}
      >
        {/* Expand/collapse icon for dirs */}
        <Show when={isDir()}>
          <span class={`material-symbols-outlined text-sm w-4 text-center transition-transform ${isExpanded() ? "rotate-90" : ""}`}>
            chevron_right
          </span>
        </Show>
        <Show when={!isDir()}>
          <span class="w-4" />
        </Show>

        {/* Icon */}
        <Show when={isDir()}>
          <span class={`material-symbols-outlined text-sm ${getIconClass()}`}>folder</span>
        </Show>
        <Show when={!isDir()}>
          <span class={`material-symbols-outlined text-sm ${getIconClass()}`}>description</span>
        </Show>

        {/* Name */}
        <span class="truncate flex-1">{props.node.name}</span>

        {/* Aggregate counts for directories */}
        <Show when={isDir() && props.node.aggregate}>
          <Show when={(props.node.aggregate?.changed_descendants ?? 0) > 0}>
            <span class="w-1.5 h-1.5 rounded-full bg-accent shrink-0" title={`${props.node.aggregate?.changed_descendants} changed`} />
          </Show>
          <Show when={(props.node.aggregate?.untracked_descendants ?? 0) > 0}>
            <span class="w-1.5 h-1.5 rounded-full bg-tertiary shrink-0" title={`${props.node.aggregate?.untracked_descendants} untracked`} />
          </Show>
          <Show when={(props.node.aggregate?.conflicted_descendants ?? 0) > 0}>
            <span class="w-1.5 h-1.5 rounded-full bg-error shrink-0" title={`${props.node.aggregate?.conflicted_descendants} conflicted`} />
          </Show>
        </Show>

        {/* Git status badge */}
        {getBadge()}
      </div>

      {/* Children */}
      <Show when={isDir() && isExpanded()}>
        <div class="pl-4 border-l border-outline-variant/20 ml-2">
          <Show when={children().length === 0}>
            <div class="px-2 py-1 text-xs text-outline italic">Empty</div>
          </Show>
          <For each={children()}>
            {(child) => (
              <TreeNodeRow
                node={child}
                onToggle={props.onToggle}
                expandedPaths={props.expandedPaths}
                loadedPaths={props.loadedPaths}
                onLoadChildren={props.onLoadChildren}
                onSelectFile={props.onSelectFile}
                focusedNodeId={props.focusedNodeId}
                activeFilePath={props.activeFilePath}
              />
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

export default function WorkbenchLeftRail(props: WorkbenchLeftRailProps) {
  const [activeTab, setActiveTab] = createSignal<RailTab>("sessions");
  const projectContext = useProjectContext();
  
  // Explorer state
  const [bootstrap, setBootstrap] = createSignal<ExplorerBootstrap | null>(null);
  const [rootChildren, setRootChildren] = createSignal<TreeNode[]>([]);
  const [expandedPaths, setExpandedPaths] = createSignal<Set<string>>(new Set());
  const [loadedPaths, setLoadedPaths] = createSignal<Map<string, TreeNode[]>>(new Map());
  const [explorerError, setExplorerError] = createSignal<string | null>(null);
  const [isLoading, setIsLoading] = createSignal(false);
  const [activeFilter, setActiveFilter] = createSignal<ExplorerFilter>("all");
  const [filterCounts, setFilterCounts] = createSignal<FilterCounts>({ changed: 0, staged: 0, untracked: 0, conflicted: 0 });

  // T4.3: Keyboard navigation state - track focused node id
  const [focusedNodeId, setFocusedNodeId] = createSignal<string | null>(null);

  // Build a flat list of visible nodes for keyboard navigation
  function getVisibleNodes(): TreeNode[] {
    const nodes: TreeNode[] = [];
    
    function traverse(nodeList: TreeNode[]) {
      for (const node of nodeList) {
        nodes.push(node);
        if (node.kind === "dir" && expandedPaths().has(node.id)) {
          const children = loadedPaths().get(node.id) || [];
          traverse(children);
        }
      }
    }
    
    traverse(rootChildren());
    return nodes;
  }

  // T4.3: Handle keyboard navigation
  function handleExplorerKeyDown(e: KeyboardEvent) {
    const visibleNodes = getVisibleNodes();
    if (visibleNodes.length === 0) return;

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        {
          const currentIdx = visibleNodes.findIndex(n => n.id === focusedNodeId());
          const nextIdx = currentIdx < 0 ? 0 : Math.min(currentIdx + 1, visibleNodes.length - 1);
          setFocusedNodeId(visibleNodes[nextIdx].id);
        }
        break;
      case "ArrowUp":
        e.preventDefault();
        {
          const currentIdx = visibleNodes.findIndex(n => n.id === focusedNodeId());
          const prevIdx = currentIdx < 0 ? 0 : Math.max(currentIdx - 1, 0);
          setFocusedNodeId(visibleNodes[prevIdx].id);
        }
        break;
      case "Enter":
      case " ":
      case "ArrowRight":
        e.preventDefault();
        {
          const focusedId = focusedNodeId();
          if (!focusedId) return;
          const node = visibleNodes.find(n => n.id === focusedId);
          if (!node) return;
          if (node.kind === "dir") {
            // Expand/load children
            if (!expandedPaths().has(node.id)) {
              loadChildren(node);
              handleToggle(node);
            }
          } else {
            // Select file
            if (props.onSelectFile) {
              props.onSelectFile(node.relative_path);
            }
          }
        }
        break;
      case "ArrowLeft":
        e.preventDefault();
        {
          const focusedId = focusedNodeId();
          if (!focusedId) return;
          const node = visibleNodes.find(n => n.id === focusedId);
          if (!node) return;
          if (node.kind === "dir" && expandedPaths().has(node.id)) {
            // Collapse directory
            handleToggle(node);
          }
        }
        break;
    }
  }

  // Compute filter counts from all loaded nodes
  function computeFilterCounts(nodes: TreeNode[]) {
    const counts = { changed: 0, staged: 0, untracked: 0, conflicted: 0 };
    
    function countNode(node: TreeNode) {
      const git = node.git;
      if (!git) return;
      
      if (git.is_changed && git.is_staged !== true) counts.changed++;
      if (git.is_staged) counts.staged++;
      if (git.is_untracked) counts.untracked++;
      if (git.is_conflicted) counts.conflicted++;
      
      // Count in loaded children too
      const children = loadedPaths().get(node.id) || [];
      children.forEach(countNode);
    }
    
    nodes.forEach(countNode);
    setFilterCounts(counts);
  }

  // Fetch explorer data when tab is activated
  createEffect(() => {
    if (activeTab() === "explorer" && props.currentSessionId) {
      loadExplorerData(props.currentSessionId, activeFilter());
    } else if (activeTab() === "explorer" && projectContext.activeProject()) {
      loadExplorerData(undefined, activeFilter());
    }
  });

  // T4.4: Auto-expand parent directories when activeFilePath changes to reveal the file
  createEffect(() => {
    const filePath = props.activeFilePath;
    if (!filePath || !props.currentSessionId) return;

    // filePath is like "src/main.rs" - we need to expand "src" directory
    const parts = filePath.split('/');
    if (parts.length < 2) return; // Top-level file, no expansion needed

    // Expand all parent directories (all parts except the last which is the filename)
    const dirsToExpand = parts.slice(0, -1); // All parts except filename

    // Function to recursively find and expand directories
    function expandDirs(nodeList: TreeNode[], pathParts: string[]): boolean {
      if (pathParts.length === 0) return true; // Done

      const [currentPart, ...remainingParts] = pathParts;
      const node = nodeList.find(n => n.name === currentPart);
      if (!node || node.kind !== "dir") return false;

      // This is the directory we need to expand
      // First load its children if not already loaded
      if (!loadedPaths().has(node.id)) {
        loadChildren(node);
      }

      // Mark it as expanded
      setExpandedPaths((prev) => {
        const next = new Set(prev);
        next.add(node.id);
        return next;
      });

      // Recursively expand children
      if (remainingParts.length > 0) {
        // Wait a tick for children to be loaded, then recurse
        const children = loadedPaths().get(node.id) || [];
        return expandDirs(children, remainingParts);
      }
      return true;
    }

    // Start expansion from root
    expandDirs(rootChildren(), dirsToExpand);
  });

  // REQ-4.5: Set focusedNodeId and scroll into view when activeFilePath changes
  createEffect(() => {
    const filePath = props.activeFilePath;
    if (!filePath) return;

    // Find the tree node that matches the active file path
    function findNodeByPath(nodeList: TreeNode[], path: string): TreeNode | null {
      for (const node of nodeList) {
        // File nodes (not directories) have relative_path matching the path
        if (node.kind !== "dir" && node.relative_path === path) {
          return node;
        }
        // Recursively search in directory children
        if (node.kind === "dir") {
          const children = loadedPaths().get(node.id) || [];
          const found = findNodeByPath(children, path);
          if (found) return found;
        }
      }
      return null;
    }

    // Find the matching node
    const foundNode = findNodeByPath(rootChildren(), filePath);
    
    if (foundNode) {
      // Set focused node for keyboard navigation
      setFocusedNodeId(foundNode.id);
      
      // Queue scroll into view - either now or when Explorer tab becomes active
      const scrollIntoView = () => {
        const element = document.querySelector(`[data-node-id="${foundNode.id}"]`);
        if (element && typeof element.scrollIntoView === "function") {
          element.scrollIntoView({ behavior: "smooth", block: "nearest" });
        }
      };
      
      if (activeTab() === "explorer") {
        // Tab is active, scroll immediately after a brief delay for DOM to update
        setTimeout(scrollIntoView, 50);
      }
      // If tab is not active, the tab switch will handle scroll when it becomes active
    }
  });

  // REQ-4.5: When Explorer tab becomes active, scroll to focusedNode if set
  createEffect(() => {
    if (activeTab() === "explorer" && focusedNodeId()) {
      const nodeId = focusedNodeId();
      if (nodeId) {
        setTimeout(() => {
          const element = document.querySelector(`[data-node-id="${nodeId}"]`);
          if (element && typeof element.scrollIntoView === "function") {
            element.scrollIntoView({ behavior: "smooth", block: "nearest" });
          }
        }, 50);
      }
    }
  });

  async function loadExplorerData(sessionId: string | undefined, filter: ExplorerFilter) {
    setIsLoading(true);
    setExplorerError(null);
    const projectId = projectContext.activeProject()?.id ?? null;
    
    try {
      // Fetch bootstrap
      const boot = await fetchExplorerBootstrap(sessionId, projectId);
      setBootstrap(boot);
      
      // Fetch root children with filter
      const tree = await fetchExplorerTree(sessionId, ".", 1, filter, false, false, projectId);
      setRootChildren(tree.children);
      
      // Compute counts from all files (fetch without filter first for counts)
      const allTree = await fetchExplorerTree(sessionId, ".", 1, "all", false, false, projectId);
      computeFilterCounts(allTree.children);
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
      // Fetch children with the current filter applied
      const tree = await fetchExplorerTree(props.currentSessionId, node.path, 1, activeFilter(), false, false, projectId);
      setLoadedPaths((prev) => {
        const next = new Map(prev);
        next.set(node.id, tree.children);
        return next;
      });
      
      // Note: We don't overwrite filtered children with "all" children.
      // The computeFilterCounts function recursively counts from already-loaded children,
      // so filtered counts will be accurate for the loaded subtree.
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
      data-component="workbench-left-rail"
      class="bg-[#181c22] flex flex-col h-full shrink-0 border-r border-outline-variant/20"
      style={{ width: `${props.width ?? 256}px`, "min-width": "180px" }}
    >
      {/* T3.1: Active project header */}
      <Show when={projectContext.activeProject()}>
        {(project) => (
          <div class="px-3 py-2 border-b border-outline-variant/20 bg-surface-container-low/50">
            <div class="text-xs font-semibold text-on-surface truncate" title={project().name}>
              {project().name}
            </div>
            <div class="text-[10px] text-outline truncate mt-0.5" title={project().canonical_path}>
              {project().canonical_path}
            </div>
          </div>
        )}
      </Show>

      {/* New Session button */}
      <div class="p-3">
        <button
          data-component="new-session-button"
          onClick={props.onNewSession}
          class="w-full bg-primary-container text-on-primary-container py-2.5 rounded-lg font-bold flex items-center justify-center gap-2 hover:opacity-90 active:scale-95 duration-150 transition-all text-sm"
        >
          <span class="material-symbols-outlined text-sm">add</span>
          <span>New Session</span>
        </button>
      </div>

      {/* Tab switcher */}
      <div 
        data-component="rail-tabs"
        class="flex border-b border-outline-variant/20 px-2"
      >
        <button
          onClick={() => setActiveTab("sessions")}
          class={`flex-1 py-2.5 text-xs font-semibold transition-all relative ${
            activeTab() === "sessions" 
              ? "text-primary" 
              : "text-outline hover:text-on-surface"
          }`}
          data-tab="sessions"
        >
          <span class="flex items-center justify-center gap-1.5">
            <span class="material-symbols-outlined text-sm">chat_bubble</span>
            <span>Sessions</span>
          </span>
          <Show when={activeTab() === "sessions"}>
            <div class="absolute bottom-0 left-2 right-2 h-0.5 bg-primary rounded-full"></div>
          </Show>
        </button>
        
        <button
          onClick={() => setActiveTab("explorer")}
          class={`flex-1 py-2.5 text-xs font-semibold transition-all relative ${
            activeTab() === "explorer" 
              ? "text-primary" 
              : "text-outline hover:text-on-surface"
          }`}
          data-tab="explorer"
        >
          <span class="flex items-center justify-center gap-1.5">
            <span class="material-symbols-outlined text-sm">folder_open</span>
            <span>Explorer</span>
          </span>
          <Show when={activeTab() === "explorer"}>
            <div class="absolute bottom-0 left-2 right-2 h-0.5 bg-primary rounded-full"></div>
          </Show>
        </button>
      </div>

      {/* Tab content */}
      <div class="flex-1 overflow-hidden">
        {/* Sessions tab */}
        <Show when={activeTab() === "sessions"}>
          <div 
            data-component="sessions-list"
            class="h-full overflow-y-auto py-2 px-2 custom-scrollbar"
          >
            <For each={props.sessions} fallback={
              <div class="p-3 text-center">
                <p class="text-outline text-xs">No sessions yet</p>
              </div>
            }>
              {(session) => (
                <div
                  onClick={() => props.onSelect(session)}
                  data-session-id={session.id}
                  class={`p-2.5 rounded-lg text-xs font-medium flex items-center gap-2 cursor-pointer transition-all mb-1 ${
                    session.id === props.currentSessionId
                      ? "bg-surface-container-high text-primary font-semibold border-l-2 border-secondary"
                      : "text-outline hover:bg-surface-container-high hover:text-on-surface-variant"
                  }`}
                >
                  <span class="material-symbols-outlined text-sm shrink-0">chat_bubble</span>
                  <span class="truncate flex-1">
                    {session.title || "Untitled"}
                  </span>
                  <span class="text-[10px] text-outline-variant shrink-0">
                    {formatTime(session.updated_at)}
                  </span>
                </div>
              )}
            </For>
          </div>
        </Show>

        {/* Explorer tab */}
        <Show when={activeTab() === "explorer"}>
          <div 
            data-component="explorer-tree"
            class="h-full overflow-y-auto py-2 px-2 custom-scrollbar flex flex-col"
            onKeyDown={handleExplorerKeyDown}
            tabIndex={0}
          >
            {/* Loading state */}
            <Show when={isLoading()}>
              <div class="flex flex-col items-center justify-center h-full p-4">
                <div class="w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin mb-3" />
                <p class="text-xs text-outline">Loading explorer...</p>
              </div>
            </Show>

            {/* Error state */}
            <Show when={!isLoading() && explorerError()}>
              <div class="flex flex-col items-center justify-center h-full p-4 text-center">
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
              <div class="flex flex-col items-center justify-center h-full p-4 text-center">
                <span class="material-symbols-outlined text-3xl text-outline mb-2">folder_open</span>
                <p class="text-xs text-outline">Select a session to view files</p>
              </div>
            </Show>

            {/* Explorer content */}
            <Show when={!isLoading() && !explorerError() && (props.currentSessionId || projectContext.activeProject())}>
              {/* Git status bar */}
              <Show when={bootstrap()}>
                <div class="flex items-center gap-2 px-2 py-1.5 text-[10px] text-outline border-b border-outline-variant/20 mb-1">
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
              <FilterBar
                activeFilter={activeFilter()}
                onFilterChange={handleFilterChange}
                counts={filterCounts()}
              />

              {/* Tree */}
              <div class="flex-1 mt-1">
                <Show when={rootChildren().length === 0 && !isLoading()}>
                  <div class="flex flex-col items-center justify-center h-full p-4 text-center">
                    <span class="material-symbols-outlined text-3xl text-outline mb-2">folder_open</span>
                    <p class="text-xs text-outline">No files match this filter</p>
                  </div>
                </Show>

                <For each={rootChildren()}>
                  {(node) => (
                    <TreeNodeRow
                      node={node}
                      onToggle={handleToggle}
                      expandedPaths={expandedPaths()}
                      loadedPaths={loadedPaths()}
                      onLoadChildren={loadChildren}
                      onSelectFile={props.onSelectFile}
                      focusedNodeId={focusedNodeId()}
                      activeFilePath={props.activeFilePath}
                    />
                  )}
                </For>
              </div>
            </Show>
          </div>
        </Show>
      </div>
    </aside>
  );
}
