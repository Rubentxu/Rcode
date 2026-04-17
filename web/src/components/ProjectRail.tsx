import { createSignal, createMemo, createEffect, onCleanup, For, Show, onMount } from "solid-js";
import { useProjectContext } from "../context/ProjectContext";
import { createProject } from "../api/projects";
import type { ProjectHealth, ProjectSummary } from "../api/projects";
import { openFolderPicker } from "../api/fs";
import ProjectHealthBadge from "./ProjectHealthBadge";

// ──────────────────────────────────────────────────────────────────
// Color palette: muted/pastel dark tones — no screaming saturation
// ──────────────────────────────────────────────────────────────────
const MUTED_COLORS = [
  { bg: "rgba(99, 102, 241, 0.18)",  border: "rgba(99, 102, 241, 0.45)",  text: "#a5b4fc" }, // indigo
  { bg: "rgba(139, 92, 246, 0.18)",  border: "rgba(139, 92, 246, 0.45)",  text: "#c4b5fd" }, // violet
  { bg: "rgba(20, 184, 166, 0.18)",  border: "rgba(20, 184, 166, 0.45)",  text: "#5eead4" }, // teal
  { bg: "rgba(59, 130, 246, 0.18)",  border: "rgba(59, 130, 246, 0.45)",  text: "#93c5fd" }, // blue
  { bg: "rgba(34, 197, 94, 0.15)",   border: "rgba(34, 197, 94, 0.40)",   text: "#86efac" }, // green
  { bg: "rgba(249, 115, 22, 0.15)",  border: "rgba(249, 115, 22, 0.40)",  text: "#fdba74" }, // orange
  { bg: "rgba(236, 72, 153, 0.15)",  border: "rgba(236, 72, 153, 0.40)",  text: "#f9a8d4" }, // pink
  { bg: "rgba(234, 179, 8, 0.15)",   border: "rgba(234, 179, 8, 0.40)",   text: "#fde047" }, // yellow
  { bg: "rgba(6, 182, 212, 0.15)",   border: "rgba(6, 182, 212, 0.40)",   text: "#67e8f9" }, // cyan
  { bg: "rgba(168, 85, 247, 0.15)",  border: "rgba(168, 85, 247, 0.40)",  text: "#d8b4fe" }, // purple
  { bg: "rgba(244, 63, 94, 0.15)",   border: "rgba(244, 63, 94, 0.40)",   text: "#fda4af" }, // rose
  { bg: "rgba(148, 163, 184, 0.12)", border: "rgba(148, 163, 184, 0.35)", text: "#cbd5e1" }, // slate
];

function getProjectPalette(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return MUTED_COLORS[Math.abs(hash) % MUTED_COLORS.length];
}

// Smart 2-letter abbreviation: e.g. "rust-code" → "Rc", "MyAPI" → "MA"
function getProjectAbbr(name: string): string {
  // Split by separators or camelCase boundaries
  const parts = name
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .split(/[\s\-_./\\]+/)
    .filter(Boolean);

  if (parts.length >= 2) {
    return (parts[0][0] + parts[1][0]).toUpperCase();
  }
  if (parts[0].length >= 2) {
    return (parts[0][0] + parts[0][1]).toUpperCase();
  }
  return parts[0][0].toUpperCase();
}

// Detect if the project is a Rust project (has Cargo.toml in path hint)
// We infer from the canonical_path name — backend doesn't expose this yet
function isRustProject(project: ProjectSummary): boolean {
  return (
    project.canonical_path?.toLowerCase().includes("cargo") ||
    project.canonical_path?.toLowerCase().endsWith(".rs") ||
    // heuristic: the project name or path hints at rust
    project.name?.toLowerCase().includes("rust") ||
    false
  );
}

// ──────────────────────────────────────────────────────────────────
// Rust "ferris" minimal SVG icon (simplified crab silhouette)
// ──────────────────────────────────────────────────────────────────
function RustBadge() {
  return (
    <svg
      width="10"
      height="10"
      viewBox="0 0 32 32"
      fill="currentColor"
      class="absolute -bottom-0.5 -right-0.5 text-orange-400 drop-shadow-sm"
      aria-hidden="true"
    >
      <title>Rust project</title>
      {/* Simplified gear/rust logo approximation */}
      <circle cx="16" cy="16" r="7" fill="none" stroke="currentColor" stroke-width="3" />
      <circle cx="16" cy="16" r="3" />
    </svg>
  );
}

