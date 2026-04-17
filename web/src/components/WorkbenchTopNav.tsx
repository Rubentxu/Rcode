import { createSignal, Show, For, onMount } from "solid-js";
import type { Session } from "../stores";
import type { ModelInfo } from "../api/types";
import { getApiBase } from "../api/config";
import { useProjectContext } from "../context/ProjectContext";
import { useProviderModels } from "../hooks/useProviderModels";

// ── Action button with tooltip ───────────────────────────────────────────────
function ActionButton(props: {
  onClick?: () => void;
  icon: string;
  label: string;
  shortcut?: string;
  active?: boolean;
  "data-component"?: string;
}) {
  const [show, setShow] = createSignal(false);
  return (
    <div class="relative" onMouseEnter={() => setShow(true)} onMouseLeave={() => setShow(false)}>
      <button
        onClick={props.onClick}
        data-component={props["data-component"]}
        class={`p-2 hover:bg-surface-container-high rounded-md transition-all active:scale-95 ${props.active ? "bg-surface-container-high" : ""}`}
        aria-label={props.label}
      >
        <span class="material-symbols-outlined text-[18px] text-primary">{props.icon}</span>
      </button>
      <Show when={show()}>
        <div class="absolute bottom-full right-0 mb-1.5 pointer-events-none z-50">
          <div class="flex items-center gap-1.5 px-2 py-1 rounded-lg text-[11px] font-medium whitespace-nowrap shadow-lg"
            style="background: var(--surface-container-highest); border: 1px solid var(--outline-variant); color: var(--on-surface);">
            <span>{props.label}</span>
            <Show when={props.shortcut}>
              <kbd class="px-1 py-0.5 rounded text-[10px] font-mono"
                style="background: var(--surface-container); border: 1px solid var(--outline-variant); color: var(--outline);">
                {props.shortcut}
              </kbd>
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
}

// ── Theme toggle (persisted in localStorage) ──────────────────────────────────
function getInitialTheme(): "dark" | "light" {
  try {
    const saved = localStorage.getItem("rcode-theme");
    if (saved === "light" || saved === "dark") return saved;
  } catch {}
  return "dark";
}

const [currentTheme, setCurrentTheme] = createSignal<"dark" | "light">(getInitialTheme());

function applyTheme(theme: "dark" | "light") {
  document.documentElement.setAttribute("data-theme", theme);
  try { localStorage.setItem("rcode-theme", theme); } catch {}
}

function toggleTheme() {
  const next = currentTheme() === "dark" ? "light" : "dark";
  setCurrentTheme(next);
  applyTheme(next);
}

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
  activeProjectName?: string;
}

interface ModelGroup {
  name: string;
  hasCreds: boolean;
  models: ModelInfo[];
}

