import {
  type Accessor,
  type ParentComponent,
  createContext,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  useContext,
} from "solid-js";
import { deleteProject, fetchProjectHealth, fetchProjects, triggerHealthRefresh, updateProject as updateProjectApi, type ProjectHealth, type ProjectSummary, type UpdateProjectPatch } from "../api/projects";
import type { GlobalState } from "../stores/globalStore";

const MRU_KEY = "rcode:active-project";

function readMruProjectId(): string | null {
  try {
    return localStorage.getItem(MRU_KEY);
  } catch {
    return null;
  }
}

function writeMruProjectId(id: string): void {
  try {
    localStorage.setItem(MRU_KEY, id);
  } catch { /* ignore */ }
}

interface ProjectContextValue {
  projects: Accessor<ProjectSummary[]>;
  activeProject: Accessor<ProjectSummary | null>;
  activeProjectId: Accessor<string | null>;
  setActiveProject: (projectId: string | null) => void;
  refreshProjects: () => Promise<ProjectSummary[]>;
  removeProject: (projectId: string) => Promise<void>;
  updateProject: (projectId: string, patch: UpdateProjectPatch) => Promise<void>;
  health: Accessor<ProjectHealth | null>;
  getHealth: (projectId: string) => ProjectHealth | undefined;
}

interface ProjectProviderProps {
  initialProjects?: ProjectSummary[];
  initialActiveProjectId?: string | null;
  skipAutoLoad?: boolean;
  globalStore?: GlobalState;
}

const defaultContext: ProjectContextValue = {
  projects: () => [],
  activeProject: () => null,
  activeProjectId: () => null,
  setActiveProject: () => undefined,
  refreshProjects: async () => [],
  removeProject: async () => undefined,
  updateProject: async () => undefined,
  health: () => null,
  getHealth: () => undefined,
};

const ProjectContext = createContext<ProjectContextValue>(defaultContext);

// Auto-selection only applies in Tauri desktop context
function isTauriContext(): boolean {
  return typeof window !== "undefined" && Boolean(window.__TAURI__);
}

export const ProjectProvider: ParentComponent<ProjectProviderProps> = (props) => {
  const [projects, setProjects] = createSignal<ProjectSummary[]>(props.initialProjects ?? []);
  const [activeProjectId, setActiveProjectId] = createSignal<string | null>(
    props.initialActiveProjectId ?? null,
  );

  const activeProject = createMemo(() => {
    const projectId = activeProjectId();
    if (!projectId) {
      return null;
    }

    return projects().find((project) => project.id === projectId) ?? null;
  });

  // ── Health state ──────────────────────────────────────────────────────────
  const [health, setHealth] = createSignal<ProjectHealth | null>(null);
  const [healthMap, setHealthMap] = createSignal<Record<string, ProjectHealth>>({});

  const getHealth = (projectId: string): ProjectHealth | undefined => {
    return healthMap()[projectId];
  };

  let pollHandle: ReturnType<typeof setInterval> | null = null;

  const clearPoll = () => {
    if (pollHandle !== null) {
      clearInterval(pollHandle);
      pollHandle = null;
    }
  };

  const fetchAndSetHealth = async (projectId: string) => {
    try {
      const h = await fetchProjectHealth(projectId);
      setHealth(h);
      setHealthMap((prev) => ({ ...prev, [projectId]: h }));
      return h;
    } catch {
      return null;
    }
  };

  const startSlowPoll = (projectId: string) => {
    clearPoll();
    pollHandle = setInterval(async () => {
      const h = await fetchAndSetHealth(projectId);
      // If it goes back to checking, switch to fast poll
      if (h?.status === "checking") {
        clearPoll();
        startFastPoll(projectId);
      }
    }, 30_000);
  };

  const startFastPoll = (projectId: string) => {
    clearPoll();
    pollHandle = setInterval(async () => {
      const h = await fetchAndSetHealth(projectId);
      if (h && h.status !== "checking") {
        clearPoll();
        startSlowPoll(projectId);
      }
    }, 5_000);
  };

  // Trigger health check and polling when active project changes
  createEffect(() => {
    const projectId = activeProjectId();
    if (!projectId) {
      setHealth(null);
      clearPoll();
      return;
    }

    void fetchAndSetHealth(projectId);
    void triggerHealthRefresh(projectId).catch(() => {/* ignore */});
    startFastPoll(projectId);
  });

  onCleanup(() => clearPoll());

  // ── CRUD ──────────────────────────────────────────────────────────────────

  const refreshProjects = async (): Promise<ProjectSummary[]> => {
    const nextProjects = await fetchProjects();
    setProjects(nextProjects);
    setActiveProjectId((current) => {
      // 1. If current activeProjectId is still valid → keep it
      if (current && nextProjects.some((project) => project.id === current)) {
        return current;
      }

      // 2. If single project → auto-select it
      if (nextProjects.length === 1) {
        const singleId = nextProjects[0].id;
        writeMruProjectId(singleId);
        return singleId;
      }

      // 3. If MRU is valid in next projects → restore MRU
      const mruId = readMruProjectId();
      if (mruId && nextProjects.some((project) => project.id === mruId)) {
        writeMruProjectId(mruId);
        return mruId;
      }

      // 4. Otherwise → null (show onboarding)
      return null;
    });
    return nextProjects;
  };

  const removeProject = async (projectId: string): Promise<void> => {
    await deleteProject(projectId);
    const currentActiveId = activeProjectId();
    setProjects((prev) => prev.filter((p) => p.id !== projectId));
    setHealthMap((prev) => {
      const next = { ...prev };
      delete next[projectId];
      return next;
    });

    // If deleting the active project, auto-select first remaining or null
    if (currentActiveId === projectId) {
      const remaining = projects().filter((p) => p.id !== projectId);
      if (remaining.length > 0) {
        setActiveProjectId(remaining[0].id);
      } else {
        setActiveProjectId(null);
      }
    }
  };

  const updateProject = async (projectId: string, patch: UpdateProjectPatch): Promise<void> => {
    await updateProjectApi(projectId, patch);
    await refreshProjects();
  };

  onMount(() => {
    if (!props.skipAutoLoad) {
      void refreshProjects();
    }
  });

  return (
    <ProjectContext.Provider
      value={{
        projects,
        activeProject,
        activeProjectId,
        setActiveProject: (projectId) => {
          if (projectId) {
            writeMruProjectId(projectId);
          }
          setActiveProjectId(projectId);
          // Sync to globalStore so WorkspaceProvider sees the change
          props.globalStore?.setActiveProject(projectId);
        },
        refreshProjects,
        removeProject,
        updateProject,
        health,
        getHealth,
      }}
    >
      {props.children}
    </ProjectContext.Provider>
  );
};

export function useProjectContext(): ProjectContextValue {
  return useContext(ProjectContext);
}
