import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { getApiBase } from "../api/config";

interface Provider {
  id: string;
  name: string;
  has_key: boolean;
  base_url: string | null;
  enabled: boolean;
  key_source?: string;
}

interface ModelInfo {
  id: string;
  provider: string;
  display_name?: string;
  has_credentials: boolean;
  source: "api" | "fallback" | "configured";
  enabled: boolean;
}

type SettingsSection = "general" | "shortcuts" | "providers" | "models";

const SETTINGS_NAV: Array<{ id: SettingsSection; title: string; group: string; icon: string }> = [
  { id: "general", title: "General", group: "Desktop", icon: "⚙" },
  { id: "shortcuts", title: "Shortcuts", group: "Desktop", icon: "⌨" },
  { id: "providers", title: "Providers", group: "Server", icon: "◉" },
  { id: "models", title: "Models", group: "Server", icon: "✦" },
];

const POPULAR_PROVIDER_IDS = ["opencode-zen", "opencode-go", "anthropic", "openai", "google", "openrouter"];

const PROVIDER_URLS: Record<string, Array<{ label: string; url: string }>> = {
  anthropic: [
    { label: "Anthropic (official)", url: "https://api.anthropic.com" },
    { label: "MiniMax Anthropic-compatible", url: "https://api.minimax.io/anthropic" },
    { label: "Custom / proxy", url: "__custom__" },
  ],
  openai: [
    { label: "OpenAI (official)", url: "https://api.openai.com" },
    { label: "Azure OpenAI", url: "https://<resource>.openai.azure.com" },
    { label: "LM Studio (local)", url: "http://localhost:1234" },
    { label: "Ollama (local)", url: "http://localhost:11434" },
    { label: "Custom / proxy", url: "__custom__" },
  ],
  google: [
    { label: "Google AI (official)", url: "https://generativelanguage.googleapis.com" },
    { label: "Custom / proxy", url: "__custom__" },
  ],
  openrouter: [
    { label: "OpenRouter (official)", url: "https://openrouter.ai/api/v1" },
    { label: "Custom / proxy", url: "__custom__" },
  ],
  minimax: [
    { label: "MiniMax (official)", url: "https://api.minimax.chat/v1" },
    { label: "Custom / proxy", url: "__custom__" },
  ],
  zai: [
    { label: "ZAI (official)", url: "https://api.zai.chat/v1" },
    { label: "Custom / proxy", url: "__custom__" },
  ],
};

const CUSTOM_OPTION = "__custom__";

function getPresetOptions(providerId: string) {
  return PROVIDER_URLS[providerId] ?? [{ label: "Custom / proxy", url: CUSTOM_OPTION }];
}

function detectSelectedPreset(providerId: string, url: string) {
  if (!url) return getPresetOptions(providerId)[0]?.url ?? "";
  const match = getPresetOptions(providerId).find((option) => option.url === url && option.url !== CUSTOM_OPTION);
  return match ? match.url : CUSTOM_OPTION;
}

function providerBadge(provider: Provider) {
  if (!provider.has_key) return { label: "Not connected", bg: "rgba(239,68,68,0.12)", color: "var(--error)" };
  if (provider.key_source === "env") return { label: "Environment", bg: "rgba(59,130,246,0.12)", color: "#60a5fa" };
  if (provider.key_source === "auth") return { label: "API Key", bg: "rgba(34,197,94,0.12)", color: "var(--success)" };
  if (provider.key_source === "config") return { label: "Config", bg: "rgba(168,85,247,0.12)", color: "#c084fc" };
  return { label: "Configured", bg: "rgba(148,163,184,0.12)", color: "var(--text-secondary)" };
}

function sourceBadge(source: ModelInfo["source"]) {
  switch (source) {
    case "configured":
      return { label: "configured", bg: "rgba(34,197,94,0.12)", color: "var(--success)" };
    case "api":
      return { label: "live", bg: "rgba(59,130,246,0.12)", color: "#60a5fa" };
    default:
      return { label: "fallback", bg: "rgba(148,163,184,0.12)", color: "var(--text-secondary)" };
  }
}

