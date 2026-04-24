import { type Component, Show, createSignal } from "solid-js";

interface ReasoningStreamPanelProps {
  content: string;
  defaultCollapsed?: boolean;
}

/**
 * Collapsible reasoning panel for live reasoning display during streaming.
 * Replaced by ReasoningBlock on commit.
 *
 * UI/UX: Enhanced with clearer visual hierarchy as a "process layer" element.
 */
export const ReasoningStreamPanel: Component<ReasoningStreamPanelProps> = (props) => {
  const [collapsed, setCollapsed] = createSignal(props.defaultCollapsed ?? true);

  const handleToggle = () => {
    setCollapsed(!collapsed());
  };

  return (
    <div
      data-component="reasoning-stream-panel"
      class="reasoning-block"
    >
      <button
        data-component="reasoning-toggle"
        class="reasoning-block-header w-full"
        onClick={handleToggle}
        type="button"
        aria-expanded={!collapsed()}
      >
        <span class="material-symbols-outlined text-secondary" style="font-size: 14px; font-variation-settings: 'FILL' 1;">psychology</span>
        <span class="reasoning-block-label">Reasoning</span>
        <Show when={collapsed()} fallback={
          <span class="material-symbols-outlined text-outline text-sm ml-auto">
            expand_less
          </span>
        }>
          <span class="reasoning-block-preview">
            Click to expand...
          </span>
          <span class="material-symbols-outlined text-outline text-sm ml-auto">
            expand_more
          </span>
        </Show>
      </button>

      <Show when={!collapsed()}>
        <div data-component="reasoning-content" class="reasoning-content font-mono text-sm text-on-surface-variant/80 space-y-1">
          <pre class="whitespace-pre-wrap">{props.content}</pre>
        </div>
      </Show>
    </div>
  );
};
