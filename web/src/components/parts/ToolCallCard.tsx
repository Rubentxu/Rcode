import { type Component, Show } from "solid-js";

interface ToolCallCardProps {
  id: string;
  name: string;
  arguments: unknown;
  status?: "pending" | "running" | "success" | "error";
  source?: string;
}

/**
 * Renders a tool call card with inline icon + name + status indicator.
 * UI/UX: Enhanced as a block element within process layer.
 */
export const ToolCallCard: Component<ToolCallCardProps> = (props) => {
  const status = () => props.status || "pending";

  return (
    <div data-part="tool_call" class="tool-call-card">
      <div class="tool-call-card-header">
        <span class="material-symbols-outlined text-primary" style={{ "font-size": "16px" }}>
          travel_explore
        </span>
        <span class="tool-call-name">{props.name}</span>

        <Show when={props.source?.startsWith("mcp:")}>
          <span class="tool-call-source">
            🔧 MCP: {props.source!.replace("mcp:", "")}
          </span>
        </Show>

        <div class="tool-call-status">
          <Show when={status() === "running"}>
            <span class="material-symbols-outlined text-tertiary animate-spin" style={{ "font-size": "14px" }}>progress_activity</span>
          </Show>

          <Show when={status() === "success"}>
            <span class="material-symbols-outlined text-secondary" style={{ "font-size": "14px", 'font-variation-settings': "'FILL' 1" }}>check_circle</span>
          </Show>

          <Show when={status() === "error"}>
            <span class="material-symbols-outlined text-error" style={{ "font-size": "14px", 'font-variation-settings': "'FILL' 1" }}>error</span>
          </Show>
        </div>
      </div>
    </div>
  );
};
