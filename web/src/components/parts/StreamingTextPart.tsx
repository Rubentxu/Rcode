import { type Component, createMemo, createSignal, createEffect, For, Show, onCleanup } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";
import { marked } from "marked";

// Configure marked for streaming use (sync, no async hooks)
marked.setOptions({
  gfm: true,
  breaks: false,
});

interface StreamingTextPartProps {
  content: string;
  throttleMs?: number;
}

/**
 * Fast synchronous markdown-to-HTML for streaming.
 * Uses `marked` (sync, ~0.5ms) instead of `unified()` (async, 50-200ms).
 * This is the key to achieving a smooth typewriter effect.
 */
function renderMarkdownSync(markdown: string): string {
  if (!markdown?.trim()) return "";
  try {
    return marked.parse(markdown) as string;
  } catch {
    // Fallback: escape and preserve line breaks
    return markdown
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\n/g, "<br/>");
  }
}

/**
 * Ultra-fast plain text renderer for the active streaming block.
 * No markdown pipeline at all — just HTML escape + inline code + line breaks.
 * Runs in <0.01ms. This is what makes tokens appear instantly.
 */
function renderStreamingPlainText(text: string): string {
  if (!text) return "";

  let escaped = text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

  const result: string[] = [];
  let i = 0;
  while (i < escaped.length) {
    // Triple backticks — code fence marker, keep as-is
    if (escaped.slice(i, i + 3) === "```") {
      result.push(escaped.slice(i, i + 3));
      i += 3;
      continue;
    }
    // Inline code (single backtick, not triple)
    if (escaped[i] === "`" && escaped[i + 1] !== "`") {
      const closeIndex = escaped.indexOf("`", i + 1);
      if (closeIndex !== -1) {
        result.push(`<code>${escaped.slice(i + 1, closeIndex)}</code>`);
        i = closeIndex + 1;
        continue;
      }
    }
    // Bold (**text**)
    if (escaped.slice(i, i + 2) === "**") {
      const closeIndex = escaped.indexOf("**", i + 2);
      if (closeIndex !== -1) {
        result.push(`<strong>${escaped.slice(i + 2, closeIndex)}</strong>`);
        i = closeIndex + 2;
        continue;
      }
    }
    // Italic (*text*)
    if (escaped[i] === "*" && escaped[i + 1] !== "*" && escaped[i - 1] !== "*") {
      const closeIndex = escaped.indexOf("*", i + 1);
      if (closeIndex !== -1 && escaped[closeIndex + 1] !== "*") {
        result.push(`<em>${escaped.slice(i + 1, closeIndex)}</em>`);
        i = closeIndex + 1;
        continue;
      }
    }
    // Line breaks
    if (escaped[i] === "\n") {
      result.push("<br/>");
    } else {
      result.push(escaped[i]);
    }
    i++;
  }

  return result.join("");
}

/**
 * Splits content into markdown blocks by double newlines.
 */
function splitMarkdownBlocks(content: string): string[] {
  if (!content) return [];
  return content.split(/(?:\r?\n){2,}/);
}

/**
 * Checks if content ends with an unclosed code fence.
 */
function hasUnclosedCodeFence(content: string): boolean {
  const fenceRegex = /```[\w]*$/;
  return fenceRegex.test(content);
}

export const StreamingTextPart: Component<StreamingTextPartProps> = (props) => {
  // Use requestAnimationFrame for smooth per-frame updates
  const [displayContent, setDisplayContent] = createSignal(props.content);
  let rafId: number | null = null;
  let pendingContent: string | null = null;

  // RAF-based batching: coalesces multiple props.content changes into one render per frame
  createEffect(() => {
    pendingContent = props.content;
    if (rafId !== null) return; // already scheduled

    rafId = requestAnimationFrame(() => {
      rafId = null;
      if (pendingContent !== null) {
        setDisplayContent(pendingContent);
        pendingContent = null;
      }
    });
  });

  onCleanup(() => {
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
  });

  // Split into blocks for block-level rendering
  const blocks = createMemo(() => splitMarkdownBlocks(displayContent()));

  // Check for incomplete code fence in the active block
  const hasIncompleteFence = createMemo(() => {
    const content = displayContent();
    return hasUnclosedCodeFence(content);
  });

  // All blocks except the last are "completed" — safe to render with full markdown
  const completedBlocks = createMemo(() => {
    const allBlocks = blocks();
    if (allBlocks.length <= 1) return [];
    return allBlocks.slice(0, -1);
  });

  // The active (last) block is still streaming — use fast plain text
  const activeBlock = createMemo(() => {
    const allBlocks = blocks();
    return allBlocks[allBlocks.length - 1] ?? "";
  });

  return (
    <div data-component="streaming-text-part" class="text-part">
      {/* Completed blocks: sync marked renderer (~0.5ms each) */}
      <For each={completedBlocks()}>
        {(block) => (
          <div
            data-component="completed-block"
            class="completed-block"
            innerHTML={renderMarkdownSync(block)}
          />
        )}
      </For>

      {/* Active block: ultra-fast plain text (<0.01ms) for instant per-token updates */}
      <div data-component="active-block" class="active-block">
        <Show
          when={!hasIncompleteFence()}
          fallback={<pre class="incomplete-code">{activeBlock()}</pre>}
        >
          <span
            class="streaming-text"
            innerHTML={renderStreamingPlainText(activeBlock())}
          />
          {/* Blinking cursor at the end of streaming text */}
          <span class="streaming-cursor" aria-hidden="true" />
        </Show>
      </div>
    </div>
  );
};
