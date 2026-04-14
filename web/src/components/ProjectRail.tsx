import { createSignal, createMemo, createEffect, onCleanup, For, Show } from "solid-js";
import { useProjectContext } from "../context/ProjectContext";
import { createProject } from "../api/projects";
import type { ProjectSummary } from "../api/projects";

interface ProjectAvatarProps {
  project: ProjectSummary;
  isActive: boolean;
  isFocused: boolean;
  onClick: () => void;
  onContextMenu: (e: MouseEvent, project: ProjectSummary) => void;
  onMouseEnter: (project: ProjectSummary) => void;
  onMouseLeave: () => void;
  ref: (el: HTMLButtonElement) => void;
}

// Deterministic color based on project name
function getProjectColor(name: string): string {
  const colors = [
    "#6366f1", // indigo
    "#8b5cf6", // violet
    "#ec4899", // pink
    "#f43f5e", // rose
    "#ef4444", // red
    "#f97316", // orange
    "#eab308", // yellow
    "#22c55e", // green
    "#14b8a6", // teal
    "#06b6d4", // cyan
    "#3b82f6", // blue
    "#a855f7", // purple
  ];
  
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  
  return colors[Math.abs(hash) % colors.length];
}

function ProjectAvatar(props: ProjectAvatarProps) {
  const initial = props.project.name.charAt(0).toUpperCase();
  const color = getProjectColor(props.project.name);
  
  return (
    <button
      ref={props.ref}
      aria-label={`Switch to project: ${props.project.name}`}
      onClick={props.onClick}
      onContextMenu={(e) => props.onContextMenu(e, props.project)}
      onMouseEnter={() => props.onMouseEnter(props.project)}
      onMouseLeave={props.onMouseLeave}
      class={`w-10 h-10 rounded-lg flex items-center justify-center text-sm font-bold transition-all shrink-0 cursor-pointer relative ${
        props.isActive
          ? "ring-2 ring-primary ring-offset-1 ring-offset-background scale-110"
          : props.isFocused
          ? "ring-2 ring-secondary ring-offset-1 ring-offset-background"
          : "opacity-70 hover:opacity-100 hover:scale-105"
      }`}
      style={{ "background-color": color }}
    >
      {initial}
      {/* Session activity badge */}
      <Show when={props.project.session_count > 0}>
        <span class="absolute -top-0.5 -right-0.5 w-2.5 h-2.5 rounded-full bg-secondary border border-background shrink-0" />
      </Show>
    </button>
  );
}

interface TooltipProps {
  project: ProjectSummary;
  position: { x: number; y: number };
}

function Tooltip(props: TooltipProps) {
  const [adjustedPos, setAdjustedPos] = createSignal(props.position);
  
  createEffect(() => {
    const tooltipWidth = 220;
    const tooltipHeight = 70;
    const offset = 8;
    
    let x = props.position.x + offset;
    let y = props.position.y;
    
    // Clamp to viewport
    if (x + tooltipWidth > window.innerWidth) {
      x = props.position.x - tooltipWidth - offset;
    }
    if (y + tooltipHeight > window.innerHeight) {
      y = window.innerHeight - tooltipHeight - offset;
    }
    if (x < 0) x = offset;
    if (y < 0) y = offset;
    
    setAdjustedPos({ x, y });
  });
  
  return (
    <div
      class="fixed z-[300] bg-surface-container-high border border-outline-variant/30 rounded-lg shadow-xl px-3 py-2 pointer-events-none"
      style={{ left: `${adjustedPos().x}px`, top: `${adjustedPos().y}px` }}
    >
      <div class="text-sm font-semibold text-on-surface truncate max-w-[200px]">{props.project.name}</div>
      <div class="text-xs text-on-surface-variant truncate max-w-[200px] mt-0.5">{props.project.canonical_path}</div>
      <div class="text-xs text-outline mt-1">
        {props.project.session_count} session{props.project.session_count !== 1 ? "s" : ""}
      </div>
    </div>
  );
}

