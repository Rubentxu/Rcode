import { type Component, createSignal, Show } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";

interface ToolResultCardProps {
  tool_call_id: string;
  content: string;
  is_error: boolean;
}

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
      class={`tool-result-card ${props.is_error ? "tool-result-error" : ""}`}
    >
      <div 
        class="tool-result-header"
        onClick={() => setIsExpanded(!isExpanded())}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => e.key === "Enter" && setIsExpanded(!isExpanded())}
      >
        <span class="tool-result-icon">{props.is_error ? "✗" : "✓"}</span>
        <span class="tool-result-label">
          {props.is_error ? "Error" : "Result"}
        </span>
        <span class="tool-result-expand">{isExpanded() ? "▼" : "▶"}</span>
      </div>
      <Show when={isExpanded()}>
        <div class="tool-result-content">
          <Show 
            when={isMarkdownLike() && !props.is_error}
            fallback={<pre><code>{props.content}</code></pre>}
          >
            <MarkdownRenderer content={props.content} />
          </Show>
        </div>
      </Show>
    </div>
  );
};
