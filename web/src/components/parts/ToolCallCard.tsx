import { type Component, createSignal, Show } from "solid-js";

interface ToolCallCardProps {
  id: string;
  name: string;
  arguments: unknown;
  status?: "pending" | "running" | "success" | "error";
}

/**
 * Renders a tool call card with Material Design 3 styling.
 * Shows tool name with status indicator.
 */
export const ToolCallCard: Component<ToolCallCardProps> = (props) => {
  const [isExpanded, setIsExpanded] = createSignal(false);

  const formattedArgs = () => {
    try {
      return JSON.stringify(props.arguments, null, 2);
    } catch {
      return String(props.arguments);
    }
  };

  const status = () => props.status || "pending";

  const statusIcon = () => {
    switch (status()) {
      case "success":
        return "check_circle";
      case "error":
        return "error";
      case "running":
        return "hourglass_empty";
      default:
        return "travel_explore";
    }
  };

  const statusColor = () => {
    switch (status()) {
      case "success":
        return "text-secondary";
      case "error":
        return "text-error";
      case "running":
        return "text-tertiary";
      default:
        return "text-primary";
    }
  };

  return (
    <div data-part="tool_call" class="flex items-center gap-3 bg-surface-container-lowest border border-outline-variant/10 py-2 px-4 rounded-full w-fit">
      <span class={`material-symbols-outlined text-primary text-[18px]`}>
        travel_explore
      </span>
      <span class="text-xs font-semibold text-outline">
        Used tool: <span class={`font-medium text-primary`}>{props.name}</span>
      </span>

      <Show when={status() === "running"}>
        <div class="h-3 w-[1px] bg-outline-variant/20"></div>
        <div class="flex items-center gap-1.5">
          <span class="material-symbols-outlined text-tertiary text-[16px] animate-spin">progress_activity</span>
          <span class="text-[10px] font-bold text-tertiary uppercase tracking-tighter">Running</span>
        </div>
      </Show>

      <Show when={status() === "success"}>
        <div class="h-3 w-[1px] bg-outline-variant/20"></div>
        <div class="flex items-center gap-1.5">
          <span class="material-symbols-outlined text-secondary text-[16px]" style="font-variation-settings: 'FILL' 1;">check_circle</span>
          <span class="text-[10px] font-bold text-secondary uppercase tracking-tighter">Success</span>
        </div>
      </Show>

      <Show when={status() === "error"}>
        <div class="h-3 w-[1px] bg-outline-variant/20"></div>
        <div class="flex items-center gap-1.5">
          <span class="material-symbols-outlined text-error text-[16px]" style="font-variation-settings: 'FILL' 1;">error</span>
          <span class="text-[10px] font-bold text-error uppercase tracking-tighter">Failed</span>
        </div>
      </Show>

      {/* Expandable args */}
      <button
        onClick={() => setIsExpanded(!isExpanded())}
        class="ml-auto p-1 hover:bg-surface-container-high rounded transition-colors"
      >
        <span class="material-symbols-outlined text-outline text-sm">
          {isExpanded() ? "expand_less" : "expand_more"}
        </span>
      </button>

      <Show when={isExpanded()}>
        <div class="absolute top-full left-0 mt-2 bg-surface-container border border-outline-variant/20 rounded-xl p-4 shadow-2xl z-50 max-w-lg">
          <div class="text-xs font-mono text-outline mb-2 uppercase tracking-wider">Arguments</div>
          <pre class="text-xs text-on-surface-variant overflow-auto max-h-64">
            <code>{formattedArgs()}</code>
          </pre>
        </div>
      </Show>
    </div>
  );
};