interface ContextMenuProps {
  project: ProjectSummary;
  position: { x: number; y: number };
  onOpen: () => void;
  onCopyPath: () => void;
  onDelete: () => void;
  onClose: () => void;
}

function ContextMenu(props: ContextMenuProps) {
  const [adjustedPos, setAdjustedPos] = createSignal(props.position);
  
  createEffect(() => {
    const menuWidth = 150;
    const menuHeight = 120;
    const offset = 4;
    
    let x = props.position.x;
    let y = props.position.y;
    
    // Clamp to viewport
    if (x + menuWidth > window.innerWidth) {
      x = window.innerWidth - menuWidth - offset;
    }
    if (y + menuHeight > window.innerHeight) {
      y = window.innerHeight - menuHeight - offset;
    }
    if (x < 0) x = offset;
    if (y < 0) y = offset;
    
    setAdjustedPos({ x, y });
  });
  
  // Close on outside click
  createEffect(() => {
    const handleClick = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (!target.closest('[data-context-menu="true"]')) {
        props.onClose();
      }
    };
    
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        props.onClose();
      }
    };
    
    document.addEventListener("click", handleClick);
    document.addEventListener("keydown", handleKeyDown);
    
    onCleanup(() => {
      document.removeEventListener("click", handleClick);
      document.removeEventListener("keydown", handleKeyDown);
    });
  });
  
  const handleOpen = () => {
    props.onOpen();
    props.onClose();
  };
  
  const handleCopyPath = async () => {
    await navigator.clipboard.writeText(props.project.canonical_path);
    props.onCopyPath();
    props.onClose();
  };
  
  const handleDelete = () => {
    props.onDelete();
    props.onClose();
  };
  
  return (
    <div
      data-context-menu="true"
      role="menu"
      class="fixed z-[300] bg-surface-container border border-outline-variant/30 rounded-lg shadow-xl py-1 min-w-[150px]"
      style={{ left: `${adjustedPos().x}px`, top: `${adjustedPos().y}px` }}
    >
      <button
        role="menuitem"
        onClick={handleOpen}
        class="w-full px-3 py-2 text-sm text-left text-on-surface hover:bg-surface-container-high flex items-center gap-2"
      >
        <span class="material-symbols-outlined text-base text-outline">open_in_new</span>
        Open
      </button>
      <button
        role="menuitem"
        onClick={handleCopyPath}
        class="w-full px-3 py-2 text-sm text-left text-on-surface hover:bg-surface-container-high flex items-center gap-2"
      >
        <span class="material-symbols-outlined text-base text-outline">content_copy</span>
        Copy Path
      </button>
      <button
        role="menuitem"
        onClick={handleDelete}
        class="w-full px-3 py-2 text-sm text-left text-error hover:bg-surface-container-high flex items-center gap-2"
      >
        <span class="material-symbols-outlined text-base text-error">delete</span>
        Delete
      </button>
    </div>
  );
}

