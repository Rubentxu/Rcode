import { type Component, createSignal } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";

interface ReasoningBlockProps {
  content: string;
}

/**
 * Renders a reasoning block with Material Design 3 styling.
 * Shows agent reasoning with monospace font and secondary color accents.
 */
export const ReasoningBlock: Component<ReasoningBlockProps> = (props) => {
  const [isExpanded, setIsExpanded] = createSignal(false);

  return (
    <div data-part="reasoning" class="bg-surface-container-low border-l-2 border-secondary/30 p-4 rounded-lg">
      <div
        class="flex items-center gap-2 mb-3 cursor-pointer"
        onClick={() => setIsExpanded(!isExpanded())}
      >
        <span class="material-symbols-outlined text-secondary text-sm" style="font-variation-settings: 'FILL' 1;">memory</span>
        <span class="text-[11px] font-mono uppercase tracking-widest text-secondary/70">Agent Reasoning</span>
        <span class="material-symbols-outlined text-outline text-xs ml-auto">
          {isExpanded() ? "expand_less" : "expand_more"}
        </span>
      </div>

      <div
        class={`font-mono text-sm text-on-surface-variant/80 space-y-1 overflow-hidden transition-all duration-300 ${
          isExpanded() ? "max-h-[500px] opacity-100" : "max-h-0 opacity-0"
        }`}
      >
        <MarkdownRenderer content={props.content} />
      </div>

      {!isExpanded() && (
        <div class="text-xs text-outline font-mono truncate">
          Click to expand reasoning...
        </div>
      )}
    </div>
  );
};
