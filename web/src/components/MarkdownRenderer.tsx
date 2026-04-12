import { type Component, createEffect, createResource } from "solid-js";
import "katex/dist/katex.min.css";

interface MarkdownRendererProps {
  content: string;
}

/**
 * T2.2: Markdown rendering via Web Worker to avoid blocking main thread
 * 
 * The worker handles the unified() pipeline, and the main thread
 * handles code block enhancement and mermaid diagram rendering (which needs DOM).
 */

// Lazy-initialized worker singleton
let markdownWorker: Worker | null = null;
let workerResolveMap: Map<string, (html: string) => void> = new Map();
let workerRejectMap: Map<string, (error: Error) => void> = new Map();
let workerIdCounter = 0;

function getMarkdownWorker(): Worker | null {
  // Check if Worker is available (not available in JSDOM test environment)
  if (typeof Worker === "undefined") {
    return null;
  }
  
  if (!markdownWorker) {
    // Using import.meta.url directly without type: 'module' for better Vite compatibility
    markdownWorker = new Worker(
      new URL("../workers/markdown.worker.ts", import.meta.url)
    );

    markdownWorker.onmessage = (event) => {
      const { id, html } = event.data;
      const resolve = workerResolveMap.get(id);
      if (resolve) {
        resolve(html);
        workerResolveMap.delete(id);
        workerRejectMap.delete(id);
      }
    };

    markdownWorker.onerror = (error) => {
      console.error("Markdown worker error:", error);
      // Reject all pending requests
      workerRejectMap.forEach((reject) => {
        reject(new Error("Worker error"));
      });
      workerResolveMap.clear();
      workerRejectMap.clear();
    };
  }
  return markdownWorker;
}

/**
 * Fallback markdown processor for test environments where Worker is not available.
 * Processes markdown synchronously without worker.
 */
async function processMarkdownFallback(content: string): Promise<string> {
  if (!content?.trim()) {
    return "";
  }

  try {
    // Dynamically import unified and process synchronously
    const { unified } = await import("unified");
    const remarkParse = (await import("remark-parse")).default;
    const remarkMath = (await import("remark-math")).default;
    const remarkGfm = (await import("remark-gfm")).default;
    const remarkRehype = (await import("remark-rehype")).default;
    const rehypeKatex = (await import("rehype-katex")).default;
    const rehypeSanitize = (await import("rehype-sanitize")).default;
    const rehypePrettyCode = (await import("rehype-pretty-code")).default;
    const rehypeStringify = (await import("rehype-stringify")).default;
    const { sanitizeSchema } = await import("../lib/sanitizeSchema");
    const { remarkMermaidPlugin } = await import("../lib/remarkMermaidPlugin");

    const processor = unified()
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

    const result = await processor.process(content);
    return String(result);
  } catch (error) {
    console.error("Markdown rendering error:", error);
    return `<pre>${content.replace(/</g, "&lt;").replace(/>/g, "&gt;")}</pre>`;
  }
}

async function processMarkdown(content: string): Promise<string> {
  if (!content?.trim()) {
    return "";
  }

  // Use fallback in test environments where Worker is not available
  const worker = getMarkdownWorker();
  if (!worker) {
    return processMarkdownFallback(content);
  }

  return new Promise((resolve, reject) => {
    const id = `md-${++workerIdCounter}`;

    workerResolveMap.set(id, resolve);
    workerRejectMap.set(id, reject);

    worker.postMessage({ id, content });

    // Timeout after 30 seconds
    setTimeout(() => {
      if (workerResolveMap.has(id)) {
        workerResolveMap.delete(id);
        workerRejectMap.delete(id);
        reject(new Error("Markdown processing timeout"));
      }
    }, 30000);
  });
}

let mermaidInitialized = false;
let mermaidModulePromise: Promise<typeof import("mermaid").default> | null = null;

async function loadMermaid() {
  if (!mermaidModulePromise) {
    mermaidModulePromise = import("mermaid").then((module) => {
      const mermaid = module.default;
      if (!mermaidInitialized) {
        mermaid.initialize({
          startOnLoad: false,
          theme: "neutral",
          securityLevel: "loose",
        });
        mermaidInitialized = true;
      }
      return mermaid;
    });
  }

  return mermaidModulePromise;
}