// ──────────────────────────────────────────────────────────────────
// ProjectAvatar — the star of the show
// ──────────────────────────────────────────────────────────────────
interface ProjectAvatarProps {
  project: ProjectSummary;
  isActive: boolean;
  isFocused: boolean;
  isDragging: boolean;
  isDragOver: boolean;
  onClick: () => void;
  onContextMenu: (e: MouseEvent, project: ProjectSummary) => void;
  onMouseEnter: (project: ProjectSummary, el: HTMLButtonElement) => void;
  onMouseLeave: () => void;
  ref: (el: HTMLButtonElement) => void;
  shortcutIndex: number; // 0-based, for Ctrl+1..9 hint
  onDragStart?: (e: DragEvent) => void;
  onDragOver?: (e: DragEvent) => void;
  onDragLeave?: (e: DragEvent) => void;
  onDrop?: (e: DragEvent) => void;
  onDragEnd?: (e: DragEvent) => void;
  health?: ProjectHealth;
}

function ProjectAvatar(props: ProjectAvatarProps) {
  const abbr = getProjectAbbr(props.project.name);
  const palette = getProjectPalette(props.project.name);
  const isRust = isRustProject(props.project);

  // Use icon if set, otherwise fall back to abbreviation
  const displayContent = () => {
    if (props.project.icon && props.project.icon.trim() !== "") {
      return <span style={{ "font-size": "16px" }}>{props.project.icon}</span>;
    }
    return <span style={{ "font-size": "11px", "letter-spacing": "0.02em" }}>{abbr}</span>;
  };

  return (
    <div
      class="relative group flex items-center"
      draggable={true}
      onDragStart={props.onDragStart}
      onDragOver={props.onDragOver}
      onDragLeave={props.onDragLeave}
      onDrop={props.onDrop}
      onDragEnd={props.onDragEnd}
      style={{
        opacity: props.isDragging ? "0.4" : "1",
        "border-top": props.isDragOver ? "2px solid var(--primary)" : "2px solid transparent",
        transition: "opacity 200ms, border-top 100ms",
      }}
    >
      {/* Left accent bar — only when active */}
      <div
        class="absolute -left-2 top-1/2 -translate-y-1/2 w-[3px] rounded-r-full transition-all duration-200"
        style={{
          height: props.isActive ? "24px" : "0px",
          background: palette.border,
          opacity: props.isActive ? "1" : "0",
        }}
      />

      <button
        ref={props.ref}
        aria-label={`Switch to project: ${props.project.name}${props.shortcutIndex < 9 ? ` (Ctrl+${props.shortcutIndex + 1})` : ""}`}
        onClick={props.onClick}
        onContextMenu={(e) => props.onContextMenu(e, props.project)}
        onMouseEnter={(e) => props.onMouseEnter(props.project, e.currentTarget)}
        onMouseLeave={props.onMouseLeave}
        class="relative flex items-center justify-center text-xs font-bold transition-all duration-200 shrink-0 cursor-pointer select-none"
        style={{
          width: "36px",
          height: "36px",
          background: props.isActive ? palette.bg : "var(--surface-container)",
          border: `1px solid ${props.isActive ? palette.border : "var(--outline-variant)"}`,
          "border-radius": props.isActive ? "50%" : "10px",
          color: props.isActive ? palette.text : "var(--on-surface-variant)",
          transform: props.isActive ? "scale(1.08)" : "scale(1)",
          "box-shadow": props.isActive
            ? `0 0 12px ${palette.bg}, 0 0 0 1px ${palette.border}`
            : "none",
          opacity: props.isFocused && !props.isActive ? "1" : undefined,
        }}
      >
        {/* Abbreviation or icon */}
        {displayContent()}

        {/* Pin indicator */}
        <Show when={props.project.pinned}>
          <span
            class="absolute -top-0.5 -left-0.5 text-[8px] leading-none"
            style={{ color: "var(--warning-color)" }}
            aria-label="pinned"
          >
            📌
          </span>
        </Show>

        {/* Rust badge */}
        <Show when={isRust}>
          <RustBadge />
        </Show>

        {/* Health badge — bottom-left (opposite corner from Rust badge) */}
        <Show when={props.health}>
          <div class="absolute bottom-0 left-0">
            <ProjectHealthBadge
              status={props.health!.status}
              nErrors={props.health!.n_errors}
              message={props.health!.message}
            />
          </div>
        </Show>

        {/* Activity dot — session count > 0 */}
        <Show when={props.project.session_count > 0 && !props.isActive}>
          <span
            class="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full border border-surface-container-low"
            style={{ background: palette.border }}
          />
        </Show>
      </button>
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────
// Rich Tooltip
// ──────────────────────────────────────────────────────────────────
interface TooltipProps {
  project: ProjectSummary;
  position: { x: number; y: number };
  shortcutIndex: number;
}

function Tooltip(props: TooltipProps) {
  const [pos, setPos] = createSignal(props.position);
  const palette = getProjectPalette(props.project.name);
  const isRust = isRustProject(props.project);

  createEffect(() => {
    const W = 240, H = 90, offset = 10;
    let x = props.position.x + offset;
    let y = props.position.y;
    if (x + W > window.innerWidth) x = props.position.x - W - offset;
    if (y + H > window.innerHeight) y = window.innerHeight - H - offset;
    if (x < 0) x = offset;
    if (y < 0) y = offset;
    setPos({ x, y });
  });

  // Shortened path: show last 2 segments
  const shortPath = () => {
    const parts = props.project.canonical_path?.split(/[\\/]/).filter(Boolean) ?? [];
    return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : props.project.canonical_path;
  };

  return (
    <div
      class="fixed z-[300] pointer-events-none rounded-xl shadow-xl"
      style={{
        left: `${pos().x}px`,
        top: `${pos().y}px`,
        background: "var(--surface-container-high)",
        border: `1px solid ${palette.border}`,
        "min-width": "200px",
        "max-width": "260px",
      }}
    >
      {/* Color accent bar on top */}
      <div
        class="rounded-t-xl h-[3px]"
        style={{ background: `linear-gradient(90deg, ${palette.border}, transparent)` }}
      />
      <div class="px-3 py-2.5">
        <div class="flex items-center gap-2 mb-1">
          <span
            class="text-xs font-bold leading-tight truncate"
            style={{ color: palette.text }}
          >
            {props.project.name}
          </span>
          <Show when={isRust}>
            <span class="text-[9px] px-1.5 py-0.5 rounded font-semibold uppercase tracking-wide"
            style={{ background: "var(--warning-bg-subtle)", color: "var(--warning-color)" }}>
              Rust
            </span>
          </Show>
          <Show when={props.shortcutIndex < 9}>
            <span class="ml-auto text-[9px] font-mono px-1 py-0.5 rounded shrink-0"
              style={{ background: "var(--surface-container)", color: "var(--outline)", border: "1px solid var(--outline-variant)" }}>
              ⌃{props.shortcutIndex + 1}
            </span>
          </Show>
        </div>
        <div class="text-[10px] text-outline truncate mb-1" title={props.project.canonical_path}>
          {shortPath()}
        </div>
        <div class="flex items-center gap-3 text-[10px] text-outline-variant">
          <span class="flex items-center gap-1">
            <span class="material-symbols-outlined text-[11px]">chat_bubble</span>
            {props.project.session_count} session{props.project.session_count !== 1 ? "s" : ""}
          </span>
        </div>
      </div>
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────
// Context Menu (unchanged logic, improved styling)
// ──────────────────────────────────────────────────────────────────
interface ContextMenuProps {
  project: ProjectSummary;
  position: { x: number; y: number };
  onOpen: () => void;
  onCopyPath: () => void;
  onDelete: () => void;
  onTogglePin: () => void;
  onClose: () => void;
}

function ContextMenu(props: ContextMenuProps) {
  const [adjustedPos, setAdjustedPos] = createSignal(props.position);

  createEffect(() => {
    const menuWidth = 160, menuHeight = 152, offset = 4;
    let x = props.position.x, y = props.position.y;
    if (x + menuWidth > window.innerWidth) x = window.innerWidth - menuWidth - offset;
    if (y + menuHeight > window.innerHeight) y = window.innerHeight - menuHeight - offset;
    if (x < 0) x = offset;
    if (y < 0) y = offset;
    setAdjustedPos({ x, y });
  });

  createEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (!(e.target as HTMLElement).closest('[data-context-menu="true"]')) props.onClose();
    };
    const handleKey = (e: KeyboardEvent) => { if (e.key === "Escape") props.onClose(); };
    document.addEventListener("click", handleClick);
    document.addEventListener("keydown", handleKey);
    onCleanup(() => {
      document.removeEventListener("click", handleClick);
      document.removeEventListener("keydown", handleKey);
    });
  });

  return (
    <div
      data-context-menu="true"
      role="menu"
      class="fixed z-[300] rounded-xl shadow-2xl py-1 overflow-hidden"
      style={{
        left: `${adjustedPos().x}px`,
        top: `${adjustedPos().y}px`,
        background: "var(--surface-container-high)",
        border: "1px solid var(--outline-variant)",
        "min-width": "160px",
      }}
    >
      {[
        { icon: "open_in_new", label: "Open", action: () => { props.onOpen(); props.onClose(); }, danger: false },
        {
          icon: "content_copy", label: "Copy Path", danger: false,
          action: async () => { await navigator.clipboard.writeText(props.project.canonical_path); props.onCopyPath(); props.onClose(); }
        },
        {
          icon: props.project.pinned ? "push_pin" : "push_pin",
          label: props.project.pinned ? "Unpin" : "Pin",
          danger: false,
          action: () => { props.onTogglePin(); props.onClose(); }
        },
        { icon: "delete", label: "Delete", action: () => { props.onDelete(); props.onClose(); }, danger: true },
      ].map((item) => (
        <button
          role="menuitem"
          onClick={item.action}
          class={`w-full px-3 py-2 text-sm text-left flex items-center gap-2 transition-colors duration-150 hover:bg-surface-container-highest ${item.danger ? "text-error" : "text-on-surface"}`}
        >
          <span class={`material-symbols-outlined text-base ${item.danger ? "text-error" : "text-outline"}`}>{item.icon}</span>
          {item.label}
        </button>
      ))}
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────
// Delete Dialog (unchanged logic, improved styling)
// ──────────────────────────────────────────────────────────────────
interface DeleteDialogProps {
  projectName: string;
  isOpen: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

function DeleteDialog(props: DeleteDialogProps) {
  createEffect(() => {
    if (!props.isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => { if (e.key === "Escape") props.onCancel(); };
    document.addEventListener("keydown", handleKeyDown);
    onCleanup(() => document.removeEventListener("keydown", handleKeyDown));
  });

  return (
    <Show when={props.isOpen}>
      <div class="fixed inset-0 z-[400] flex items-center justify-center">
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={props.onCancel} />
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby="delete-dialog-title"
          class="relative rounded-2xl shadow-2xl w-full max-w-sm mx-4 p-6"
          style={{ background: "var(--surface-container)", border: "1px solid var(--outline-variant)" }}
        >
          <div class="flex items-center gap-3 mb-4">
            <span class="material-symbols-outlined text-2xl text-error">warning</span>
            <h2 id="delete-dialog-title" class="text-base font-semibold text-on-surface">Delete Project</h2>
          </div>
          <p class="text-sm text-on-surface-variant mb-1">Are you sure you want to delete:</p>
          <p class="text-sm font-semibold text-on-surface mb-6">"{props.projectName}"</p>
          <div class="flex justify-end gap-3">
            <button onClick={props.onCancel} class="px-4 py-2 text-sm font-medium text-on-surface-variant hover:bg-surface-container-high rounded-lg transition-colors">
              Cancel
            </button>
            <button onClick={props.onConfirm} class="px-4 py-2 text-sm font-medium bg-error text-on-error rounded-lg hover:opacity-90 transition-all">
              Delete
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}

// ──────────────────────────────────────────────────────────────────
// Add Project Dialog (unchanged logic, improved styling)
// ──────────────────────────────────────────────────────────────────
interface AddProjectDialogProps {
  isOpen: boolean;
  initialPath?: string;
  initialName?: string;
  onClose: () => void;
  onProjectCreated: () => void;
}

function AddProjectDialog(props: AddProjectDialogProps) {
  const [path, setPath] = createSignal(props.initialPath || "");
  const [name, setName] = createSignal(props.initialName || "");
  const [isCreating, setIsCreating] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  createEffect(() => {
    if (props.isOpen) {
      if (props.initialPath !== undefined) setPath(props.initialPath);
      if (props.initialName !== undefined) setName(props.initialName);
    }
  });

  const handleCreate = async () => {
    const pathValue = path().trim();
    if (!pathValue) { setError("Path is required"); return; }
    setIsCreating(true);
    setError(null);
    try {
      await createProject(pathValue, name().trim() || undefined);
      props.onProjectCreated();
      props.onClose();
      setPath(""); setName("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create project");
    } finally {
      setIsCreating(false);
    }
  };

  const handleOpenNativePicker = async () => {
    const selected = await openFolderPicker();
    if (selected) {
      setPath(selected);
      // Auto-populate name from basename if name is empty
      if (!name().trim()) {
        const basename = selected.split(/[\\/]/).pop() || "";
        setName(basename);
      }
    }
  };

  return (
    <Show when={props.isOpen}>
      <div class="fixed inset-0 z-[200] flex items-center justify-center">
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={props.onClose} />
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby="add-project-dialog-title"
          class="relative rounded-2xl shadow-2xl w-full max-w-md mx-4 p-6"
          style={{ background: "var(--surface-container)", border: "1px solid var(--outline-variant)" }}
        >
          <div class="flex items-center justify-between mb-5">
            <h2 id="add-project-dialog-title" class="text-base font-semibold text-on-surface">Add Project</h2>
            <button onClick={props.onClose} class="p-1 hover:bg-surface-container-high rounded-md transition-colors" aria-label="Close dialog">
              <span class="material-symbols-outlined text-xl text-on-surface-variant">close</span>
            </button>
          </div>

          <div class="space-y-4">
            <div>
              <label for="project-path-input" class="block text-xs font-semibold text-on-surface-variant mb-1.5 uppercase tracking-wider">
                Workspace Path
              </label>
              <div class="flex gap-2">
                <input
                  id="project-path-input"
                  type="text"
                  value={path()}
                  onInput={(e) => {
                    setPath(e.currentTarget.value);
                    // Auto-populate name from basename if name is empty
                    if (!name().trim()) {
                      const basename = e.currentTarget.value.split(/[\\/]/).pop() || "";
                      if (basename) setName(basename);
                    }
                  }}
                  placeholder="/path/to/your/project"
                  class="flex-1 bg-surface-container-low text-on-surface px-3 py-2 rounded-lg text-sm transition-colors focus:outline-none"
                  style={{ border: "1px solid var(--outline-variant)" }}
                  onFocus={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "var(--primary)"; }}
                  onBlur={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "var(--outline-variant)"; }}
                />
                <Show when={typeof window !== "undefined" && (window as any).__TAURI__}>
                  <button onClick={handleOpenNativePicker} class="px-3 py-2 bg-surface-container-high hover:bg-surface-container-highest rounded-lg transition-colors" style={{ border: "1px solid var(--outline-variant)" }} aria-label="Pick folder">
                    <span class="material-symbols-outlined text-base text-outline">folder_open</span>
                  </button>
                </Show>
              </div>
              <p class="text-[10px] text-outline mt-1">Filesystem path to your project workspace</p>
            </div>

            <div>
              <label for="project-name-input" class="block text-xs font-semibold text-on-surface-variant mb-1.5 uppercase tracking-wider">
                Name <span class="font-normal normal-case text-outline">(optional)</span>
              </label>
              <input
                id="project-name-input"
                type="text"
                value={name()}
                onInput={(e) => setName(e.currentTarget.value)}
                placeholder="My Project"
                class="w-full bg-surface-container-low text-on-surface px-3 py-2 rounded-lg text-sm transition-colors focus:outline-none"
                style={{ border: "1px solid var(--outline-variant)" }}
                onFocus={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "var(--primary)"; }}
                onBlur={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "var(--outline-variant)"; }}
              />
            </div>

            <Show when={error()}>
              <div class="p-3 rounded-lg" style={{ background: "var(--error-bg-subtle)", border: "1px solid var(--error-border-subtle)" }}>
                <p class="text-sm text-error">{error()}</p>
              </div>
            </Show>
          </div>

          <div class="flex justify-end gap-3 mt-6">
            <button onClick={props.onClose} class="px-4 py-2 text-sm font-medium text-on-surface-variant hover:bg-surface-container-high rounded-lg transition-colors">
              Cancel
            </button>
            <button
              onClick={handleCreate}
              disabled={isCreating() || !path().trim()}
              class="px-4 py-2 text-sm font-semibold bg-primary text-on-primary rounded-lg hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed transition-all flex items-center gap-2"
            >
              <Show when={isCreating()}>
                <span class="w-3.5 h-3.5 border-2 border-on-primary border-t-transparent rounded-full animate-spin" />
              </Show>
              Add Project
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}

// ──────────────────────────────────────────────────────────────────
// Main ProjectRail
// ──────────────────────────────────────────────────────────────────
export default function ProjectRail() {
  const projectContext = useProjectContext();
  const [showAddDialog, setShowAddDialog] = createSignal(false);

  // Drag-and-drop state
  const [projectOrder, setProjectOrder] = createSignal<string[]>([]);
  const [draggingId, setDraggingId] = createSignal<string | null>(null);
  const [dragOverId, setDragOverId] = createSignal<string | null>(null);

  // Sorted projects based on drag order, with pinned projects first
  const sortedProjects = createMemo(() => {
    const projects = projectContext.projects();
    const order = projectOrder();
    if (order.length === 0) {
      // No drag order: pinned first (preserve original order), then unpinned (preserve original order)
      const pinned = projects.filter(p => p.pinned);
      const unpinned = projects.filter(p => !p.pinned);
      return [...pinned, ...unpinned];
    }
    return [...projects].sort((a, b) => {
      // Pinned first
      if (a.pinned && !b.pinned) return -1;
      if (!a.pinned && b.pinned) return 1;
      // Within each group, preserve drag order
      const ai = order.indexOf(a.id);
      const bi = order.indexOf(b.id);
      if (ai === -1) return 1;
      if (bi === -1) return -1;
      return ai - bi;
    });
  });

  // Initialize order from localStorage
  createEffect(() => {
    const projects = projectContext.projects();
    if (projects.length === 0) return;
    const saved = localStorage.getItem("rcode:project-order");
    if (saved) {
      try {
        const parsed: string[] = JSON.parse(saved);
        setProjectOrder(parsed);
      } catch { /* ignore */ }
    } else {
      setProjectOrder(projects.map(p => p.id));
    }
  });

  // Tooltip state
  const [hoveredProject, setHoveredProject] = createSignal<ProjectSummary | null>(null);
  const [tooltipPosition, setTooltipPosition] = createSignal({ x: 0, y: 0 });
  const [showTooltip, setShowTooltip] = createSignal(false);
  let showTimer: ReturnType<typeof setTimeout> | null = null;
  let hideTimer: ReturnType<typeof setTimeout> | null = null;
  const avatarRefs = new Map<string, HTMLButtonElement>();

  // Context menu state
  const [contextMenuProject, setContextMenuProject] = createSignal<ProjectSummary | null>(null);
  const [contextMenuPosition, setContextMenuPosition] = createSignal({ x: 0, y: 0 });

  // Delete dialog state
  const [deleteDialogProject, setDeleteDialogProject] = createSignal<ProjectSummary | null>(null);

  // Keyboard navigation
  const [focusedIndex, setFocusedIndex] = createSignal<number>(-1);

  // Pre-filled values from native picker
  let pendingProjectPath = "";
  let pendingProjectName = "";

  const handleSelectProject = (projectId: string) => {
    projectContext.setActiveProject(projectId);
  };

  const handleAddProject = async () => {
    const selected = await openFolderPicker();
    if (selected) {
      pendingProjectPath = selected;
      pendingProjectName = selected.split(/[\\/]/).pop() || "";
      setShowAddDialog(true);
    } else {
      // User cancelled or not in Tauri - still show dialog for manual path entry
      setShowAddDialog(true);
    }
  };

  // Listen for add-project event dispatched from App (WelcomeScreen/RecentProjectsView CTAs)
  onMount(() => {
    const handler = () => handleAddProject();
    window.addEventListener("rcode:open-add-project", handler);
    onCleanup(() => window.removeEventListener("rcode:open-add-project", handler));
  });

  const handleProjectCreated = () => {
    void projectContext.refreshProjects();
  };

  // Tooltip handlers — show after 300ms delay
  const handleMouseEnter = (project: ProjectSummary, el: HTMLButtonElement) => {
    setHoveredProject(project);
    if (hideTimer) { clearTimeout(hideTimer); hideTimer = null; }
    if (!showTimer) {
      showTimer = setTimeout(() => {
        const rect = el.getBoundingClientRect();
        setTooltipPosition({ x: rect.right, y: rect.top });
        setShowTooltip(true);
        showTimer = null;
      }, 300);
    }
  };

  const handleMouseLeave = () => {
    if (showTimer) { clearTimeout(showTimer); showTimer = null; }
    if (!hideTimer) {
      hideTimer = setTimeout(() => {
        setShowTooltip(false);
        setHoveredProject(null);
        hideTimer = null;
      }, 100);
    }
  };

  // Context menu
  const handleContextMenu = (e: MouseEvent, project: ProjectSummary) => {
    e.preventDefault();
    setContextMenuProject(project);
    setContextMenuPosition({ x: e.clientX, y: e.clientY });
  };

  const handleDeleteClick = () => {
    const project = contextMenuProject();
    if (project) setDeleteDialogProject(project);
  };

  const handleTogglePin = async () => {
    const project = contextMenuProject();
    if (project) {
      await projectContext.updateProject(project.id, {
        name: project.name,
        pinned: !project.pinned,
        icon: project.icon,
      });
    }
  };

  const handleConfirmDelete = async () => {
    const project = deleteDialogProject();
    if (project) {
      await projectContext.removeProject(project.id);
      setDeleteDialogProject(null);
    }
  };

  // Keyboard navigation (ArrowUp/Down + Enter) + Ctrl+1..9
  const handleKeyDown = (e: KeyboardEvent) => {
    const projects = sortedProjects();
    if (projects.length === 0) return;

    // Ctrl+1..9 quick switch
    if ((e.ctrlKey || e.metaKey) && e.key >= "1" && e.key <= "9") {
      const idx = parseInt(e.key, 10) - 1;
      if (idx < projects.length) {
        e.preventDefault();
        handleSelectProject(projects[idx].id);
      }
      return;
    }

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setFocusedIndex((prev) => (prev < 0 ? 0 : (prev + 1) % projects.length));
        break;
      case "ArrowUp":
        e.preventDefault();
        setFocusedIndex((prev) => (prev < 0 ? projects.length - 1 : (prev - 1 + projects.length) % projects.length));
        break;
      case "Enter": {
        e.preventDefault();
        const idx = focusedIndex();
        if (idx >= 0 && idx < projects.length) handleSelectProject(projects[idx].id);
        break;
      }
    }
  };

  const setAvatarRef = (projectId: string) => (el: HTMLButtonElement) => {
    avatarRefs.set(projectId, el);
  };

  // Index for hoveredProject (for tooltip shortcut hint)
  const hoveredIndex = createMemo(() => {
    const h = hoveredProject();
    if (!h) return -1;
    return projectContext.projects().findIndex((p) => p.id === h.id);
  });

  return (
    <>
      <div
        data-component="project-rail"
        role="navigation"
        aria-label="Project list"
        class="flex flex-col h-full shrink-0 items-center py-3 gap-1 focus:outline-none"
        style={{
          width: "52px",
          background: "var(--surface-container-low)",
          "border-right": "1px solid var(--outline-variant)",
        }}
        onKeyDown={handleKeyDown}
        tabIndex={0}
      >
        {/* RCode brand dot at top */}
        <div
          class="w-7 h-7 rounded-lg flex items-center justify-center mb-1 shrink-0"
          style={{ background: "var(--accent-bg-hover)", border: "1px solid var(--accent-border-hover)" }}
          title="RCode"
        >
          <span class="material-symbols-outlined text-[14px]" style={{ color: "var(--info-color)", "font-variation-settings": "'FILL' 1" }}>
            code
          </span>
        </div>

        {/* Divider */}
        <div class="w-5 h-px mb-1 shrink-0" style={{ background: "var(--separator)" }} />

        {/* Project list */}
        <div class="flex flex-col items-center gap-2.5 flex-1 overflow-y-auto w-full px-2 custom-scrollbar">
          <For each={sortedProjects()}>
            {(project, index) => (
              <ProjectAvatar
                project={project}
                isActive={project.id === projectContext.activeProjectId()}
                isFocused={focusedIndex() === index()}
                isDragging={draggingId() === project.id}
                isDragOver={dragOverId() === project.id}
                onClick={() => handleSelectProject(project.id)}
                onContextMenu={handleContextMenu}
                onMouseEnter={handleMouseEnter}
                onMouseLeave={handleMouseLeave}
                ref={setAvatarRef(project.id)}
                shortcutIndex={index()}
                health={projectContext.getHealth(project.id)}
                onDragStart={(e) => {
                  setDraggingId(project.id);
                  e.dataTransfer!.effectAllowed = "move";
                }}
                onDragOver={(e) => {
                  e.preventDefault();
                  setDragOverId(project.id);
                  e.dataTransfer!.dropEffect = "move";
                }}
                onDragLeave={() => {
                  setDragOverId(null);
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  const from = draggingId();
                  const to = project.id;
                  if (!from || from === to) return;
                  const current = sortedProjects().map(p => p.id);
                  const fromIdx = current.indexOf(from);
                  const toIdx = current.indexOf(to);
                  const newOrder = [...current];
                  newOrder.splice(fromIdx, 1);
                  newOrder.splice(toIdx, 0, from);
                  setProjectOrder(newOrder);
                  localStorage.setItem("rcode:project-order", JSON.stringify(newOrder));
                  setDraggingId(null);
                  setDragOverId(null);
                }}
                onDragEnd={() => {
                  setDraggingId(null);
                  setDragOverId(null);
                }}
              />
            )}
          </For>
        </div>

        {/* Divider */}
        <div class="w-5 h-px mt-1 mb-1 shrink-0" style={{ background: "var(--separator)" }} />

        {/* Add project button */}
        <button
          onClick={handleAddProject}
          aria-label="Add new project"
          class="w-9 h-9 rounded-xl flex items-center justify-center transition-all duration-200 shrink-0 group"
          style={{
            background: "var(--surface-container)",
            border: "1px dashed var(--outline-variant)",
            color: "var(--outline)",
          }}
          title="Add Project (new workspace)"
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLElement).style.borderColor = "var(--primary)";
            (e.currentTarget as HTMLElement).style.color = "var(--primary)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLElement).style.borderColor = "var(--outline-variant)";
            (e.currentTarget as HTMLElement).style.color = "var(--outline)";
          }}
        >
          <span class="material-symbols-outlined text-base">add</span>
        </button>
      </div>

      {/* Rich Tooltip */}
      <Show when={showTooltip() && hoveredProject()}>
        <Tooltip
          project={hoveredProject()!}
          position={tooltipPosition()}
          shortcutIndex={hoveredIndex()}
        />
      </Show>

      {/* Context Menu */}
      <Show when={contextMenuProject()}>
        <ContextMenu
          project={contextMenuProject()!}
          position={contextMenuPosition()}
          onOpen={() => handleSelectProject(contextMenuProject()!.id)}
          onCopyPath={() => {}}
          onDelete={handleDeleteClick}
          onTogglePin={handleTogglePin}
          onClose={() => setContextMenuProject(null)}
        />
      </Show>

      {/* Delete Dialog */}
      <Show when={deleteDialogProject()}>
        <DeleteDialog
          projectName={deleteDialogProject()!.name}
          isOpen={true}
          onConfirm={handleConfirmDelete}
          onCancel={() => setDeleteDialogProject(null)}
        />
      </Show>

      <AddProjectDialog
        isOpen={showAddDialog()}
        initialPath={pendingProjectPath}
        initialName={pendingProjectName}
        onClose={() => { setShowAddDialog(false); pendingProjectPath = ""; pendingProjectName = ""; }}
        onProjectCreated={handleProjectCreated}
      />
    </>
  );
}
