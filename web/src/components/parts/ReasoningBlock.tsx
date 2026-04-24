import { type Component, createSignal, Show } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";

interface ReasoningBlockProps {
  content: string;
}

/**
 * Renders a reasoning block with enhanced visual hierarchy.
 * Shows agent reasoning with monospace font and secondary color accents.
 *
 * T1.1: Uses <Show when={isExpanded()}> instead of CSS hiding to prevent
 * unified() pipeline from running on collapsed reasoning blocks.
 *
 * UI/UX: Now has clearer visual separation as a "process layer" element.
 */
export const ReasoningBlock: Component<ReasoningBlockProps> = (props) => {
  const [isExpanded, setIsExpanded] = createSignal(false);

  return (
    <div data-part="reasoning" class="reasoning-block">
      <div
        class="reasoning-block-header"
        onClick={() => setIsExpanded(!isExpanded())}
        role="button"
        aria-expanded={isExpanded()}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            setIsExpanded(!isExpanded());
          }
        }}
      >
        <span class="material-symbols-outlined text-secondary" style="font-size: 14px; font-variation-settings: 'FILL' 1;">psychology</span>
        <span class="reasoning-block-label">Reasoning</span>
        <Show when={isExpanded()} fallback={
          <span class="reasoning-block-preview text-xs text-outline font-mono truncate">
            Click to expand...
          </span>
        }>
          <span class="material-symbols-outlined text-outline text-sm ml-auto">
            expand_less
          </span>
        </Show>
        {!isExpanded() && (
          <span class="material-symbols-outlined text-outline text-sm ml-auto">
            expand_more
          </span>
        )}
      </div>

      {/*
        T1.1: <Show> prevents the MarkdownRenderer from being in the DOM at all
        when collapsed, so unified() pipeline never runs on hidden reasoning blocks.
      */}
      <Show when={isExpanded()}>
        <div class="reasoning-content font-mono text-sm text-on-surface-variant/80 space-y-1">
          <MarkdownRenderer content={props.content} />
        </div>
      </Show>
    </div>
  );
};
