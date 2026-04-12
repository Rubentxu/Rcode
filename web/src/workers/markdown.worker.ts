/**
 * Markdown Worker - Offloads unified() markdown processing to a Web Worker
 * to prevent blocking the main thread during rendering of long conversations.
 * 
 * T2.1: Worker receives { id, content }, responds with { id, html }
 * The mermaid diagrams are processed as placeholders by remarkMermaidPlugin;
 * actual SVG rendering happens in the main thread via enhanceMermaidDiagrams.
 */

import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkMath from "remark-math";
import remarkGfm from "remark-gfm";
import remarkRehype from "remark-rehype";
import rehypeKatex from "rehype-katex";
import rehypeSanitize from "rehype-sanitize";
import rehypePrettyCode from "rehype-pretty-code";
import rehypeStringify from "rehype-stringify";
import { sanitizeSchema } from "../lib/sanitizeSchema";
import { remarkMermaidPlugin } from "../lib/remarkMermaidPlugin";

// Import mermaid types for documentation
import type {} from "mermaid";

interface WorkerMessage {
  id: string;
  content: string;
}

interface WorkerResponse {
  id: string;
  html: string;
}

function createMarkdownProcessor() {
  return unified()
    .use(remarkParse)
    .use(remarkMath)
    .use(remarkGfm)
    .use(remarkRehype)
    .use(rehypeKatex, { throwOnError: false, errorColor: "#cc0000" })
    .use(remarkMermaidPlugin)
    .use(rehypeSanitize, sanitizeSchema)
    .use(rehypePrettyCode, {
      theme: "github-dark",
      keepBackground: false,
    })
    .use(rehypeStringify);
}

async function processMarkdownInWorker(content: string): Promise<string> {
  if (!content?.trim()) {
    return "";
  }

  try {
    const result = await createMarkdownProcessor().process(content);
    return String(result);
  } catch (error) {
    console.error("Markdown rendering error in worker:", error);
    return `<pre>${content.replace(/</g, "&lt;").replace(/>/g, "&gt;")}</pre>`;
  }
}

// Handle messages from the main thread
self.onmessage = async (event: MessageEvent<WorkerMessage>) => {
  const { id, content } = event.data;

  try {
    const html = await processMarkdownInWorker(content);
    const response: WorkerResponse = { id, html };
    self.postMessage(response);
  } catch (error) {
    console.error("Worker error:", error);
    // Send back error HTML on failure
    const errorHtml = `<pre>${content?.replace(/</g, "&lt;").replace(/>/g, "&gt;") || ""}</pre>`;
    const response: WorkerResponse = { id, html: errorHtml };
    self.postMessage(response);
  }
};
