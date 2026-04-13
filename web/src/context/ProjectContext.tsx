import {
  type Accessor,
  type ParentComponent,
  createContext,
  createMemo,
  createSignal,
  onMount,
  useContext,
} from "solid-js";
import { deleteProject, fetchProjects, type ProjectSummary } from "../api/projects";
import type { GlobalState } from "../stores/globalStore";

interface ProjectContextValue {
  projects: Accessor<ProjectSummary[]>;
  activeProject: Accessor<ProjectSummary | null>;
  activeProjectId: Accessor<string | null>;
  setActiveProject: (projectId: string | null) => void;
  refreshProjects: () => Promise<ProjectSummary[]>;
  removeProject: (projectId: string) => Promise<void>;
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
};

const ProjectContext = createContext<ProjectContextValue>(defaultContext);

function shouldAutoSelectFirstProject(): boolean {
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

  const refreshProjects = async (): Promise<ProjectSummary[]> => {
    const nextProjects = await fetchProjects();
    setProjects(nextProjects);
    setActiveProjectId((current) => {
      if (current && nextProjects.some((project) => project.id === current)) {
        return current;
      }

      if (shouldAutoSelectFirstProject() && nextProjects.length > 0) {
        return nextProjects[0].id;
      }

      return null;
    });
    return nextProjects;
  };

  const removeProject = async (projectId: string): Promise<void> => {
    await deleteProject(projectId);
    const currentActiveId = activeProjectId();
    setProjects((prev) => prev.filter((p) => p.id !== projectId));
    
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
          setActiveProjectId(projectId);
          // Sync to globalStore so WorkspaceProvider sees the change
          props.globalStore?.setActiveProject(projectId);
        },
        refreshProjects,
        removeProject,
      }}
    >
      {props.children}
    </ProjectContext.Provider>
  );
};

export function useProjectContext(): ProjectContextValue {
  return useContext(ProjectContext);
}