interface DeleteDialogProps {
  projectName: string;
  isOpen: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

function DeleteDialog(props: DeleteDialogProps) {
  // T2.3: Close on Escape key
  createEffect(() => {
    if (!props.isOpen) return;
    
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        props.onCancel();
      }
    };
    
    document.addEventListener("keydown", handleKeyDown);
    onCleanup(() => document.removeEventListener("keydown", handleKeyDown));
  });
  
  return (
    <Show when={props.isOpen}>
      <div class="fixed inset-0 z-[400] flex items-center justify-center">
        {/* Backdrop */}
        <div 
          class="absolute inset-0 bg-black/50 backdrop-blur-sm"
          onClick={props.onCancel}
        />
        
        {/* Dialog */}
        <div role="dialog" aria-modal="true" aria-labelledby="delete-dialog-title" class="relative bg-surface-container border border-outline-variant/30 rounded-xl shadow-2xl w-full max-w-sm mx-4 p-6">
          <div class="flex items-center gap-3 mb-4">
            <span class="material-symbols-outlined text-2xl text-error">warning</span>
            <h2 id="delete-dialog-title" class="text-lg font-semibold text-on-surface">Delete Project</h2>
          </div>
          
          <p class="text-sm text-on-surface-variant mb-1">
            Are you sure you want to delete this project?
          </p>
          <p class="text-sm font-medium text-on-surface mb-6">
            "{props.projectName}"
          </p>
          
          <div class="flex justify-end gap-3">
            <button
              onClick={props.onCancel}
              class="px-4 py-2 text-sm font-medium text-on-surface-variant hover:bg-surface-container-high rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={props.onConfirm}
              class="px-4 py-2 text-sm font-medium bg-error text-on-error rounded-lg hover:opacity-90 transition-all"
            >
              Delete
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}

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
  
  // Update when initial values change (dialog reopens with new values)
  createEffect(() => {
    if (props.isOpen) {
      if (props.initialPath !== undefined) setPath(props.initialPath);
      if (props.initialName !== undefined) setName(props.initialName);
    }
  });
  
  const handleCreate = async () => {
    const pathValue = path().trim();
    if (!pathValue) {
      setError("Path is required");
      return;
    }
    
    setIsCreating(true);
    setError(null);
    
    try {
      await createProject(pathValue, name().trim() || undefined);
      props.onProjectCreated();
      props.onClose();
      setPath("");
      setName("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create project");
    } finally {
      setIsCreating(false);
    }
  };
  
  const handleOpenNativePicker = async () => {
    // T4.3: Tauri native folder picker
    if (typeof window !== "undefined" && (window as any).__TAURI__) {
      try {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const selected = await open({
          directory: true,
          multiple: false,
          title: "Select Project Folder",
        });
        if (selected && typeof selected === "string") {
          setPath(selected);
        }
      } catch (err) {
        console.warn("Native folder picker failed, using text input:", err);
      }
    }
  };
  
  return (
    <Show when={props.isOpen}>
      <div class="fixed inset-0 z-[200] flex items-center justify-center">
        {/* Backdrop */}
        <div 
          class="absolute inset-0 bg-black/50 backdrop-blur-sm"
          onClick={props.onClose}
        />
        
        {/* Dialog */}
        <div role="dialog" aria-modal="true" aria-labelledby="add-project-dialog-title" class="relative bg-surface-container border border-outline-variant/30 rounded-xl shadow-2xl w-full max-w-md mx-4 p-6">
          <div class="flex items-center justify-between mb-4">
            <h2 id="add-project-dialog-title" class="text-lg font-semibold text-on-surface">Add Project</h2>
            <button
              onClick={props.onClose}
              class="p-1 hover:bg-surface-container-high rounded-md transition-colors"
              aria-label="Close dialog"
            >
              <span class="material-symbols-outlined text-xl text-on-surface-variant">close</span>
            </button>
          </div>
          
          <div class="space-y-4">
            <div>
              <label for="project-path-input" class="block text-sm font-medium text-on-surface-variant mb-1.5">
                Workspace Path
              </label>
              <div class="flex gap-2">
                <input
                  id="project-path-input"
                  type="text"
                  value={path()}
                  onInput={(e) => setPath(e.currentTarget.value)}
                  placeholder="/path/to/your/project"
                  class="flex-1 bg-surface-container-low text-on-surface px-3 py-2 rounded-lg border border-outline-variant/30 focus:border-primary focus:outline-none transition-colors text-sm"
                />
                <Show when={typeof window !== "undefined" && (window as any).__TAURI__}>
                  <button
                    onClick={handleOpenNativePicker}
                    class="px-3 py-2 bg-surface-container-high hover:bg-surface-container-highest rounded-lg border border-outline-variant/30 transition-colors"
                    aria-label="Pick folder (Tauri)"
                  >
                    <span class="material-symbols-outlined text-base text-outline">folder_open</span>
                  </button>
                </Show>
              </div>
              <p class="text-xs text-outline mt-1">Enter the filesystem path to your project workspace</p>
            </div>
            
            <div>
              <label for="project-name-input" class="block text-sm font-medium text-on-surface-variant mb-1.5">
                Name (optional)
              </label>
              <input
                id="project-name-input"
                type="text"
                value={name()}
                onInput={(e) => setName(e.currentTarget.value)}
                placeholder="My Project"
                class="w-full bg-surface-container-low text-on-surface px-3 py-2 rounded-lg border border-outline-variant/30 focus:border-primary focus:outline-none transition-colors text-sm"
              />
            </div>
            
            <Show when={error()}>
              <div class="p-3 bg-error-container/20 border border-error/30 rounded-lg">
                <p class="text-sm text-error">{error()}</p>
              </div>
            </Show>
          </div>
          
          <div class="flex justify-end gap-3 mt-6">
            <button
              onClick={props.onClose}
              class="px-4 py-2 text-sm font-medium text-on-surface-variant hover:bg-surface-container-high rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleCreate}
              disabled={isCreating() || !path().trim()}
              class="px-4 py-2 text-sm font-medium bg-primary text-on-primary rounded-lg hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed transition-all flex items-center gap-2"
            >
              <Show when={isCreating()}>
                <span class="w-4 h-4 border-2 border-on-primary border-t-transparent rounded-full animate-spin" />
              </Show>
              Add Project
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}

