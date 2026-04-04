import { createSignal, onMount, For, Show } from "solid-js";

interface Provider {
  id: string;
  name: string;
  has_key: boolean;
  base_url: string | null;
  enabled: boolean;
}

// Known base URLs per provider. Values are [label, url] pairs.
// The first entry is always the official default.
const PROVIDER_URLS: Record<string, Array<{ label: string; url: string }>> = {
  anthropic: [
    { label: "Anthropic (official)", url: "https://api.anthropic.com" },
    { label: "Custom / proxy", url: "" },
  ],
  openai: [
    { label: "OpenAI (official)", url: "https://api.openai.com" },
    { label: "Azure OpenAI", url: "https://<resource>.openai.azure.com" },
    { label: "LM Studio (local)", url: "http://localhost:1234" },
    { label: "Ollama (local)", url: "http://localhost:11434" },
    { label: "Custom / proxy", url: "" },
  ],
  google: [
    { label: "Google AI (official)", url: "https://generativelanguage.googleapis.com" },
    { label: "Vertex AI", url: "https://<region>-aiplatform.googleapis.com" },
    { label: "Custom / proxy", url: "" },
  ],
  openrouter: [
    { label: "OpenRouter (official)", url: "https://openrouter.ai/api/v1" },
    { label: "Custom / proxy", url: "" },
  ],
  minimax: [
    { label: "MiniMax (official)", url: "https://api.minimax.chat/v1" },
    { label: "Custom / proxy", url: "" },
  ],
  zai: [
    { label: "ZAI (official)", url: "https://api.zai.chat/v1" },
    { label: "Custom / proxy", url: "" },
  ],
};

const CUSTOM_OPTION = "custom";

function getPresetOptions(providerId: string) {
  return PROVIDER_URLS[providerId] ?? [{ label: "Custom / proxy", url: "" }];
}

/** Return "custom" if url doesn't match any preset, otherwise the preset url. */
function detectSelectedPreset(providerId: string, url: string): string {
  if (!url) return getPresetOptions(providerId)[0]?.url ?? "";
  const match = getPresetOptions(providerId).find((o) => o.url === url);
  return match ? match.url : CUSTOM_OPTION;
}

