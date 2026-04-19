import type { TreeNode } from "../../api/explorer";
import { useExplorerContext } from "./ExplorerContext";

// ─── helpers ────────────────────────────────────────────────────────────────

/** Compact number formatter: 1234 → "1.2k", 12345 → "12k" */
function fmtNum(n: number): string {
  if (n >= 1000) return `${Math.round(n / 100) / 10}k`;
  return String(n);
}

// ─── flat row descriptor ─────────────────────────────────────────────────────

/**
 * A single flattened row in the virtual tree list.
 * depth=0 means root level.
 */
export interface FlatNode {
  node: TreeNode;
  depth: number;
}

/**
 * Build a flat ordered list of visible rows from the tree state.
 * Call this inside a createMemo to keep it reactive.
 */
export function buildFlatTree(
  nodes: TreeNode[],
  expandedPaths: Set<string>,
  loadedPaths: Map<string, TreeNode[]>,
  depth = 0,
): FlatNode[] {
  const result: FlatNode[] = [];
  for (const node of nodes) {
    result.push({ node, depth });
    if (node.kind === "dir" && expandedPaths.has(node.id)) {
      const children = loadedPaths.get(node.id) ?? [];
      const childRows = buildFlatTree(children, expandedPaths, loadedPaths, depth + 1);
      result.push(...childRows);
    }
  }
  return result;
}

// ─── component ──────────────────────────────────────────────────────────────

export interface TreeNodeRowProps {
  node: TreeNode;
  depth: number;
  isExpanded: boolean;
  onToggle: (node: TreeNode) => void;
  onLoadChildren: (node: TreeNode) => void;
  onSelectFile?: (path: string) => void;
}

