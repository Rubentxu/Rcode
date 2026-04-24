import { type Component, createSignal, Show, createMemo } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";

interface ToolResultCardProps {
  tool_call_id: string;
  content: string;
  is_error: boolean;
  truncated?: boolean;
}

const PREVIEW_MAX_CHARS = 150;
const EXPAND_THRESHOLD_CHARS = 200;

/**
 * Renders a tool result card with intelligent expand/collapse behavior.
 * Structural optimization: all three states are compact.
 *
 * UX decisions:
 * - Empty/null content → minimal inline indicator, NO card, NO expand
 * - Short content (≤200 chars) → compact single-block, NO expand affordance
 * - Long content → tight header + short preview + expand affordance
 * - Errors → always expandable to show full error details
 */
export const ToolResultCard: Component<ToolResultCardProps> = (props) => {
  const [isExpanded, setIsExpanded] = createSignal(false);

  // Normalize content
  const normalizedContent = createMemo(() => {
    const c = props.content ?? "";
    return c.trim();
  });

  const isEmpty = createMemo(() => normalizedContent().length === 0);

  const needsExpand = createMemo(() => {
    if (props.is_error) return true; // Errors should always be expandable
    return normalizedContent().length > EXPAND_THRESHOLD_CHARS;
  });

  const preview = createMemo(() => {
    const content = normalizedContent();
    if (content.length <= PREVIEW_MAX_CHARS) return content;
    return content.slice(0, PREVIEW_MAX_CHARS) + "…";
  });

  // Check if content looks like markdown (simple heuristic)
  const isMarkdownLike = () => {
    const content = normalizedContent();
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

  // EMPTY STATE: Ultra-compact inline indicator - NO card, NO border, just inline
  if (isEmpty()) {
    return (
      <span
        data-part="tool_result"
        class="tool-result-inline-indicator"
      >
        <span
          class={`material-symbols-outlined ${props.is_error ? "text-error" : "text-secondary"}`}
          style={{ "font-size": "12px", ...(props.is_error ? {} : { "font-variation-settings": "'FILL' 1" }) }}
        >
          {props.is_error ? "error" : "check_circle"}
        </span>
        <span class={`tool-result-inline-label ${props.is_error ? "text-error" : "text-secondary"}`}>
          {props.is_error ? "Error" : "No result"}
        </span>
      </span>
    );
  }

  // SHORT CONTENT: Compact single-block, no expand affordance
  if (!needsExpand()) {
    return (
      <div
        data-part="tool_result"
        class={`tool-result-card tool-result-card--compact ${
          props.is_error ? "tool-result-card--error" : ""
        }`}
      >
        <div class="tool-result-header tool-result-header--compact">
          <span
            class={`material-symbols-outlined ${props.is_error ? "text-error" : "text-secondary"}`}
            style={{ "font-size": "12px", ...(props.is_error ? {} : { "font-variation-settings": "'FILL' 1" }) }}
          >
            {props.is_error ? "error" : "check_circle"}
          </span>
          <span class={`tool-result-label ${props.is_error ? "text-error" : "text-secondary"}`}>
            {props.is_error ? "Error" : "Result"}
          </span>
          <Show when={props.truncated}>
            <span class="tool-result-badge">⚠</span>
          </Show>
        </div>
        <div class="tool-result-content tool-result-content--compact">
          <Show
            when={isMarkdownLike() && !props.is_error}
            fallback={
              <pre class="tool-result-code tool-result-code--inline">{normalizedContent()}</pre>
            }
          >
            <div class="tool-result-markdown text-sm text-on-surface-variant">
              <MarkdownRenderer content={normalizedContent()} />
            </div>
          </Show>
        </div>
      </div>
    );
  }

  // LONG CONTENT: Tight header + compact preview + expand affordance
  return (
    <div
      data-part="tool_result"
      class={`tool-result-card tool-result-card--expandable ${
        isExpanded() ? "tool-result-card--expanded" : ""
      } ${props.is_error ? "tool-result-card--error" : ""}`}
    >
      <div
        class="tool-result-header tool-result-header--expandable"
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
        <span
          class={`material-symbols-outlined ${props.is_error ? "text-error" : "text-secondary"}`}
          style={{ "font-size": "12px", ...(props.is_error ? {} : { "font-variation-settings": "'FILL' 1" }) }}
        >
          {props.is_error ? "error" : "check_circle"}
        </span>
        <span class={`tool-result-label ${props.is_error ? "text-error" : "text-secondary"}`}>
          {props.is_error ? "Error" : "Result"}
        </span>
        <Show when={props.truncated}>
          <span class="tool-result-badge">⚠</span>
        </Show>
        <span class="tool-result-expand-hint">
          {isExpanded() ? "less" : "more"}
        </span>
        <span class="material-symbols-outlined text-outline" style={{ "font-size": "14px" }}>
          {isExpanded() ? "expand_less" : "expand_more"}
        </span>
      </div>

      <Show when={isExpanded()}>
        <div class="tool-result-content tool-result-content--expanded">
          <Show
            when={isMarkdownLike() && !props.is_error}
            fallback={
              <pre class="tool-result-code">{normalizedContent()}</pre>
            }
          >
            <div class="tool-result-markdown text-sm text-on-surface-variant">
              <MarkdownRenderer content={normalizedContent()} />
            </div>
          </Show>
        </div>
      </Show>

      {/* Preview shown when collapsed - compact with fade */}
      <Show when={!isExpanded()}>
        <div class="tool-result-preview tool-result-preview--compact">
          <pre class="tool-result-code tool-result-preview__code">{preview()}</pre>
        </div>
      </Show>
    </div>
  );
};