export function Settings(props: { onClose: () => void }) {
  const [providers, setProviders] = createSignal<Provider[]>([]);
  const [models, setModels] = createSignal<ModelInfo[]>([]);
  const [activeSection, setActiveSection] = createSignal<SettingsSection>("general");
  const [editingProvider, setEditingProvider] = createSignal<string | null>(null);
  const [apiKey, setApiKey] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [selectedPreset, setSelectedPreset] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [saveError, setSaveError] = createSignal<string | null>(null);
  const [saveSuccess, setSaveSuccess] = createSignal(false);
  const [backendUnreachable, setBackendUnreachable] = createSignal(false);
  const [showCustomForm, setShowCustomForm] = createSignal(false);
  const [customProviderId, setCustomProviderId] = createSignal("");
  const [customApiKey, setCustomApiKey] = createSignal("");
  const [customBaseUrl, setCustomBaseUrl] = createSignal("");
  const [modelSearch, setModelSearch] = createSignal("");

  onMount(async () => {
    await Promise.all([loadProviders(), loadModels()]);
  });

  async function loadProviders() {
    const maxRetries = 5;
    for (let attempt = 0; attempt < maxRetries; attempt++) {
      try {
        const res = await fetch(`${await getApiBase()}/config/providers`);
        if (res.ok) {
          const data = await res.json();
          setProviders(data.providers || []);
          setBackendUnreachable(false);
          return;
        }
        setBackendUnreachable(false);
        return;
      } catch (error) {
        if (attempt < maxRetries - 1) {
          await new Promise((resolve) => setTimeout(resolve, 500));
          continue;
        }
        console.error("Failed to load providers after retries:", error);
        setBackendUnreachable(true);
      }
    }
  }

  async function loadModels() {
    try {
      const res = await fetch(`${await getApiBase()}/models`);
      if (!res.ok) return;
      const data = await res.json();
      setModels(data.models || []);
    } catch (error) {
      console.error("Failed to load models:", error);
    }
  }

  async function saveProvider(providerId: string) {
    setSaving(true);
    setSaveError(null);
    setSaveSuccess(false);

    const body: Record<string, string> = {};
    const key = apiKey().trim();
    if (key) body.api_key = key;
    const url = baseUrl().trim();
    if (url) body.base_url = url;

    try {
      const res = await fetch(`${await getApiBase()}/config/providers/${providerId}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });

      if (!res.ok) {
        const text = await res.text().catch(() => "Unknown error");
        setSaveError(`Server error ${res.status}: ${text}`);
        setSaving(false);
        return;
      }

      await Promise.all([loadProviders(), loadModels()]);
      setSaveSuccess(true);
      setTimeout(() => {
        setEditingProvider(null);
        setApiKey("");
        setBaseUrl("");
        setSelectedPreset("");
        setSaveSuccess(false);
      }, 800);
    } catch (error) {
      setSaveError(`Network error: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setSaving(false);
    }
  }

  async function setProviderEnabled(providerId: string, enabled: boolean) {
    try {
      await fetch(`${await getApiBase()}/config/providers/${providerId}/state`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled }),
      });
      await Promise.all([loadProviders(), loadModels()]);
    } catch (error) {
      setSaveError(`Network error: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  async function setModelEnabled(modelId: string, enabled: boolean) {
    try {
      await fetch(`${await getApiBase()}/config/models/${encodeURIComponent(modelId)}/state`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled }),
      });
      await loadModels();
    } catch (error) {
      setSaveError(`Network error: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  async function addCustomProvider() {
    if (!customProviderId().trim() || !customApiKey().trim() || !customBaseUrl().trim()) {
      return;
    }

    try {
      const res = await fetch(`${await getApiBase()}/config/providers/${customProviderId()}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ api_key: customApiKey(), base_url: customBaseUrl() }),
      });

      if (!res.ok) {
        const text = await res.text().catch(() => "Unknown error");
        setSaveError(`Server error ${res.status}: ${text}`);
        return;
      }

      setCustomProviderId("");
      setCustomApiKey("");
      setCustomBaseUrl("");
      setShowCustomForm(false);
      await Promise.all([loadProviders(), loadModels()]);
    } catch (error) {
      setSaveError(`Network error: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  function startEditing(provider: Provider) {
    setEditingProvider(provider.id);
    setSaveError(null);
    setSaveSuccess(false);
    const savedUrl = provider.base_url || "";
    const preset = detectSelectedPreset(provider.id, savedUrl);
    setSelectedPreset(preset);
    if (savedUrl) {
      setBaseUrl(savedUrl);
    } else {
      const defaultUrl = getPresetOptions(provider.id)[0]?.url ?? "";
      setBaseUrl(defaultUrl !== CUSTOM_OPTION ? defaultUrl : "");
    }
    setApiKey("");
  }

  function cancelEditing() {
    setEditingProvider(null);
    setApiKey("");
    setBaseUrl("");
    setSelectedPreset("");
    setSaveError(null);
    setSaveSuccess(false);
  }

  function handlePresetChange(providerId: string, value: string) {
    setSelectedPreset(value);
    if (value !== CUSTOM_OPTION) {
      setBaseUrl(value);
    } else {
      setBaseUrl("");
    }
  }

  const connectedProviders = createMemo(() => providers().filter((provider) => provider.has_key));
  const popularProviders = createMemo(() => {
    const existingIds = new Set(providers().map((provider) => provider.id));
    return POPULAR_PROVIDER_IDS
      .filter((id) => !existingIds.has(id))
      .map((id) => ({
        id,
        name: id === "opencode-zen" ? "OpenCode Zen" : id === "opencode-go" ? "OpenCode Go" : id.charAt(0).toUpperCase() + id.slice(1),
        description:
          id === "opencode-zen"
            ? "Selected premium models including Claude, GPT, Gemini, and more"
            : id === "opencode-go"
              ? "Economic subscription for everyone"
              : "Connect this provider to unlock its model catalog",
      }));
  });

  const filteredModels = createMemo(() => {
    const query = modelSearch().trim().toLowerCase();
    let visible = models();
    if (query) {
      visible = visible.filter((model) =>
        model.id.toLowerCase().includes(query)
        || (model.display_name || "").toLowerCase().includes(query)
        || model.provider.toLowerCase().includes(query),
      );
    }

    const groups = new Map<string, ModelInfo[]>();
    for (const model of visible) {
      if (!groups.has(model.provider)) {
        groups.set(model.provider, []);
      }
      groups.get(model.provider)!.push(model);
    }

    return [...groups.entries()].map(([provider, items]) => ({
      provider,
      models: [...items].sort((a, b) => {
        const rank = (model: ModelInfo) => {
          if (model.source === "configured") return 0;
          if (model.enabled) return 1;
          return 2;
        };
        return rank(a) - rank(b) || a.id.localeCompare(b.id);
      }),
    }));
  });

  const inputStyle = "width: 100%; padding: 8px 10px; background: var(--bg-secondary); border: 1px solid var(--border); border-radius: var(--radius-md); color: var(--text-primary); font-size: 13px; box-sizing: border-box; outline: none;";
  const sectionTitleStyle = "margin: 0 0 18px; color: var(--text-primary); font-size: 31px; font-weight: 600;";
  const cardStyle = "background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 16px; padding: 16px 18px;";

  const renderProvidersSection = () => (
    <div>
      <h2 style={sectionTitleStyle}>Providers</h2>
      <p style="margin: 0 0 22px; color: var(--text-secondary); font-size: 14px;">Connected providers</p>

      <div style={`${cardStyle}; display: flex; flex-direction: column; gap: 0;`}>
        <For each={connectedProviders()}>
          {(provider, index) => {
            const badge = providerBadge(provider);
            const presets = getPresetOptions(provider.id);
            const hasPresets = presets.length > 1;
            return (
              <div style={`padding: 14px 0; ${index() > 0 ? "border-top: 1px solid var(--border);" : ""}`}>
                <div style="display: flex; justify-content: space-between; align-items: center; gap: 16px;">
                  <div style="display: flex; align-items: center; gap: 12px; min-width: 0;">
                    <div style="width: 28px; height: 28px; border-radius: 8px; background: var(--bg-tertiary); display: flex; align-items: center; justify-content: center; font-size: 13px; font-weight: 700; color: var(--text-primary); text-transform: uppercase;">
                      {provider.name.slice(0, 1)}
                    </div>
                    <div>
                      <div style="display: flex; align-items: center; gap: 8px; margin-bottom: 2px;">
                        <strong style="font-size: 15px; color: var(--text-primary);">{provider.name}</strong>
                        <span style={`font-size: 11px; padding: 2px 8px; border-radius: 999px; background: ${badge.bg}; color: ${badge.color};`}>
                          {badge.label}
                        </span>
                      </div>
                      <Show when={provider.base_url}>
                        <div style="font-size: 12px; color: var(--text-secondary);">{provider.base_url}</div>
                      </Show>
                    </div>
                  </div>
                  <button
                    onClick={() => setProviderEnabled(provider.id, false)}
                    style="padding: 8px 14px; border-radius: 10px; border: 1px solid var(--border); background: none; color: var(--text-primary); cursor: pointer; font-size: 14px;"
                  >
                    Disconnect
                  </button>
                </div>

                <Show when={editingProvider() === provider.id}>
                  <div style="margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border);">
                    <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 12px;">
                      <div>
                        <label style="display: block; margin-bottom: 6px; font-size: 12px; color: var(--text-secondary);">API Key</label>
                        <input
                          type="password"
                          value={apiKey()}
                          onInput={(e) => setApiKey(e.currentTarget.value)}
                          placeholder={provider.has_key ? "Leave empty to keep current key" : "Enter API key"}
                          style={inputStyle}
                        />
                      </div>
                      <div>
                        <label style="display: block; margin-bottom: 6px; font-size: 12px; color: var(--text-secondary);">Base URL</label>
                        <Show when={hasPresets}>
                          <select
                            value={selectedPreset()}
                            onInput={(e) => handlePresetChange(provider.id, e.currentTarget.value)}
                            style={`${inputStyle}; margin-bottom: 8px;`}
                          >
                            <For each={presets}>{(option) => <option value={option.url}>{option.label}</option>}</For>
                          </select>
                        </Show>
                        <Show when={!hasPresets || selectedPreset() === CUSTOM_OPTION}>
                          <input
                            type="text"
                            value={baseUrl()}
                            onInput={(e) => setBaseUrl(e.currentTarget.value)}
                            placeholder="https://api.example.com/v1"
                            style={inputStyle}
                          />
                        </Show>
                        <Show when={hasPresets && selectedPreset() !== CUSTOM_OPTION && selectedPreset() !== ""}>
                          <div style="margin-top: 4px; font-size: 12px; color: var(--text-secondary);">{baseUrl()}</div>
                        </Show>
                      </div>
                    </div>
                    <Show when={saveError()}>
                      <div style="margin-top: 12px; padding: 8px 10px; background: rgba(239,68,68,0.12); border: 1px solid rgba(239,68,68,0.3); border-radius: 10px; color: var(--error); font-size: 12px;">{saveError()}</div>
                    </Show>
                    <Show when={saveSuccess()}>
                      <div style="margin-top: 12px; padding: 8px 10px; background: rgba(34,197,94,0.12); border: 1px solid rgba(34,197,94,0.3); border-radius: 10px; color: var(--success); font-size: 12px;">Saved successfully</div>
                    </Show>
                    <div style="display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px;">
                      <button onClick={cancelEditing} style="padding: 8px 14px; border-radius: 10px; border: 1px solid var(--border); background: none; color: var(--text-secondary); cursor: pointer;">Cancel</button>
                      <button onClick={() => saveProvider(provider.id)} style={`padding: 8px 14px; border-radius: 10px; border: none; background: var(--accent); color: white; cursor: pointer; opacity: ${saving() ? "0.6" : "1"};`}>{saving() ? "Saving..." : "Save"}</button>
                    </div>
                  </div>
                </Show>
              </div>
            );
          }}
        </For>
      </div>

      <p style="margin: 28px 0 14px; color: var(--text-secondary); font-size: 14px;">Popular providers</p>
      <div style={`${cardStyle}; display: flex; flex-direction: column; gap: 0;`}>
        <For each={popularProviders()}>
          {(provider, index) => (
            <div style={`padding: 14px 0; ${index() > 0 ? "border-top: 1px solid var(--border);" : ""}`}>
              <div style="display: flex; justify-content: space-between; align-items: center; gap: 16px;">
                <div>
                  <div style="display: flex; align-items: center; gap: 8px; margin-bottom: 4px;">
                    <strong style="font-size: 15px; color: var(--text-primary);">{provider.name}</strong>
                    <span style="font-size: 11px; padding: 2px 8px; border-radius: 999px; background: rgba(148,163,184,0.14); color: var(--text-secondary);">recommended</span>
                  </div>
                  <div style="font-size: 13px; color: var(--text-secondary);">{provider.description}</div>
                </div>
                <button
                  onClick={() => {
                    setActiveSection("providers");
                    setShowCustomForm(true);
                    setCustomProviderId(provider.id);
                  }}
                  style="padding: 8px 14px; border-radius: 10px; border: 1px solid var(--border); background: rgba(255,255,255,0.04); color: var(--text-primary); cursor: pointer;"
                >
                  + Connect
                </button>
              </div>
            </div>
          )}
        </For>
      </div>

      <div style="margin-top: 22px;">
        <Show when={!showCustomForm()}>
          <button
            onClick={() => setShowCustomForm(true)}
            style="width: 100%; padding: 12px 14px; border-radius: 12px; border: 1px dashed var(--border); background: none; color: var(--text-secondary); cursor: pointer;"
          >
            + Add custom provider
          </button>
        </Show>

        <Show when={showCustomForm()}>
          <div style={`${cardStyle}; margin-top: 10px;`}>
            <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 12px;">
              <div>
                <label style="display: block; margin-bottom: 6px; font-size: 12px; color: var(--text-secondary);">Provider ID</label>
                <input value={customProviderId()} onInput={(e) => setCustomProviderId(e.currentTarget.value)} placeholder="my-provider" style={inputStyle} />
              </div>
              <div>
                <label style="display: block; margin-bottom: 6px; font-size: 12px; color: var(--text-secondary);">API Key</label>
                <input type="password" value={customApiKey()} onInput={(e) => setCustomApiKey(e.currentTarget.value)} placeholder="Enter API key" style={inputStyle} />
              </div>
            </div>
            <div style="margin-top: 12px;">
              <label style="display: block; margin-bottom: 6px; font-size: 12px; color: var(--text-secondary);">Base URL</label>
              <input value={customBaseUrl()} onInput={(e) => setCustomBaseUrl(e.currentTarget.value)} placeholder="https://api.example.com/v1" style={inputStyle} />
            </div>
            <div style="display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px;">
              <button onClick={() => setShowCustomForm(false)} style="padding: 8px 14px; border-radius: 10px; border: 1px solid var(--border); background: none; color: var(--text-secondary); cursor: pointer;">Cancel</button>
              <button onClick={addCustomProvider} style="padding: 8px 14px; border-radius: 10px; border: none; background: var(--accent); color: white; cursor: pointer;">Add provider</button>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );

  const renderModelsSection = () => (
    <div>
      <h2 style={sectionTitleStyle}>Models</h2>
      <div style="margin-bottom: 18px;">
        <input
          value={modelSearch()}
          onInput={(e) => setModelSearch(e.currentTarget.value)}
          placeholder="Search models"
          style={`${inputStyle}; padding-left: 14px;`}
        />
      </div>

      <For each={filteredModels()}>
        {(group) => (
          <div style="margin-bottom: 20px;">
            <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 10px;">
              <strong style="font-size: 22px; color: var(--text-primary); text-transform: capitalize;">{group.provider}</strong>
            </div>
            <div style={`${cardStyle}; display: flex; flex-direction: column; gap: 0;`}>
              <For each={group.models}>
                {(model, index) => {
                  const badge = sourceBadge(model.source);
                  return (
                    <div style={`padding: 14px 0; display: flex; justify-content: space-between; align-items: center; gap: 14px; ${index() > 0 ? "border-top: 1px solid var(--border);" : ""}`}>
                      <div style="min-width: 0;">
                        <div style="display: flex; align-items: center; gap: 8px; margin-bottom: 4px;">
                          <span style="font-size: 15px; color: var(--text-primary);">{model.display_name || model.id.split("/")[1] || model.id}</span>
                          <span style={`font-size: 11px; padding: 2px 8px; border-radius: 999px; background: ${badge.bg}; color: ${badge.color};`}>
                            {badge.label}
                          </span>
                        </div>
                        <div style="font-size: 12px; color: var(--text-secondary);">{model.id}</div>
                      </div>
                      <div
                        onClick={() => void setModelEnabled(model.id, !model.enabled)}
                        style={`width: 30px; height: 18px; border-radius: 999px; border: 1px solid var(--border); background: ${model.enabled ? "rgba(255,255,255,0.85)" : "rgba(255,255,255,0.08)"}; position: relative;`}
                        title={model.enabled ? "Available" : "Not available"}
                      >
                        <div style={`position: absolute; top: 1px; ${model.enabled ? "right: 1px;" : "left: 1px;"} width: 14px; height: 14px; border-radius: 50%; background: ${model.enabled ? "#111" : "rgba(255,255,255,0.35)"};`} />
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>
        )}
      </For>
    </div>
  );

  const renderGeneralSection = () => (
    <div>
      <h2 style={sectionTitleStyle}>General</h2>
      <div style={cardStyle}>
        <div style="font-size: 15px; color: var(--text-primary); margin-bottom: 10px;">Desktop settings</div>
        <div style="font-size: 13px; color: var(--text-secondary); line-height: 1.6;">
          This section is now structured like OpenCode Desktop and is ready to host theme, startup, telemetry, and default behavior settings.
        </div>
      </div>
    </div>
  );

  const renderShortcutsSection = () => (
    <div>
      <h2 style={sectionTitleStyle}>Shortcuts</h2>
      <div style={cardStyle}>
        <div style="display: flex; flex-direction: column; gap: 12px;">
          <div style="display: flex; justify-content: space-between; color: var(--text-primary);"><span>Open settings</span><span style="color: var(--text-secondary);">Ctrl+,</span></div>
          <div style="display: flex; justify-content: space-between; color: var(--text-primary);"><span>Toggle terminal</span><span style="color: var(--text-secondary);">Ctrl+`</span></div>
          <div style="display: flex; justify-content: space-between; color: var(--text-primary);"><span>New session</span><span style="color: var(--text-secondary);">Ctrl+N</span></div>
        </div>
      </div>
    </div>
  );

  return (
    <div style="position: fixed; inset: 0; background: rgba(0,0,0,0.5); display: flex; align-items: stretch; justify-content: center; z-index: 1000;">
      <div style="width: min(1100px, 96vw); height: min(780px, 92vh); margin: auto; background: var(--bg-primary); color: var(--text-primary); border-radius: 18px; overflow: hidden; display: flex; box-shadow: 0 30px 100px rgba(0,0,0,0.45); border: 1px solid var(--border-strong);">
        <aside style="width: 220px; border-right: 1px solid var(--border); background: var(--bg-secondary); padding: 18px 12px; display: flex; flex-direction: column; justify-content: space-between;">
          <div>
            <For each={["Desktop", "Server"]}>
              {(group) => (
                <div style="margin-bottom: 18px;">
                  <div style="padding: 8px 10px; font-size: 12px; color: var(--text-secondary); text-transform: none;">{group}</div>
                  <For each={SETTINGS_NAV.filter((item) => item.group === group)}>
                    {(item) => (
                      <button
                        onClick={() => setActiveSection(item.id)}
                        style={`width: 100%; display: flex; align-items: center; gap: 10px; padding: 10px 12px; border: none; border-radius: 10px; cursor: pointer; background: ${activeSection() === item.id ? "var(--bg-tertiary)" : "transparent"}; color: var(--text-primary); text-align: left; font-size: 15px;`}
                      >
                        <span style="width: 18px; text-align: center; opacity: 0.75;">{item.icon}</span>
                        <span>{item.title}</span>
                      </button>
                    )}
                  </For>
                </div>
              )}
            </For>
          </div>

          <div style="padding: 8px 10px; color: var(--text-secondary); font-size: 13px; line-height: 1.5;">
            <div>RCode Desktop</div>
            <div>v0.1.0</div>
          </div>
        </aside>

        <section style="flex: 1; padding: 26px 36px; overflow-y: auto; background: var(--bg-primary);">
          <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 18px;">
            <div />
            <button onClick={props.onClose} style="width: 32px; height: 32px; border-radius: 999px; border: 1px solid var(--border); background: var(--bg-secondary); color: var(--text-secondary); cursor: pointer;">×</button>
          </div>

          <Show when={backendUnreachable()}>
            <div style="margin-bottom: 18px; padding: 12px 14px; background: rgba(239,68,68,0.10); border: 1px solid rgba(239,68,68,0.25); border-radius: 12px; color: #b91c1c; font-size: 13px;">
              Backend not available. Make sure the server is running or that `VITE_API_BASE` points to the correct address.
            </div>
          </Show>

          <Show when={activeSection() === "general"}>{renderGeneralSection()}</Show>
          <Show when={activeSection() === "shortcuts"}>{renderShortcutsSection()}</Show>
          <Show when={activeSection() === "providers"}>{renderProvidersSection()}</Show>
          <Show when={activeSection() === "models"}>{renderModelsSection()}</Show>
        </section>
      </div>
    </div>
  );
}
