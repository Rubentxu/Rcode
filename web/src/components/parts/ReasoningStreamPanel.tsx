import { type Component, Show, createSignal } from "solid-js";

interface ReasoningStreamPanelProps {
  content: string;
  defaultCollapsed?: boolean;
}

/**
 * Collapsible reasoning panel for live reasoning display during streaming.
 * Replaced by ReasoningBlock on commit.
 */
export const ReasoningStreamPanel: Component<ReasoningStreamPanelProps> = (props) => {
  const [collapsed, setCollapsed] = createSignal(props.defaultCollapsed ?? true);
  
  const handleToggle = () => {
    setCollapsed(!collapsed());
  };
  
  return (
    <div 
      data-component="reasoning-stream-panel"
      class="border-l border-secondary/50 px-3 py-2"
    >
      <button 
        data-component="reasoning-toggle"
        class="flex items-center gap-2 cursor-pointer w-full"
        onClick={handleToggle}
        type="button"
      >
        <span class="material-symbols-outlined text-secondary text-sm" style="font-variation-settings: 'FILL' 1;">memory</span>
        <span class="text-[11px] font-mono uppercase tracking-widest text-secondary/70">Reasoning</span>
        <span class="material-symbols-outlined text-outline text-xs ml-auto">
          {collapsed() ? "expand_more" : "expand_less"}
        </span>
      </button>
      
      <Show when={!collapsed()}>
        <div data-component="reasoning-content" class="font-mono text-sm text-on-surface-variant/80 space-y-1 mt-2">
          <pre class="whitespace-pre-wrap">{props.content}</pre>
        </div>
      </Show>
      
      {collapsed() && (
        <div class="text-xs text-outline font-mono truncate">
          Click to expand...
        </div>
      )}
    </div>
  );
};
