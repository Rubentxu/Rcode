import { type Component, Show } from "solid-js";

interface StreamingToolCallCardProps {
  id: string;
  name: string;
  arguments_delta: string;
  status: "running" | "completed";
}

/**
 * Streaming tool call card - inline icon + name + status.
 */
export const StreamingToolCallCard: Component<StreamingToolCallCardProps> = (props) => {
  return (
    <div 
      data-component="streaming-tool-call-card"
      data-status={props.status}
      class="inline-flex items-center gap-1.5"
    >
      <Show
        when={props.status === "running"}
        fallback={
          <span class="text-secondary" style={{ "font-size": "12px" }}>✓</span>
        }
      >
        <span class="text-tertiary animate-spin" style={{ "font-size": "12px" }}>
          <svg class="spinner" viewBox="0 0 24 24" width="12" height="12">
            <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2" fill="none" stroke-dasharray="60" stroke-dashoffset="20" />
          </svg>
        </span>
      </Show>
      
      <span class="text-xs font-medium text-outline">
        {props.name}
      </span>
    </div>
  );
};
