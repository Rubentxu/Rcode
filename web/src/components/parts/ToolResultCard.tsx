import { type Component, createSignal, Show } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";

interface ToolResultCardProps {
  tool_call_id: string;
  content: string;
  is_error: boolean;
}

/**
 * Renders a tool result card with Material Design 3 styling.
 * Shows success/error status indicator.
 */
export const ToolResultCard: Component<ToolResultCardProps> = (props) => {
  const [isExpanded, setIsExpanded] = createSignal(false);

  // Check if content looks like markdown (simple heuristic)
  const isMarkdownLike = () => {
    const content = props.content.trim();
    return (
      content.startsWith("#") ||
      content.startsWith("-") ||
      content.startsWith("1.") ||
      content.startsWith("```") ||
      content.startsWith(">") ||
      content.includes("**") ||
      content.includes("*") ||
      content.includes("`")
    );
  };

  return (
    <div
      data-part="tool_result"
      class={`tool-result-card overflow-hidden rounded-lg ${
        props.is_error
          ? "bg-error-container/10"
          : "bg-bg-tertiary"
      }`}
    >
      <div
        class="flex items-center gap-3 p-3 cursor-pointer hover:bg-surface-container-high/50 transition-colors"
        onClick={() => setIsExpanded(!isExpanded())}
      >
        <span
          class={`material-symbols-outlined text-[16px] ${
            props.is_error ? "text-error" : "text-secondary"
          }`}
          style={props.is_error ? "" : "font-variation-settings: 'FILL' 1;"}
        >
          {props.is_error ? "error" : "check_circle"}
        </span>

        <span class={`text-xs font-semibold ${props.is_error ? "text-error" : "text-secondary"}`}>
          {props.is_error ? "Error" : "Result"}
        </span>

        <span class="material-symbols-outlined text-outline text-sm ml-auto">
          {isExpanded() ? "expand_less" : "expand_more"}
        </span>
      </div>

      <Show when={isExpanded()}>
        <div
          class={`p-4 border-t ${
            props.is_error ? "border-error/20" : "border-outline-variant/10"
          }`}
        >
          <Show
            when={isMarkdownLike() && !props.is_error}
            fallback={
              <pre class="text-xs font-mono text-on-surface-variant overflow-auto max-h-64 bg-surface-container-lowest p-3 rounded">
                <code>{props.content}</code>
              </pre>
            }
          >
            <div class="text-sm text-on-surface-variant">
              <MarkdownRenderer content={props.content} />
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};
