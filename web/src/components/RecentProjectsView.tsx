import { For, Show } from "solid-js";
import type { ProjectSummary } from "../api/projects";

interface RecentProjectsViewProps {
  projects: ProjectSummary[];
  activeProject: ProjectSummary | null;
  onSelectProject: (id: string) => void;
  onAddProject: () => void;
}

function shortenPath(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  if (parts.length <= 3) return path;
  return `…/${parts.slice(-2).join("/")}`;
}

export default function RecentProjectsView(props: RecentProjectsViewProps) {
  // Sort by updated_at desc, take up to 5
  const recentProjects = () =>
    [...props.projects]
      .sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime())
      .slice(0, 5);

  return (
    <div
      data-component="recent-projects-view"
      class="flex flex-col items-center justify-center h-full px-6 py-12 gap-8"
    >
      {/* Header */}
      <div class="flex flex-col items-center gap-2">
        <div
          class="w-12 h-12 rounded-xl flex items-center justify-center"
          style="background: radial-gradient(circle at 30% 30%, var(--accent-bg-hover) 0%, var(--accent-bg-subtle) 100%); border: 1px solid var(--accent-border-subtle);"
        >
          <span
            class="material-symbols-outlined text-xl"
            style="color: var(--accent); font-variation-settings: 'FILL' 1;"
          >
            code
          </span>
        </div>
        <h1
          class="text-xl font-bold"
          style="background: var(--brand-gradient); -webkit-background-clip: text; -webkit-text-fill-color: transparent;"
        >
          RCode
        </h1>
      </div>

      {/* Active project CTA */}
      <Show when={props.activeProject}>
        <div class="w-full max-w-sm">
          <button
            onClick={() => props.onSelectProject(props.activeProject!.id)}
            class="w-full flex items-center gap-3 px-4 py-3 rounded-xl text-left transition-all duration-200 hover:scale-[1.01]"
            style={{
              background: "var(--surface-container)",
              border: "1px solid var(--outline-variant)",
              "box-shadow": "var(--shadow-sm)",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLElement).style.borderColor = "var(--primary)";
              (e.currentTarget as HTMLElement).style.background = "var(--surface-container-high)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.borderColor = "var(--outline-variant)";
              (e.currentTarget as HTMLElement).style.background = "var(--surface-container)";
            }}
          >
            <span
              class="material-symbols-outlined text-lg"
              style="color: var(--primary);"
            >
              play_arrow
            </span>
            <div class="flex-1 min-w-0">
              <p class="text-sm font-semibold text-on-surface truncate">
                Continue in {props.activeProject!.name}
              </p>
              <p class="text-xs text-outline truncate">
                {shortenPath(props.activeProject!.canonical_path)}
              </p>
            </div>
            <span class="material-symbols-outlined text-base text-outline">chevron_right</span>
          </button>
        </div>
      </Show>

      {/* Recent projects list */}
      <div class="w-full max-w-sm">
        <p class="text-xs font-semibold text-outline uppercase tracking-widest mb-3 text-center">
          Recent Projects
        </p>
        <div class="flex flex-col gap-2">
          <For each={recentProjects()}>
            {(project) => (
              <div
                class="flex items-center gap-3 px-4 py-2.5 rounded-lg"
                style={{
                  background: "var(--surface-container-low)",
                  border: "1px solid var(--outline-variant)",
                }}
              >
                <div class="flex-1 min-w-0">
                  <p class="text-sm font-medium text-on-surface truncate">{project.name}</p>
                  <p class="text-xs text-outline truncate">
                    {shortenPath(project.canonical_path)} · {project.session_count} session{project.session_count !== 1 ? "s" : ""}
                  </p>
                </div>
                <button
                  onClick={() => props.onSelectProject(project.id)}
                  class="px-3 py-1.5 text-xs font-medium rounded-md transition-colors"
                  style={{
                    background: "var(--surface-container-high)",
                    color: "var(--on-surface-variant)",
                    border: "1px solid var(--outline-variant)",
                  }}
                  onMouseEnter={(e) => {
                    (e.currentTarget as HTMLElement).style.borderColor = "var(--primary)";
                    (e.currentTarget as HTMLElement).style.color = "var(--primary)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLElement).style.borderColor = "var(--outline-variant)";
                    (e.currentTarget as HTMLElement).style.color = "var(--on-surface-variant)";
                  }}
                >
                  Open
                </button>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* Secondary CTA */}
      <button
        onClick={props.onAddProject}
        class="flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-lg transition-all duration-200"
        style={{
          background: "transparent",
          color: "var(--outline)",
          border: "1px dashed var(--outline-variant)",
        }}
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
        Open another folder
      </button>
    </div>
  );
}