export default function WorkbenchTopNav(props: WorkbenchTopNavProps) {
  const projectContext = useProjectContext();
  const pm = useProviderModels();
  const [showModelDropdown, setShowModelDropdown] = createSignal(false);
  const [currentModel, setCurrentModel] = createSignal(props.currentModel || "");
  const [searchExpanded, setSearchExpanded] = createSignal(false);

  // Apply persisted theme on first mount
  onMount(() => applyTheme(currentTheme()));

  // Group models by provider display_name, only for configured providers, only enabled models
  const grouped = (): ModelGroup[] => {
    const groups: Record<string, ModelGroup> = {};
    for (const m of pm.allModels) {
      // Only include enabled models in the dropdown selector (per REQ-WB-01)
      if (!m.enabled) continue;
      const provider = pm.modelsByProvider.get(m.provider);
      // Only show models from configured providers
      if (!provider || !provider.configured && !provider.has_key) continue;
      const providerName = provider.display_name || m.provider;
      if (!groups[providerName]) {
        groups[providerName] = { name: providerName, hasCreds: provider.has_key, models: [] };
      }
      groups[providerName].models.push(m);
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

  const currentModelMeta = () => pm.allModels.find((model) => model.id === currentModel());

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
        return "background: var(--success-bg-subtle); color: var(--secondary);";
      case "api":
        return "background: var(--info-bg-subtle); color: var(--info-color);";
      default:
        return "background: var(--surface-container); color: var(--on-surface-variant);";
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
      class="flex justify-between items-center w-full px-4 h-12 bg-surface-container-low border-b border-outline-variant/20 shrink-0"
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
          <Show when={props.activeProjectName}>
            <>
              <span class="material-symbols-outlined text-[14px] text-outline" style={{"font-variation-settings": "'FILL' 0"}}>folder</span>
              <span class="text-xs font-semibold text-on-surface-variant truncate max-w-[100px] sm:max-w-[140px]">{props.activeProjectName}</span>
              <span class="text-outline-variant/40 text-xs select-none">/</span>
            </>
          </Show>
          <span class="text-sm font-semibold text-on-surface truncate max-w-[120px] sm:max-w-[200px] md:max-w-[300px]" title={props.title || "RCode"}>{props.title || "RCode"}</span>
        </div>
      </div>

      {/* Center section - Search */}
      <div class="flex-1 mx-2 md:mx-4">
        {/* Full search bar on md+ */}
        <div class="hidden md:block max-w-md mx-auto">
          <div class="relative">
            <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-outline text-sm">search</span>
            <input
              type="text"
              placeholder="Search..."
              aria-label="Search sessions and files"
              data-component="workbench-search"
              class="w-full bg-surface-container-low text-on-surface text-sm pl-9 pr-4 py-2 rounded-lg border border-outline-variant/20 focus:border-primary focus:outline-none transition-colors"
            />
          </div>
        </div>
        {/* Compact search toggle on small screens */}
        <div class="md:hidden flex justify-center">
          <Show when={!searchExpanded()}>
            <button
              onClick={() => setSearchExpanded(true)}
              aria-label="Open search"
              class="p-2 hover:bg-surface-container-high rounded-md transition-all"
            >
              <span class="material-symbols-outlined text-[18px] text-outline">search</span>
            </button>
          </Show>
          <Show when={searchExpanded()}>
            <div class="relative w-full max-w-[200px]">
              <span class="material-symbols-outlined absolute left-2 top-1/2 -translate-y-1/2 text-outline text-sm">search</span>
              <input
                type="text"
                placeholder="Search..."
                aria-label="Search sessions and files"
                autofocus
                onBlur={() => setSearchExpanded(false)}
                class="w-full bg-surface-container-low text-on-surface text-xs pl-7 pr-3 py-1.5 rounded-lg border border-primary focus:outline-none transition-colors"
              />
            </div>
          </Show>
        </div>
      </div>

      {/* Right section - Status and controls */}
      <div class="flex items-center gap-3">
        {/* Connected status badge */}
        <div
          class="flex items-center gap-2 px-2.5 py-1 rounded-full border transition-all"
          style={{
            background: props.sseStatus === "connected"
              ? "var(--success-bg-subtle)"
              : props.sseStatus === "connecting"
              ? "var(--warning-bg-subtle)"
              : "transparent",
            "border-color": props.sseStatus === "connected"
              ? "var(--success-border-subtle)"
              : props.sseStatus === "connecting"
              ? "var(--outline-variant)"
              : "var(--outline-variant)",
            opacity: props.sseStatus === "disconnected" || !props.sseStatus ? "0.6" : "1",
          }}
        >
          <span class="relative flex h-2 w-2">
            <span class={`inline-flex rounded-full h-2 w-2 ${
              props.sseStatus === "connected" ? "bg-secondary" :
              props.sseStatus === "connecting" ? "bg-tertiary" : "bg-outline/50"
            }`}></span>
            <Show when={props.sseStatus === "connected"}>
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-secondary opacity-75"></span>
            </Show>
          </span>
          <span class={`text-[10px] font-semibold tracking-wide uppercase hidden sm:inline ${
            props.sseStatus === "connected" ? "text-secondary" :
            props.sseStatus === "connecting" ? "text-tertiary" : "text-outline"
          }`}>
            {props.sseStatus || "disconnected"}
          </span>
        </div>

        {/* Model selector dropdown */}
        <div class="relative hidden sm:block">
          <button
            onClick={() => setShowModelDropdown(!showModelDropdown())}
            class="flex items-center gap-1.5 bg-surface-container-low px-2.5 py-1 rounded-full border border-outline-variant/10 hover:bg-surface-container-high transition-all cursor-pointer"
          >
            <span class="text-[10px] font-semibold text-primary truncate max-w-[80px] lg:max-w-[120px]">
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
              role="listbox"
              aria-label="Available models"
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

        {/* Fallback model badge — only on xs screens */}
        <div class="sm:hidden flex items-center gap-1.5 bg-surface-container-low px-2.5 py-1 rounded-full border border-outline-variant/10">
          <span class="text-[10px] font-semibold text-primary truncate max-w-[80px]">
            {props.currentModel?.split('/')[1] || props.currentModel || "No model"}
          </span>
        </div>

        {/* Action buttons */}
        <div class="flex items-center gap-1">
          <ActionButton
            onClick={toggleTheme}
            icon={currentTheme() === "dark" ? "light_mode" : "dark_mode"}
            label={currentTheme() === "dark" ? "Light mode" : "Dark mode"}
            data-component="theme-toggle"
          />
          <ActionButton
            onClick={props.onOutlineToggle}
            icon={props.outlineOpen ? "right_panel_close" : "right_panel_open"}
            label="Toggle outline"
            shortcut="Ctrl+\\"
            active={props.outlineOpen}
            data-component="outline-toggle"
          />
          <ActionButton
            onClick={props.onTerminalToggle}
            icon="terminal"
            label="Toggle terminal"
            shortcut="Ctrl+`"
            active={props.terminalOpen}
            data-component="terminal-toggle"
          />
          <ActionButton
            onClick={props.onSettingsClick}
            icon="settings"
            label="Settings"
            shortcut="Ctrl+,"
            data-component="settings-toggle"
          />
        </div>
      </div>
    </header>
  );
}
