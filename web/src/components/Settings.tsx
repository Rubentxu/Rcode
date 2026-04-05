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
    { label: "Vertex AI", url: "https://<region>-aiplatform.googleapis.com" },
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

/** Return preset url that matches, or CUSTOM_OPTION if none matches. */
function detectSelectedPreset(providerId: string, url: string): string {
  if (!url) return getPresetOptions(providerId)[0]?.url ?? "";
  const match = getPresetOptions(providerId).find((o) => o.url === url && o.url !== CUSTOM_OPTION);
  return match ? match.url : CUSTOM_OPTION;
}

export function Settings(props: { onClose: () => void }) {
  const [providers, setProviders] = createSignal<Provider[]>([]);
  const [editingProvider, setEditingProvider] = createSignal<string | null>(null);
  const [apiKey, setApiKey] = createSignal("");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [selectedPreset, setSelectedPreset] = createSignal<string>("");
  const [saving, setSaving] = createSignal(false);
  const [saveError, setSaveError] = createSignal<string | null>(null);
  const [saveSuccess, setSaveSuccess] = createSignal(false);
  const [backendUnreachable, setBackendUnreachable] = createSignal(false);

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
        setBackendUnreachable(false);
      } else {
        setBackendUnreachable(false);
      }
    } catch (e) {
      console.error("Failed to load providers:", e);
      setBackendUnreachable(true);
    }
  }

  async function saveProvider(providerId: string) {
    setSaving(true);
    setSaveError(null);
    setSaveSuccess(false);

    // Build body — only include fields that have values
    const body: Record<string, string> = {};
    const key = apiKey().trim();
    if (key) body.api_key = key;

    // Always include the current base URL (even if unchanged) so the server stores it
    const url = baseUrl().trim();
    if (url) body.base_url = url;

    try {
      const res = await fetch(`${API_BASE}/config/providers/${providerId}`, {
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

      // Success — reload provider list so status badge updates
      await loadProviders();
      setSaveSuccess(true);

      // Close form after a short delay so the user sees "Saved ✓"
      setTimeout(() => {
        setEditingProvider(null);
        setApiKey("");
        setBaseUrl("");
        setSelectedPreset("");
        setSaveSuccess(false);
      }, 800);
    } catch (e) {
      setSaveError(`Network error: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSaving(false);
    }
  }

  async function addCustomProvider() {
    if (!customProviderId().trim() || !customApiKey().trim() || !customBaseUrl().trim()) {
      return;
    }

    try {
      const res = await fetch(`${API_BASE}/config/providers/${customProviderId()}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          api_key: customApiKey(),
          base_url: customBaseUrl(),
        }),
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
      await loadProviders();
    } catch (e) {
      setSaveError(`Network error: ${e instanceof Error ? e.message : String(e)}`);
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
      // Use the saved URL as-is (visible in the text hint below the select)
      setBaseUrl(savedUrl);
    } else {
      // No URL saved yet — pre-fill with the official default for this provider
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

  // Styles
  const inputStyle = [
    "width: 100%",
    "padding: 8px 10px",
    "background: var(--bg-secondary)",
    "border: 1px solid var(--border)",
    "border-radius: var(--radius-md)",
    "color: var(--text-primary)",
    "font-size: 13px",
    "box-sizing: border-box",
    "outline: none",
  ].join("; ");

  const selectStyle = [
    "width: 100%",
    "padding: 8px 10px",
    "background: var(--bg-secondary)",
    "border: 1px solid var(--border)",
    "border-radius: var(--radius-md)",
    "color: var(--text-primary)",
    "font-size: 13px",
    "box-sizing: border-box",
    "outline: none",
    "cursor: pointer",
    "margin-bottom: 6px",
    // Force the dropdown arrow to be visible with appearance
    "-webkit-appearance: none",
    "-moz-appearance: none",
    "appearance: none",
    "background-image: url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23888' d='M6 8L1 3h10z'/%3E%3C/svg%3E\")",
    "background-repeat: no-repeat",
    "background-position: right 10px center",
    "padding-right: 28px",
  ].join("; ");

  const labelStyle = "display: block; font-size: 12px; color: var(--text-secondary); margin-bottom: 4px;";

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

        <Show when={backendUnreachable()}>
          <div style="margin-bottom: 16px; padding: 10px 14px; background: rgba(239,68,68,0.12); border: 1px solid rgba(239,68,68,0.3); border-radius: var(--radius-md); font-size: 13px; color: var(--error);">
            ⚠ Backend not available. Make sure the rcode server is running (<code>opencode serve</code>) or set <code>VITE_API_BASE</code> to the correct address.
          </div>
        </Show>

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
                              ? "Leave empty to keep existing key"
                              : "Enter API key..."
                          }
                          style={inputStyle}
                        />
                      </div>

                      {/* Base URL */}
                      <div style="margin-bottom: 12px;">
                        <label style={labelStyle}>
                          Base URL{hasPresets ? "" : " (optional)"}
                        </label>

                        {/* Preset dropdown */}
                        <Show when={hasPresets}>
                          <select
                            value={selectedPreset()}
                            onInput={(e) =>
                              handlePresetChange(provider.id, e.currentTarget.value)
                            }
                            style={selectStyle}
                          >
                            <For each={presets}>
                              {(opt) => (
                                <option value={opt.url}>
                                  {opt.label}
                                </option>
                              )}
                            </For>
                          </select>
                        </Show>

                        {/* Manual input — shown only when "Custom / proxy" is chosen */}
                        <Show when={!hasPresets || selectedPreset() === CUSTOM_OPTION}>
                          <input
                            type="text"
                            value={baseUrl()}
                            onInput={(e) => setBaseUrl(e.currentTarget.value)}
                            placeholder="https://api.example.com/v1"
                            style={inputStyle}
                          />
                        </Show>

                        {/* Read-only URL hint when a known preset is selected */}
                        <Show
                          when={
                            hasPresets &&
                            selectedPreset() !== CUSTOM_OPTION &&
                            selectedPreset() !== ""
                          }
                        >
                          <div style="margin-top: 4px; font-size: 12px; color: var(--text-secondary); font-family: monospace; padding: 6px 8px; background: var(--bg-secondary); border-radius: var(--radius-md); border: 1px solid var(--border);">
                            {baseUrl()}
                          </div>
                        </Show>
                      </div>

                      {/* Error / success feedback */}
                      <Show when={saveError()}>
                        <div style="margin-bottom: 8px; padding: 6px 10px; background: rgba(239,68,68,0.12); border: 1px solid rgba(239,68,68,0.3); border-radius: var(--radius-md); font-size: 12px; color: var(--error);">
                          {saveError()}
                        </div>
                      </Show>
                      <Show when={saveSuccess()}>
                        <div style="margin-bottom: 8px; padding: 6px 10px; background: rgba(34,197,94,0.12); border: 1px solid rgba(34,197,94,0.3); border-radius: var(--radius-md); font-size: 12px; color: var(--success);">
                          ✓ Saved successfully
                        </div>
                      </Show>

                      {/* Action buttons */}
                      <div style="display: flex; gap: 8px; justify-content: flex-end;">
                        <button
                          onClick={cancelEditing}
                          disabled={saving()}
                          style="padding: 6px 16px; border-radius: var(--radius-md); border: 1px solid var(--border); background: none; color: var(--text-secondary); cursor: pointer; font-size: 13px;"
                        >
                          Cancel
                        </button>
                        <button
                          onClick={() => saveProvider(provider.id)}
                          disabled={saving()}
                          style={`padding: 6px 16px; border-radius: var(--radius-md); border: none; background: var(--accent); color: white; cursor: pointer; font-size: 13px; opacity: ${saving() ? "0.6" : "1"};`}
                        >
                          {saving() ? "Saving…" : "Save"}
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
