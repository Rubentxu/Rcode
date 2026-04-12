import { createSignal, onMount, Show, For } from "solid-js";
import type { Session } from "../App";
import { getApiBase } from "../api/config";
import { useProjectContext } from "../context/ProjectContext";

interface WorkbenchTopNavProps {
  title: string;
  sseStatus?: "connected" | "connecting" | "disconnected";
  currentModel?: string;
  onModelChange?: (model: string) => void;
  activeSessionId?: string;
  onTerminalToggle?: () => void;
  terminalOpen?: boolean;
  onSettingsClick?: () => void;
  onOutlineToggle?: () => void;
  outlineOpen?: boolean;
}

interface ModelInfo {
  id: string;
  provider: string;
  display_name?: string;
  has_credentials: boolean;
  source: "api" | "fallback" | "configured";
  enabled: boolean;
}

interface ModelGroup {
  name: string;
  hasCreds: boolean;
  models: ModelInfo[];
}

export default function WorkbenchTopNav(props: WorkbenchTopNavProps) {
  const projectContext = useProjectContext();
  const [models, setModels] = createSignal<ModelInfo[]>([]);
  const [showModelDropdown, setShowModelDropdown] = createSignal(false);
  const [currentModel, setCurrentModel] = createSignal(props.currentModel || "");

  onMount(async () => {
    try {
      const res = await fetch(`${await getApiBase()}/models`);
      if (res.ok) {
        const data = await res.json();
        setModels(data.models || []);
      }
    } catch {
      // Silently fail - selector just shows current model
    }
  });

  // Group models by provider, only including enabled models in the dropdown
  const grouped = (): ModelGroup[] => {
    const groups: Record<string, ModelGroup> = {};
    for (const m of models()) {
      // Only include enabled models in the dropdown selector (per REQ-WB-01)
      if (!m.enabled) continue;
      if (!groups[m.provider]) {
        groups[m.provider] = { name: m.provider, hasCreds: m.has_credentials, models: [] };
      }
      groups[m.provider].models.push(m);
    }
    return Object.values(groups).map((group) => ({
      ...group,
      models: [...group.models].sort((a, b) => {
        const rank = (model: ModelInfo) => {
          if (model.source === "configured") return 0;
          if (model.enabled) return 1;
          if (model.source === "api") return 2;
          return 3;
        };
        return rank(a) - rank(b) || a.id.localeCompare(b.id);
      }),
    }));
  };

  const currentModelMeta = () => models().find((model) => model.id === currentModel());

  const sourceBadgeLabel = (source: ModelInfo["source"]) => {
    switch (source) {
      case "configured":
        return "configured";
      case "api":
        return "live";
      default:
        return "fallback";
    }
  };

  const sourceBadgeStyle = (source: ModelInfo["source"]) => {
    switch (source) {
      case "configured":
        return "background: rgba(34,197,94,0.14); color: var(--secondary);";
      case "api":
        return "background: rgba(59,130,246,0.14); color: #60a5fa;";
      default:
        return "background: rgba(148,163,184,0.14); color: var(--on-surface-variant);";
    }
  };

  const handleModelSelect = async (modelId: string) => {
    setCurrentModel(modelId);
    setShowModelDropdown(false);
    props.onModelChange?.(modelId);

    // If a session is active, switch its model via /connect
    if (props.activeSessionId) {
      try {
        await fetch(`${await getApiBase()}/connect`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ session_id: props.activeSessionId, model_id: modelId }),
        });
      } catch (err) {
        console.error("Failed to switch model:", err);
      }
    }
  };

  // Update local state when prop changes
  if (props.currentModel && props.currentModel !== currentModel()) {
    setCurrentModel(props.currentModel);
  }

  return (
    <header
      data-component="workbench-topnav"
      class="flex justify-between items-center w-full px-4 h-12 bg-[#181c22] border-b border-outline-variant/20 shrink-0"
    >
      {/* Left section - Brand + session title */}
      <div class="flex items-center gap-4 min-w-0">
        <div class="flex items-center gap-2 shrink-0">
          <div class="w-7 h-7 rounded-lg bg-primary-container flex items-center justify-center">
            <span class="material-symbols-outlined text-on-primary-container text-sm" style="font-variation-settings: 'FILL' 1;">account_tree</span>
          </div>
          <span class="text-sm font-black text-white tracking-tight hidden sm:block">RCode</span>
        </div>

        <div class="h-5 w-[1px] bg-outline-variant/30 hidden sm:block"></div>

        <div class="flex items-center gap-2 min-w-0">
          <span class="text-xs font-medium text-outline shrink-0">Session:</span>
          <span class="text-sm font-semibold text-on-surface truncate">{props.title || "No session"}</span>
          <Show when={projectContext.activeProject()}>
            <div class="flex items-center gap-1.5 px-2 py-0.5 bg-surface-container-low rounded-full border border-outline-variant/10">
              <span class="material-symbols-outlined text-xs text-secondary shrink-0">folder</span>
              <span class="text-xs font-medium text-secondary truncate max-w-[150px]" title={projectContext.activeProject()?.name}>
                {projectContext.activeProject()?.name}
              </span>
            </div>
          </Show>
        </div>
      </div>

      {/* Center section - Search */}
      <div class="flex-1 max-w-md mx-4 hidden md:block">
        <div class="relative">
          <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-outline text-sm">search</span>
          <input
            type="text"
            placeholder="Search..."
            data-component="workbench-search"
            class="w-full bg-surface-container-low text-on-surface text-sm pl-9 pr-4 py-2 rounded-lg border border-outline-variant/20 focus:border-primary focus:outline-none transition-colors"
          />
        </div>
      </div>

      {/* Right section - Status and controls */}
      <div class="flex items-center gap-3">
        {/* Connected status badge */}
        <div class="flex items-center gap-2 bg-surface-container-low px-2.5 py-1 rounded-full border border-outline-variant/10">
          <span class="relative flex h-2 w-2">
            <span
              class={`inline-flex rounded-full h-2 w-2 ${
                props.sseStatus === "connected" ? "bg-secondary" :
                props.sseStatus === "connecting" ? "bg-tertiary" : "bg-outline"
              }`}
            ></span>
            <Show when={props.sseStatus === "connected"}>
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-secondary opacity-75"></span>
            </Show>
          </span>
          <span class="text-[10px] font-bold text-secondary tracking-widest uppercase hidden sm:inline">
            {props.sseStatus || "disconnected"}
          </span>
        </div>

        {/* Model selector dropdown */}
        <div class="relative hidden lg:block">
          <button
            onClick={() => setShowModelDropdown(!showModelDropdown())}
            class="flex items-center gap-1.5 bg-surface-container-low px-2.5 py-1 rounded-full border border-outline-variant/10 hover:bg-surface-container-high transition-all cursor-pointer"
          >
            <span class="text-[10px] font-semibold text-primary truncate max-w-[120px]">
              {currentModelMeta()?.display_name || currentModel().split('/')[1] || currentModel() || "No model"}
            </span>
            <Show when={currentModelMeta()}>
              {(model) => (
                <span
                  class="text-[10px] px-1 py-0.5 rounded font-bold uppercase"
                  style={sourceBadgeStyle(model().source)}
                >
                  {sourceBadgeLabel(model().source)}
                </span>
              )}
            </Show>
            <span class="text-[10px] text-outline">▾</span>
          </button>

          <Show when={showModelDropdown()}>
            <div
              class="absolute top-full right-0 mt-2 bg-surface-container border border-outline-variant/20 rounded-xl min-w-[320px] z-[100] max-h-[450px] overflow-y-auto shadow-2xl"
            >
              <For each={grouped()}>
                {(group) => (
                  <div class="mb-1">
                    <div
                      class="flex items-center gap-2 px-4 py-3 bg-surface-container-high border-b border-outline-variant/10"
                    >
                      <span class="text-xs font-black text-outline uppercase tracking-widest capitalize">
                        {group.name}
                      </span>
                      <span
                        class={`text-[10px] px-2 py-0.5 rounded font-bold ${
                          group.hasCreds
                            ? "bg-secondary-container/20 text-secondary"
                            : "bg-error-container/20 text-error"
                        }`}
                      >
                        {group.hasCreds ? "configured" : "no key"}
                      </span>
                    </div>
                    <For each={group.models}>
                      {(model) => (
                        <button
                          class={`w-full text-left px-4 py-3 border-b border-outline-variant/5 hover:bg-surface-container-high/50 transition-all ${
                            model.id === currentModel() ? "bg-primary-container/10" : ""
                          }`}
                          onClick={() => handleModelSelect(model.id)}
                        >
                          <div class="flex justify-between items-center">
                            <div class="flex items-center gap-2 min-w-0">
                              <span class="text-sm font-medium text-on-surface truncate">
                                {model.display_name || model.id.split('/')[1] || model.id}
                              </span>
                              <span
                                class="text-[10px] px-2 py-0.5 rounded uppercase"
                                style={sourceBadgeStyle(model.source)}
                              >
                                {sourceBadgeLabel(model.source)}
                              </span>
                            </div>
                            <div class="flex items-center gap-2">
                              <Show when={model.id === currentModel()}>
                                <span class="text-[10px] text-primary font-semibold">selected</span>
                              </Show>
                              <Show when={!model.enabled}>
                                <span class="text-[10px] text-outline opacity-50">disabled</span>
                              </Show>
                            </div>
                          </div>
                        </button>
                      )}
                    </For>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>

        {/* Fallback model badge for smaller screens */}
        <div class="lg:hidden flex items-center gap-1.5 bg-surface-container-low px-2.5 py-1 rounded-full border border-outline-variant/10">
          <span class="text-[10px] font-semibold text-primary truncate max-w-[120px]">
            {props.currentModel?.split('/')[1] || props.currentModel || "No model"}
          </span>
        </div>

        {/* Action buttons */}
        <div class="flex items-center gap-1">
          <button
            onClick={props.onOutlineToggle}
            class="p-2 hover:bg-surface-container-high rounded-md transition-all active:scale-95"
            title="Toggle Outline Panel"
            data-component="outline-toggle"
          >
            <span class="material-symbols-outlined text-[18px] text-primary">
              {props.outlineOpen ? "panel_right_close" : "panel_right"}
            </span>
          </button>

          <button
            onClick={props.onTerminalToggle}
            class="p-2 hover:bg-surface-container-high rounded-md transition-all active:scale-95"
            title="Toggle Terminal"
            data-component="terminal-toggle"
          >
            <span class="material-symbols-outlined text-[18px] text-primary">terminal</span>
          </button>

          <button
            onClick={props.onSettingsClick}
            class="p-2 hover:bg-surface-container-high rounded-md transition-all active:scale-95"
            title="Settings"
            data-component="settings-toggle"
          >
            <span class="material-symbols-outlined text-[18px] text-primary">settings</span>
          </button>
        </div>
      </div>
    </header>
  );
}