export function TreeNodeRow(props: TreeNodeRowProps) {
  const explorerCtx = useExplorerContext();
  const isDir = () => props.node.kind === "dir";

  const handleClick = () => {
    if (isDir()) {
      if (!props.isExpanded) {
        props.onLoadChildren(props.node);
      }
      props.onToggle(props.node);
    } else {
      props.onSelectFile?.(props.node.relative_path);
    }
  };

  // ── git state helpers ──────────────────────────────────────────────────────
  const git = () => props.node.git;
  const isChanged    = () => git()?.is_changed === true && git()?.is_staged !== true;
  const isStaged     = () => git()?.is_staged === true;
  const isUntracked  = () => git()?.is_untracked === true;
  const isConflicted = () => git()?.is_conflicted === true;
  const isIgnored    = () => git()?.ignored === true;
  const isOutsideRepo = () => git()?.repo_scope === "outside_repo";

  // ── icon color ─────────────────────────────────────────────────────────────
  const iconClass = () => {
    if (isDir()) {
      const agg = props.node.aggregate;
      if (agg) {
        if ((agg.conflicted_descendants ?? 0) > 0) return "text-error";
        if ((agg.changed_descendants ?? 0) > 0)    return "text-accent";
        if ((agg.untracked_descendants ?? 0) > 0)  return "text-tertiary";
      }
      return "text-secondary";
    }
    if (isConflicted()) return "text-error";
    if (isStaged() && isChanged()) return "text-accent";
    if (isStaged())    return "text-secondary";
    if (isChanged())   return "text-accent";
    if (isUntracked()) return "text-tertiary";
    if (isIgnored())   return "text-outline-variant opacity-50";
    if (isOutsideRepo()) return "text-outline-variant";
    return "text-outline";
  };

  // ── name text color ────────────────────────────────────────────────────────
  const nameClass = () => {
    if (isConflicted()) return "text-error font-medium";
    if (isStaged())     return "text-secondary";
    if (isChanged())    return "text-accent";
    if (isUntracked())  return "text-tertiary";
    if (isDir()) {
      const agg = props.node.aggregate;
      if (agg) {
        if ((agg.conflicted_descendants ?? 0) > 0) return "text-error";
        if ((agg.changed_descendants ?? 0) > 0)    return "text-on-surface";
      }
    }
    return "";
  };

  // ── active/focused state ───────────────────────────────────────────────────
  const isActiveFile = () => !isDir() && explorerCtx.activeFilePath() === props.node.relative_path;
  const isFocused    = () => explorerCtx.focusedNodeId() === props.node.id;

  const rowClass = () => [
    "flex items-center gap-1.5 px-2 py-[3px] rounded cursor-pointer",
    "hover:bg-surface-container-high text-xs transition-colors select-none",
    isDir() ? "font-medium" : "",
    isIgnored() ? "opacity-40" : "",
    isOutsideRepo() ? "opacity-60" : "",
    isFocused()
      ? "ring-1 ring-primary bg-primary/10"
      : isActiveFile()
        ? "bg-primary-container/30 text-primary"
        : "",
  ].filter(Boolean).join(" ");

  // ── diff badge (+N -N) ─────────────────────────────────────────────────────
  const DiffBadge = () => {
    const add = git()?.additions;
    const del = git()?.deletions;
    const hasAdd = add != null && add > 0;
    const hasDel = del != null && del > 0;
    if (!hasAdd && !hasDel) return null;
    return (
      <span
        class="flex items-center gap-0.5 shrink-0 font-mono text-[10px] leading-none"
        aria-label={`+${add ?? 0} -${del ?? 0} lines`}
      >
        {hasAdd && (
          <span class="px-0.5 rounded-sm tabular-nums" style="background:rgba(var(--success-rgb,78,222,163),.15);color:var(--success,#4edea3)">
            +{fmtNum(add!)}
          </span>
        )}
        {hasDel && (
          <span class="px-0.5 rounded-sm bg-error/15 text-error tabular-nums">
            -{fmtNum(del!)}
          </span>
        )}
      </span>
    );
  };

  // ── status dot (for files without numstat, e.g. untracked) ────────────────
  const StatusDot = () => {
    const add = git()?.additions;
    const del = git()?.deletions;
    const hasNumstat = (add != null && add > 0) || (del != null && del > 0);
    if (hasNumstat) return null;

    if (isConflicted()) return <span class="w-1.5 h-1.5 rounded-full bg-error shrink-0" title="Conflicted" />;
    if (isStaged() && isChanged()) return <span class="w-1.5 h-1.5 rounded-full bg-secondary shrink-0" title="Staged + Modified" />;
    if (isStaged())    return <span class="w-1.5 h-1.5 rounded-full bg-secondary shrink-0" title="Staged" />;
    if (isChanged())   return <span class="w-1.5 h-1.5 rounded-full bg-accent shrink-0" title="Modified" />;
    if (isUntracked()) return <span class="w-1.5 h-1.5 rounded-full bg-tertiary shrink-0" title="Untracked" />;
    return null;
  };

  // ── directory aggregate pill ───────────────────────────────────────────────
  const AggregatePill = () => {
    if (!isDir()) return null;
    const agg = props.node.aggregate;
    if (!agg) return null;

    const conflict  = agg.conflicted_descendants ?? 0;
    const changed   = agg.changed_descendants ?? 0;
    const untracked = agg.untracked_descendants ?? 0;
    const total = conflict + changed + untracked;
    if (total === 0) return null;

    const pillClass = conflict > 0
      ? "bg-error/20 text-error border-error/30"
      : changed > 0
        ? "bg-accent/20 text-accent border-accent/30"
        : "bg-tertiary/20 text-tertiary border-tertiary/30";

    return (
      <span
        class={`shrink-0 inline-flex items-center justify-center min-w-[18px] h-[14px] px-1 text-[9px] font-mono font-semibold leading-none rounded-full border ${pillClass}`}
        title={[
          conflict  > 0 ? `${conflict} conflicted`  : "",
          changed   > 0 ? `${changed} changed`       : "",
          untracked > 0 ? `${untracked} untracked`   : "",
        ].filter(Boolean).join(", ")}
      >
        {fmtNum(total)}
      </span>
    );
  };

  return (
    <div
      onClick={handleClick}
      class={rowClass()}
      data-node-id={props.node.id}
      data-node-kind={props.node.kind}
      data-active={isActiveFile() ? "true" : undefined}
      data-focused={isFocused() ? "true" : "false"}
      // Indent via left padding proportional to depth
      style={{ "padding-left": `${8 + props.depth * 12}px` }}
    >
      {/* Chevron / spacer */}
      {isDir() ? (
        <span
          class={`material-symbols-outlined text-sm w-4 text-center transition-transform duration-150 ${props.isExpanded ? "rotate-90" : ""}`}
        >
          chevron_right
        </span>
      ) : (
        <span class="w-4 shrink-0" />
      )}

      {/* Folder / file icon */}
      <span class={`material-symbols-outlined text-sm shrink-0 ${iconClass()}`}>
        {isDir() ? (props.isExpanded ? "folder_open" : "folder") : "description"}
      </span>

      {/* File/dir name */}
      <span class={`truncate flex-1 ${nameClass()}`}>
        {props.node.name}
      </span>

      {/* Right-side indicators */}
      {isDir() ? (
        <AggregatePill />
      ) : (
        <>
          <DiffBadge />
          <StatusDot />
        </>
      )}
    </div>
  );
}
