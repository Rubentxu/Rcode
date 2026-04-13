import { type Component } from "solid-js";
import { marked } from "marked";

// Configure marked for consistent rendering
marked.setOptions({
  gfm: true,
  breaks: false,
});

/**
 * Synchronous markdown-to-HTML renderer.
 * Uses `marked` (sync, ~0.5ms) instead of the async Web Worker pipeline.
 * This eliminates the render gap that caused text to "disappear" when
 * the streaming draft was committed and the committed message used
 * the async MarkdownRenderer (Web Worker) which showed an empty div
 * while processing.
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

interface TextPartProps {
  content: string;
}

export const TextPart: Component<TextPartProps> = (props) => {
  return (
    <div data-part="text" class="text-part" innerHTML={renderMarkdownSync(props.content)} />
  );
};
