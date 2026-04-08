import { type Component, createSignal } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";

interface ReasoningBlockProps {
  content: string;
}

export const ReasoningBlock: Component<ReasoningBlockProps> = (props) => {
  const [isExpanded, setIsExpanded] = createSignal(false);

  return (
    <div data-part="reasoning" class="reasoning-block">
      <details open={isExpanded()} onToggle={(e) => setIsExpanded((e.target as HTMLDetailsElement).open)}>
        <summary class="reasoning-summary">
          <span class="reasoning-icon">{isExpanded() ? "▼" : "▶"}</span>
          <span class="reasoning-label">Reasoning</span>
        </summary>
        <div class="reasoning-content">
          <MarkdownRenderer content={props.content} />
        </div>
      </details>
    </div>
  );
};
