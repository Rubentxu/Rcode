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
      class="reasoning-panel"
    >
      <button 
        data-component="reasoning-toggle"
        class="reasoning-toggle"
        onClick={handleToggle}
        type="button"
      >
        <span class="reasoning-icon">{collapsed() ? "💭" : "💭"}</span>
        <span class="reasoning-label">
          {collapsed() ? "Show reasoning" : "Hide reasoning"}
        </span>
        <span class="toggle-arrow">{collapsed() ? "▶" : "▼"}</span>
      </button>
      
      <Show when={!collapsed()}>
        <div data-component="reasoning-content" class="reasoning-content">
          <pre class="reasoning-text">{props.content}</pre>
        </div>
      </Show>
    </div>
  );
};