export default function ProjectRail() {
  const projectContext = useProjectContext();
  const [showAddDialog, setShowAddDialog] = createSignal(false);
  
  // T2.1: Tooltip state
  const [hoveredProject, setHoveredProject] = createSignal<ProjectSummary | null>(null);
  const [tooltipPosition, setTooltipPosition] = createSignal({ x: 0, y: 0 });
  const [showTooltip, setShowTooltip] = createSignal(false);
  let showTimer: ReturnType<typeof setTimeout> | null = null;
  let hideTimer: ReturnType<typeof setTimeout> | null = null;
  const avatarRefs = new Map<string, HTMLButtonElement>();
  
  // T2.2: Context menu state
  const [contextMenuProject, setContextMenuProject] = createSignal<ProjectSummary | null>(null);
  const [contextMenuPosition, setContextMenuPosition] = createSignal({ x: 0, y: 0 });
  
  // T2.3: Delete dialog state
  const [deleteDialogProject, setDeleteDialogProject] = createSignal<ProjectSummary | null>(null);
  
  // T4.2: Keyboard navigation
  const [focusedIndex, setFocusedIndex] = createSignal<number>(-1);
  
  const handleSelectProject = (projectId: string) => {
    projectContext.setActiveProject(projectId);
  };
  
  const handleAddProject = async () => {
    // T4.3/REQ-PR-06: When Tauri is present, open native folder picker first
    if (typeof window !== "undefined" && (window as any).__TAURI__) {
      try {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const selected = await open({
          directory: true,
          multiple: false,
          title: "Select Project Folder",
        });
        if (selected && typeof selected === "string") {
          // User selected a folder - pre-fill dialog with path and derived name
          // Extract folder name from path
          const folderName = selected.split(/[\\/]/).pop() || "";
          // We need to pass the pre-filled values to the dialog
          // Use a temporary signal to pre-populate
          pendingProjectPath = selected;
          pendingProjectName = folderName;
          setShowAddDialog(true);
        }
        // If cancelled (null/undefined), do NOT open dialog
      } catch (err) {
        console.warn("Native folder picker failed, opening text dialog:", err);
        setShowAddDialog(true);
      }
    } else {
      // Web context - open dialog directly
      setShowAddDialog(true);
    }
  };
  
  // T4.3: Temporary storage for pre-filled project data from native picker
  let pendingProjectPath = "";
  let pendingProjectName = "";
  
  const handleProjectCreated = () => {
    void projectContext.refreshProjects();
  };
  
  // T2.1: Tooltip handlers
  const handleMouseEnter = (project: ProjectSummary) => {
    setHoveredProject(project);
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    if (!showTimer) {
      showTimer = setTimeout(() => {
        // Get position from avatar ref
        const el = avatarRefs.get(project.id);
        if (el) {
          const rect = el.getBoundingClientRect();
          setTooltipPosition({ x: rect.right, y: rect.top });
          setShowTooltip(true);
        }
        showTimer = null;
      }, 300);
    }
  };
  
  const handleMouseLeave = () => {
    if (showTimer) {
      clearTimeout(showTimer);
      showTimer = null;
    }
    if (!hideTimer) {
      hideTimer = setTimeout(() => {
        setShowTooltip(false);
        setHoveredProject(null);
        hideTimer = null;
      }, 100);
    }
  };
  
  // T2.2: Context menu handlers
  const handleContextMenu = (e: MouseEvent, project: ProjectSummary) => {
    e.preventDefault();
    setContextMenuProject(project);
    setContextMenuPosition({ x: e.clientX, y: e.clientY });
  };
  
  const handleDeleteClick = () => {
    const project = contextMenuProject();
    if (project) {
      setDeleteDialogProject(project);
    }
  };
  
  const handleConfirmDelete = async () => {
    const project = deleteDialogProject();
    if (project) {
      await projectContext.removeProject(project.id);
      setDeleteDialogProject(null);
    }
  };
  
  const handleCancelDelete = () => {
    setDeleteDialogProject(null);
  };
  
  // T4.2: Keyboard navigation
  const handleKeyDown = (e: KeyboardEvent) => {
    const projects = projectContext.projects();
    if (projects.length === 0) return;
    
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setFocusedIndex((prev) => {
          const next = prev < 0 ? 0 : (prev + 1) % projects.length;
          return next;
        });
        break;
      case "ArrowUp":
        e.preventDefault();
        setFocusedIndex((prev) => {
          const next = prev < 0 ? projects.length - 1 : (prev - 1 + projects.length) % projects.length;
          return next;
        });
        break;
      case "Enter":
        e.preventDefault();
        const idx = focusedIndex();
        if (idx >= 0 && idx < projects.length) {
          handleSelectProject(projects[idx].id);
        }
        break;
    }
  };
  
  const setAvatarRef = (projectId: string) => (el: HTMLButtonElement) => {
    avatarRefs.set(projectId, el);
  };
  
  return (
    <>
      <div 
        data-component="project-rail"
        role="navigation"
        aria-label="Project list"
        class="bg-[#13161b] flex flex-col h-full shrink-0 border-r border-outline-variant/20 w-14 items-center py-2 gap-1"
        onKeyDown={handleKeyDown}
        tabIndex={0}
      >
        {/* Project list */}
        <div class="flex flex-col items-center gap-2 flex-1 overflow-y-auto py-2 custom-scrollbar">
          <For each={projectContext.projects()}>
            {(project, index) => (
              <ProjectAvatar
                project={project}
                isActive={project.id === projectContext.activeProjectId()}
                isFocused={focusedIndex() === index()}
                onClick={() => handleSelectProject(project.id)}
                onContextMenu={handleContextMenu}
                onMouseEnter={handleMouseEnter}
                onMouseLeave={handleMouseLeave}
                ref={setAvatarRef(project.id)}
              />
            )}
          </For>
        </div>
        
        {/* Add project button */}
        <button
          onClick={handleAddProject}
          class="w-10 h-10 rounded-lg flex items-center justify-center text-sm font-bold bg-surface-container-low hover:bg-surface-container-high transition-all shrink-0 text-outline hover:text-on-surface"
          title="Add Project"
        >
          <span class="material-symbols-outlined text-lg">add</span>
        </button>
      </div>
      
      {/* T2.1: Tooltip */}
      <Show when={showTooltip() && hoveredProject()}>
        <Tooltip project={hoveredProject()!} position={tooltipPosition()} />
      </Show>
      
      {/* T2.2: Context Menu */}
      <Show when={contextMenuProject()}>
        <ContextMenu
          project={contextMenuProject()!}
          position={contextMenuPosition()}
          onOpen={() => handleSelectProject(contextMenuProject()!.id)}
          onCopyPath={() => {}}
          onDelete={handleDeleteClick}
          onClose={() => setContextMenuProject(null)}
        />
      </Show>
      
      {/* T2.3: Delete Dialog */}
      <Show when={deleteDialogProject()}>
        <DeleteDialog
          projectName={deleteDialogProject()!.name}
          isOpen={true}
          onConfirm={handleConfirmDelete}
          onCancel={handleCancelDelete}
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
