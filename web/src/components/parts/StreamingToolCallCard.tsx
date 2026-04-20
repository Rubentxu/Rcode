import { type Component, Show } from "solid-js";

interface StreamingToolCallCardProps {
  id: string;
  name: string;
  arguments_delta: string;
  status: "running" | "completed";
  // MEDIUM 2: MCP source badge - flows through streaming path
  source?: string;
}

/**
 * Streaming tool call card - inline icon + name + status + optional MCP badge.
 */
export const StreamingToolCallCard: Component<StreamingToolCallCardProps> = (props) => {
  return (
    <div
      data-component="streaming-tool-call-card"
      data-status={props.status}
      data-source={props.source}
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

      {/* MEDIUM 2: MCP source badge - shows server name when tool is from MCP */}
      <Show when={props.source}>
        <span
          class="px-1.5 py-0.5 rounded text-[10px] font-medium bg-violet-200/50 text-violet-700"
          title={`Source: ${props.source}`}
        >
          {props.source}
        </span>
      </Show>
    </div>
  );
};
