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
 */
export const ToolCallCard: Component<ToolCallCardProps> = (props) => {
  const status = () => props.status || "pending";

  return (
    <div data-part="tool_call" class="inline-flex items-center gap-2 py-1">
      <span class={`material-symbols-outlined text-primary text-sm`} style={{ "font-size": "14px" }}>
        travel_explore
      </span>
      <span class="text-xs font-medium text-outline">
        {props.name}
      </span>

      <Show when={props.source?.startsWith("mcp:")}>
        <span class="px-1.5 py-0.5 text-[10px] font-semibold rounded bg-violet-200/50 text-violet-700">
          🔧 MCP: {props.source!.replace("mcp:", "")}
        </span>
      </Show>

      <Show when={status() === "running"}>
        <span class="material-symbols-outlined text-tertiary text-sm animate-spin" style={{ "font-size": "12px" }}>progress_activity</span>
      </Show>

      <Show when={status() === "success"}>
        <span class="material-symbols-outlined text-secondary" style={{ "font-size": "12px", 'font-variation-settings': "'FILL' 1" }}>check_circle</span>
      </Show>

      <Show when={status() === "error"}>
        <span class="material-symbols-outlined text-error" style={{ "font-size": "12px", 'font-variation-settings': "'FILL' 1" }}>error</span>
      </Show>
    </div>
  );
};
