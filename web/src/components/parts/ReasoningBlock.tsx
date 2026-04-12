import { type Component, createSignal, Show } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";

interface ReasoningBlockProps {
  content: string;
}

/**
 * Renders a reasoning block with Material Design 3 styling.
 * Shows agent reasoning with monospace font and secondary color accents.
 * 
 * T1.1: Uses <Show when={isExpanded()}> instead of CSS hiding to prevent
 * unified() pipeline from running on collapsed reasoning blocks.
 */
export const ReasoningBlock: Component<ReasoningBlockProps> = (props) => {
  const [isExpanded, setIsExpanded] = createSignal(false);

  return (
    <div data-part="reasoning" class="border-l border-secondary/50 px-3 py-2">
      <div
        class="flex items-center gap-2 cursor-pointer"
        onClick={() => setIsExpanded(!isExpanded())}
      >
        <span class="material-symbols-outlined text-secondary text-sm" style="font-variation-settings: 'FILL' 1;">memory</span>
        <span class="text-[11px] font-mono uppercase tracking-widest text-secondary/70">Reasoning</span>
        <span class="material-symbols-outlined text-outline text-xs ml-auto">
          {isExpanded() ? "expand_less" : "expand_more"}
        </span>
      </div>

      {/*
        T1.1: <Show> prevents the MarkdownRenderer from being in the DOM at all
        when collapsed, so unified() pipeline never runs on hidden reasoning blocks.
      */}
      <Show when={isExpanded()}>
        <div class="font-mono text-sm text-on-surface-variant/80 space-y-1">
          <MarkdownRenderer content={props.content} />
        </div>
      </Show>

      {!isExpanded() && (
        <div class="text-xs text-outline font-mono truncate">
          Click to expand...
        </div>
      )}
    </div>
  );
};
