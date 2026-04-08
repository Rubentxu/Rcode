import { type Component, Show, createSignal } from "solid-js";

interface StreamingToolCallCardProps {
  id: string;
  name: string;
  arguments_delta: string;
  status: "running" | "completed";
}

/**
 * Streaming tool call card that shows progress during tool execution.
 * Spinner → arguments → completed state.
 */
export const StreamingToolCallCard: Component<StreamingToolCallCardProps> = (props) => {
  const [expanded, setExpanded] = createSignal(false);
  
  const handleToggle = () => {
    setExpanded(!expanded());
  };
  
  return (
    <div 
      data-component="streaming-tool-call-card"
      data-status={props.status}
      class="tool-call-card"
    >
      <div data-component="tool-call-header" class="tool-call-header">
        <Show
          when={props.status === "running"}
          fallback={
            <span data-component="tool-status-icon" class="status-complete">✓</span>
          }
        >
          <span data-component="tool-status-icon" class="status-running">
            <svg class="spinner" viewBox="0 0 24 24" width="16" height="16">
              <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2" fill="none" stroke-dasharray="60" stroke-dashoffset="20" />
            </svg>
          </span>
        </Show>
        
        <span data-component="tool-name" class="tool-name">
          {props.name}
        </span>
        
        <button 
          data-component="toggle-expand"
          class="toggle-expand"
          onClick={handleToggle}
          type="button"
        >
          {expanded() ? "▼" : "▶"}
        </button>
      </div>
      
      <Show when={expanded() || props.arguments_delta}>
        <div data-component="tool-call-args" class="tool-call-args">
          <Show when={props.arguments_delta}>
            <pre data-component="arguments-preview">{props.arguments_delta}</pre>
          </Show>
        </div>
      </Show>
    </div>
  );
};