export function Settings(props: { onClose: () => void }) {
  const [providers, setProviders] = createSignal<Provider[]>([]);
  const [editingProvider, setEditingProvider] = createSignal<string | null>(null);
  const [apiKey, setApiKey] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [selectedPreset, setSelectedPreset] = createSignal<string>("");

  // Custom provider form state
  const [showCustomForm, setShowCustomForm] = createSignal(false);
  const [customProviderId, setCustomProviderId] = createSignal("");
  const [customApiKey, setCustomApiKey] = createSignal("");
  const [customBaseUrl, setCustomBaseUrl] = createSignal("");

  const API_BASE = import.meta.env.VITE_API_BASE || "http://localhost:4096";

  onMount(async () => {
    await loadProviders();
  });

  async function loadProviders() {
    try {
      const res = await fetch(`${API_BASE}/config/providers`);
      if (res.ok) {
        const data = await res.json();
        setProviders(data.providers || []);
      }
    } catch (e) {
      console.error("Failed to load providers:", e);
    }
  }

  async function saveProvider(providerId: string) {
    try {
      await fetch(`${API_BASE}/config/providers/${providerId}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          api_key: apiKey() || undefined,
          base_url: baseUrl() || undefined,
        }),
      });
      setEditingProvider(null);
      setApiKey("");
      setBaseUrl("");
      setSelectedPreset("");
      await loadProviders();
    } catch (e) {
      console.error("Failed to save provider:", e);
    }
  }

  async function addCustomProvider() {
    if (!customProviderId().trim() || !customApiKey().trim() || !customBaseUrl().trim()) {
      return;
    }

    try {
      await fetch(`${API_BASE}/config/providers/${customProviderId()}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          api_key: customApiKey(),
          base_url: customBaseUrl(),
        }),
      });

      setCustomProviderId("");
      setCustomApiKey("");
      setCustomBaseUrl("");
      setShowCustomForm(false);
      await loadProviders();
    } catch (e) {
      console.error("Failed to add custom provider:", e);
    }
  }

  function startEditing(provider: Provider) {
    setEditingProvider(provider.id);
    const url = provider.base_url || "";
    const preset = detectSelectedPreset(provider.id, url);
    setSelectedPreset(preset);
    setBaseUrl(url);
    setApiKey("");
  }

  function cancelEditing() {
    setEditingProvider(null);
    setApiKey("");
    setBaseUrl("");
    setSelectedPreset("");
  }

  function handlePresetChange(providerId: string, value: string) {
    setSelectedPreset(value);
    if (value !== CUSTOM_OPTION) {
      setBaseUrl(value);
    } else {
      setBaseUrl("");
    }
  }

  const inputStyle =
    "width: 100%; padding: 8px; background: var(--bg-secondary); border: 1px solid var(--border); border-radius: var(--radius-md); color: var(--text-primary); font-size: 13px; box-sizing: border-box;";

  const labelStyle =
    "display: block; font-size: 12px; color: var(--text-secondary); margin-bottom: 4px;";

  return (
    <div style="position: fixed; inset: 0; background: rgba(0,0,0,0.5); display: flex; align-items: center; justify-content: center; z-index: 1000;">
      <div style="background: var(--bg-primary); border-radius: 12px; padding: 24px; width: 620px; max-height: 80vh; overflow-y: auto;">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px;">
          <h2 style="margin: 0; color: var(--text-primary);">Settings</h2>
          <button
            onClick={props.onClose}
            style="background: none; border: none; color: var(--text-secondary); cursor: pointer; font-size: 20px; padding: 4px;"
          >
            ✕
          </button>
        </div>

        <h3 style="color: var(--text-primary); margin-bottom: 12px;">Providers</h3>

        <div style="display: flex; flex-direction: column; gap: 8px;">
          <For each={providers()}>
            {(provider) => {
              const presets = getPresetOptions(provider.id);
              const hasPresets = presets.length > 1;

              return (
                <div style="border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 12px;">
                  <div style="display: flex; justify-content: space-between; align-items: center;">
                    <div style="display: flex; align-items: center; gap: 8px;">
                      <strong style="color: var(--text-primary);">{provider.name}</strong>
                      <span
                        style={{
                          "font-size": "12px",
                          padding: "2px 6px",
                          "border-radius": "4px",
                          background: provider.has_key
                            ? "rgba(34, 197, 94, 0.15)"
                            : "rgba(239, 68, 68, 0.15)",
                          color: provider.has_key ? "var(--success)" : "var(--error)",
                        }}
                      >
                        {provider.has_key ? "✓ Key set" : "✗ No key"}
                      </span>
                    </div>
                    <button
                      onClick={() => startEditing(provider)}
                      style="background: var(--bg-tertiary); border: 1px solid var(--border); color: var(--text-primary); padding: 4px 12px; border-radius: var(--radius-md); cursor: pointer; font-size: 13px;"
                    >
                      Configure
                    </button>
                  </div>

                  <Show when={editingProvider() === provider.id}>
                    <div style="margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border);">
                      {/* API Key */}
                      <div style="margin-bottom: 8px;">
                        <label style={labelStyle}>API Key</label>
                        <input
                          type="password"
                          value={apiKey()}
                          onInput={(e) => setApiKey(e.currentTarget.value)}
                          placeholder={
                            provider.has_key
                              ? "Leave empty to keep existing"
                              : "Enter API key..."
                          }
                          style={inputStyle}
                        />
                      </div>

                      {/* Base URL selector */}
                      <div style="margin-bottom: 8px;">
                        <label style={labelStyle}>
                          Base URL
                          {hasPresets ? "" : " (optional)"}
                        </label>

                        {/* Preset selector — only shown when we know the URLs */}
                        <Show when={hasPresets}>
                          <select
                            value={selectedPreset()}
                            onChange={(e) =>
                              handlePresetChange(provider.id, e.currentTarget.value)
                            }
                            style={`${inputStyle} margin-bottom: 6px; appearance: auto;`}
                          >
                            <For each={presets}>
                              {(opt) => (
                                <option value={opt.url === "" ? CUSTOM_OPTION : opt.url}>
                                  {opt.label}
                                </option>
                              )}
                            </For>
                          </select>
                        </Show>

                        {/* Manual input — always visible when custom, or when no presets */}
                        <Show
                          when={!hasPresets || selectedPreset() === CUSTOM_OPTION}
                        >
                          <input
                            type="text"
                            value={baseUrl()}
                            onInput={(e) => setBaseUrl(e.currentTarget.value)}
                            placeholder="https://api.example.com/v1"
                            style={inputStyle}
                          />
                        </Show>

                        {/* Show selected URL as read-only hint when a preset is chosen */}
                        <Show
                          when={
                            hasPresets &&
                            selectedPreset() !== CUSTOM_OPTION &&
                            selectedPreset() !== ""
                          }
                        >
                          <div
                            style="margin-top: 4px; font-size: 12px; color: var(--text-secondary); font-family: monospace; padding: 4px 8px; background: var(--bg-secondary); border-radius: var(--radius-md); border: 1px solid var(--border);"
                          >
                            {selectedPreset()}
                          </div>
                        </Show>
                      </div>

                      <div style="display: flex; gap: 8px; justify-content: flex-end;">
                        <button
                          onClick={cancelEditing}
                          style="padding: 6px 16px; border-radius: var(--radius-md); border: 1px solid var(--border); background: none; color: var(--text-secondary); cursor: pointer; font-size: 13px;"
                        >
                          Cancel
                        </button>
                        <button
                          onClick={() => saveProvider(provider.id)}
                          style="padding: 6px 16px; border-radius: var(--radius-md); border: none; background: var(--accent); color: white; cursor: pointer; font-size: 13px;"
                        >
                          Save
                        </button>
                      </div>
                    </div>
                  </Show>
                </div>
              );
            }}
          </For>
        </div>

        <h3 style="color: var(--text-primary); margin-top: 24px; margin-bottom: 12px;">
          Custom Provider
        </h3>
        <p style="font-size: 13px; color: var(--text-secondary); margin-bottom: 12px;">
          Add any OpenAI-compatible API as a custom provider.
        </p>

        <Show when={!showCustomForm()}>
          <button
            onClick={() => setShowCustomForm(true)}
            style="padding: 8px 16px; border-radius: var(--radius-md); border: 1px dashed var(--border); background: none; color: var(--text-secondary); cursor: pointer; font-size: 13px; width: 100%;"
          >
            + Add Custom Provider
          </button>
        </Show>

        <Show when={showCustomForm()}>
          <div style="border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 12px; margin-top: 8px;">
            <div style="margin-bottom: 8px;">
              <label style={labelStyle}>Provider ID</label>
              <input
                type="text"
                value={customProviderId()}
                onInput={(e) => setCustomProviderId(e.currentTarget.value)}
                placeholder="e.g., my-llm"
                style={inputStyle}
              />
            </div>
            <div style="margin-bottom: 8px;">
              <label style={labelStyle}>API Key</label>
              <input
                type="password"
                value={customApiKey()}
                onInput={(e) => setCustomApiKey(e.currentTarget.value)}
                placeholder="Enter API key..."
                style={inputStyle}
              />
            </div>
            <div style="margin-bottom: 8px;">
              <label style={labelStyle}>Base URL (required)</label>
              <input
                type="text"
                value={customBaseUrl()}
                onInput={(e) => setCustomBaseUrl(e.currentTarget.value)}
                placeholder="https://api.example.com/v1"
                style={inputStyle}
              />
              <p style="margin: 4px 0 0; font-size: 11px; color: var(--text-secondary);">
                Common: LM Studio → http://localhost:1234 · Ollama → http://localhost:11434
              </p>
            </div>
            <div style="display: flex; gap: 8px; justify-content: flex-end;">
              <button
                onClick={() => {
                  setShowCustomForm(false);
                  setCustomProviderId("");
                  setCustomApiKey("");
                  setCustomBaseUrl("");
                }}
                style="padding: 6px 16px; border-radius: var(--radius-md); border: 1px solid var(--border); background: none; color: var(--text-secondary); cursor: pointer; font-size: 13px;"
              >
                Cancel
              </button>
              <button
                onClick={addCustomProvider}
                style="padding: 6px 16px; border-radius: var(--radius-md); border: none; background: var(--accent); color: white; cursor: pointer; font-size: 13px;"
              >
                Add Provider
              </button>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );
}