export async function enhanceMermaidDiagrams(container: ParentNode) {
  const mermaidBlocks = container.querySelectorAll<HTMLPreElement>(
    'pre.mermaid[data-mermaid-status="pending"]'
  );
  if (mermaidBlocks.length === 0) return;

  const mermaid = await loadMermaid();

  for (const block of Array.from(mermaidBlocks)) {
    const source = block.getAttribute("data-mermaid-source");
    if (source) {
      try {
        const id = `mermaid-${Math.random().toString(36).slice(2, 11)}`;
        const { svg } = await mermaid.render(id, source);
        block.innerHTML = svg;
        block.setAttribute("data-mermaid-status", "rendered");
      } catch (error) {
        console.error("Mermaid rendering error:", error);
        block.replaceChildren();
        const fallback = document.createElement("span");
        fallback.className = "mermaid-error";
        fallback.textContent = "Unable to render Mermaid diagram.";
        block.appendChild(fallback);
        block.setAttribute("data-mermaid-status", "error");
      }
    }
  }
}

export function resetMarkdownRendererTestState() {
  mermaidInitialized = false;
  mermaidModulePromise = null;
}

// Enhance code blocks with language badge and copy button
function enhanceCodeBlocks(container: Element) {
  // Find all figure elements (from rehype-pretty-code) that contain code
  const figures = container.querySelectorAll("figure[data-rehype-pretty-code-figure]");
  
  figures.forEach((figure) => {
    // Check if already enhanced
    if (figure.parentElement?.classList.contains("code-block")) {
      return;
    }
    
    const pre = figure.querySelector("pre[data-language]");
    if (!pre) return;
    
    const language = pre.getAttribute("data-language") || "text";
    
    // Create wrapper
    const wrapper = document.createElement("div");
    wrapper.className = "code-block";
    wrapper.setAttribute("data-language", language);
    
    // Create header
    const header = document.createElement("div");
    header.className = "code-block-header";
    
    // Language badge
    const langBadge = document.createElement("span");
    langBadge.className = "code-block-lang";
    langBadge.textContent = language;
    header.appendChild(langBadge);
    
    // Copy button
    const copyBtn = document.createElement("button");
    copyBtn.className = "code-block-copy";
    copyBtn.setAttribute("data-copy", "true");
    copyBtn.type = "button";
    copyBtn.textContent = "Copy";
    copyBtn.addEventListener("click", async (e) => {
      e.preventDefault();
      e.stopPropagation();
      
      const code = pre.querySelector("code");
      const text = code?.textContent || pre.textContent || "";
      
      try {
        await navigator.clipboard.writeText(text);
        copyBtn.textContent = "Copied!";
        copyBtn.classList.add("copied");
        setTimeout(() => {
          copyBtn.textContent = "Copy";
          copyBtn.classList.remove("copied");
        }, 2000);
      } catch (err) {
        console.error("Failed to copy:", err);
        copyBtn.textContent = "Error";
        setTimeout(() => {
          copyBtn.textContent = "Copy";
        }, 2000);
      }
    });
    header.appendChild(copyBtn);
    
    // Wrap the figure
    figure.parentNode?.insertBefore(wrapper, figure);
    wrapper.appendChild(header);
    wrapper.appendChild(figure);
  });
}

export const MarkdownRenderer: Component<MarkdownRendererProps> = (props) => {
  const [html] = createResource(() => props.content, processMarkdown);
  let containerRef: HTMLDivElement | undefined;

  // Enhance code blocks with copy functionality whenever html changes
  createEffect(() => {
    const content = html();
    if (content && containerRef) {
      containerRef.innerHTML = content;
      enhanceCodeBlocks(containerRef);
      enhanceMermaidDiagrams(containerRef);
    }
  });

  return (
    <div class="markdown-body">
      <div ref={containerRef} />
    </div>
  );
};

// Export the markdown processing function for testing (without enhancement)
export async function renderMarkdownToHtml(content: string): Promise<string> {
  if (!content || !content.trim()) {
    return "";
  }

  // Use the same worker-based processing as the renderer
  return processMarkdown(content);
}
