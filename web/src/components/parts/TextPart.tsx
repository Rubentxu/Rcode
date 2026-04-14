import { type Component } from "solid-js";
import { marked } from "marked";
import { unified } from "unified";
import rehypeParse from "rehype-parse";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import rehypeStringify from "rehype-stringify";

// Configure marked for consistent rendering
marked.setOptions({
  gfm: true,
  breaks: false,
});

// GFM-compatible sanitize schema - preserves code blocks, tables, links, etc.
// SEC-3: Preserves GFM elements; SEC-4: Strips dangerous elements and attributes
const gfmSanitizeSchema = {
  ...defaultSchema,
  tagNames: [
    // Block elements
    "p", "br", "hr",
    // Headings
    "h1", "h2", "h3", "h4", "h5", "h6",
    // Lists
    "ul", "ol", "li",
    // Code
    "code", "pre", "blockquote",
    // Tables (GFM)
    "table", "thead", "tbody", "tfoot", "tr", "th", "td",
    // Inline
    "strong", "em", "del", "span", "a",
    // Images
    "img",
  ],
  attributes: {
    ...defaultSchema.attributes,
    "*": [...(defaultSchema.attributes?.["*"] ?? []), "class", "id"],
    a: [...(defaultSchema.attributes?.["a"] ?? []), "href", "title", "target", "rel"],
    img: ["src", "alt", "title", "width", "height", "class"],
    code: ["class"],
    pre: ["class"],
    td: ["align"],
    th: ["align"],
  },
  // Strip all event handlers and javascript: URLs
  strip: ["script", "iframe", "object", "embed", "form", "input", "button", "select", "textarea"],
};

/**
 * Sanitizes HTML string through rehype-sanitize pipeline.
 * Uses processSync for synchronous processing.
 */
function sanitizeHtml(html: string): string {
  const processor = unified()
    .use(rehypeParse)
    .use(rehypeSanitize, gfmSanitizeSchema)
    .use(rehypeStringify);
  
  const result = processor.processSync(html);
  return String(result);
}

/**
 * Synchronous markdown-to-HTML renderer.
 * Uses `marked` (sync, ~0.5ms) instead of the async Web Worker pipeline.
 * This eliminates the render gap that caused text to "disappear" when
 * the streaming draft was committed and the committed message used
 * the async MarkdownRenderer (Web Worker) which showed an empty div
 * while processing.
 * 
 * SEC-1: All HTML produced by marked.parse() is sanitized via rehype-sanitize
 * before being assigned to innerHTML to prevent XSS attacks.
 */
function renderMarkdownSync(markdown: string): string {
  if (!markdown?.trim()) return "";
  try {
    const rawHtml = marked.parse(markdown) as string;
    // SEC-1: Sanitize before innerHTML assignment
    return sanitizeHtml(rawHtml);
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
