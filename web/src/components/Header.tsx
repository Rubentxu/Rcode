import { createSignal, onMount, Show, For } from "solid-js";
import { getApiBase } from "../api/config";
import type { ModelInfo, ProviderProtocol } from "../api/types";

interface HeaderProps {
  title: string;
  sseStatus?: "connected" | "connecting" | "disconnected";
  onTerminalToggle?: () => void;
  terminalOpen?: boolean;
  currentModel?: string;
  onModelChange?: (model: string) => void;
  activeSessionId?: string;
  onSettingsClick?: () => void;
}

interface ModelGroup {
  name: string;
  hasCreds: boolean;
  models: ModelInfo[];
}

export default function Header(props: HeaderProps) {
  const [models, setModels] = createSignal<ModelInfo[]>([]);
  const [showModelDropdown, setShowModelDropdown] = createSignal(false);
  const [currentModel, setCurrentModel] = createSignal(props.currentModel || "Loading...");

  onMount(async () => {
    try {
      const res = await fetch(`${await getApiBase()}/models`);
      if (res.ok) {
        const data = await res.json();
        const availableModels: ModelInfo[] = data.models || [];
        setModels(availableModels);
        if (availableModels.length > 0 && !props.currentModel) {
          const preferredModel =
            // 1. Backend-configured model (highest priority)
            availableModels.find((model) => model.enabled && model.source === "configured")
            // 2. First enabled model
            ?? availableModels.find((model) => model.enabled)
            ?? availableModels[0];

          console.info("Selected default model", {
            selected: preferredModel.id,
            enabled: preferredModel.enabled,
            source: preferredModel.source,
          });

          setCurrentModel(preferredModel.id);
          props.onModelChange?.(preferredModel.id);
        }
      }
    } catch {
      setCurrentModel("Error loading models");
    }
  });

  // Update local state when prop changes
  if (props.currentModel && props.currentModel !== currentModel()) {
    setCurrentModel(props.currentModel);
  }

  // Group models by provider
  const grouped = (): ModelGroup[] => {
    const groups: Record<string, ModelGroup> = {};
    for (const m of models()) {
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

  const protocolBadgeLabel = (protocol?: ProviderProtocol) => {
    switch (protocol) {
      case "openai_compat":
        return "OpenAI compat";
      case "anthropic_compat":
        return "Anthropic compat";
      case "google":
        return "Google";
      default:
        return null;
    }
  };

  const protocolBadgeStyle = (protocol?: ProviderProtocol) => {
    switch (protocol) {
      case "openai_compat":
        return "background: rgba(99,102,241,0.14); color: #818cf8;";
      case "anthropic_compat":
        return "background: rgba(245,158,11,0.14); color: #fbbf24;";
      case "google":
        return "background: rgba(34,197,94,0.14); color: var(--secondary);";
      default:
        return null;
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

  return (
    <header class="flex justify-between items-center w-full px-6 py-3 h-14 bg-[#181c22]">
      {/* Left section - Session info */}
      <div class="flex items-center gap-6">
        <div class="flex items-center gap-2">
          <span class="text-xs font-medium text-outline">Session:</span>
          <span class="text-sm font-bold text-on-surface">{props.title || "RCode"}</span>
        </div>
        <div class="h-4 w-[1px] bg-outline-variant/30"></div>
        <div class="flex items-center gap-2">
          <span class="text-xs font-medium text-outline">Model:</span>
          <span class="text-sm font-semibold text-primary">
            {currentModelMeta()?.display_name || currentModel().split('/')[1] || currentModel()}
          </span>
        </div>
      </div>

      {/* Right section - Status and controls */}
      <div class="flex items-center gap-4">
        {/* Connected status badge */}
        <div class="flex items-center gap-2 bg-surface-container-lowest px-3 py-1.5 rounded-full border border-outline-variant/10">
          <span class="relative flex h-2 w-2">
            <Show when={props.sseStatus === "connected"}>
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-secondary opacity-75"></span>
            </Show>
            <span
              class={`relative inline-flex rounded-full h-2 w-2 ${
                props.sseStatus === "connected" ? "bg-secondary" :
                props.sseStatus === "connecting" ? "bg-tertiary" : "bg-outline"
              }`}
            ></span>
          </span>
          <span class="text-[10px] font-bold text-secondary tracking-widest uppercase">
            {props.sseStatus || "disconnected"}
          </span>
        </div>

        {/* Action buttons */}
        <div class="flex items-center gap-1">
          <button
            onClick={props.onTerminalToggle}
            class="p-2 hover:bg-surface-container-high rounded-md transition-all active:scale-95 duration-200"
            aria-label="Toggle terminal"
          >
            <span class="material-symbols-outlined text-[20px] text-primary">terminal</span>
          </button>
          <button
            onClick={props.onSettingsClick}
            class="p-2 hover:bg-surface-container-high rounded-md transition-all active:scale-95 duration-200"
            aria-label="Open settings"
          >
            <span class="material-symbols-outlined text-[20px] text-primary">settings</span>
          </button>
        </div>

        {/* Model selector dropdown */}
        <div class="relative">
          <button
            role="combobox"
            aria-expanded={showModelDropdown()}
            aria-haspopup="listbox"
            aria-label="Select model"
            aria-controls="model-listbox"
            onClick={() => setShowModelDropdown(!showModelDropdown())}
            class="flex items-center gap-2 bg-surface-container-lowest px-3 py-1.5 rounded-full border border-outline-variant/10 hover:bg-surface-container-low transition-all cursor-pointer"
          >
            <span class="text-xs font-semibold text-primary">
              {currentModelMeta()?.display_name || currentModel().split('/')[1] || currentModel()}
            </span>
            <Show when={currentModelMeta()}>
              {(model) => (
                <span
                  class={`text-[10px] px-1.5 py-0.5 rounded font-bold uppercase`}
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
              id="model-listbox"
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
                          role="option"
                          aria-selected={model.id === currentModel()}
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
                              <Show when={model.is_compatible && protocolBadgeLabel(model.protocol)}>
                                <span
                                  class="text-[10px] px-1.5 py-0.5 rounded font-medium"
                                  style={protocolBadgeStyle(model.protocol) || ""}
                                >
                                  {protocolBadgeLabel(model.protocol)}
                                </span>
                              </Show>
                              <span
                                class={`text-[10px] px-2 py-0.5 rounded uppercase`}
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
      </div>
    </header>
  );
}
