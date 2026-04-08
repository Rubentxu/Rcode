import { type Component, createResource, createEffect } from "solid-js";
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkRehype from "remark-rehype";
import rehypeSanitize from "rehype-sanitize";
import rehypePrettyCode from "rehype-pretty-code";
import rehypeStringify from "rehype-stringify";

interface MarkdownRendererProps {
  content: string;
}

async function processMarkdown(content: string): Promise<string> {
  if (!content?.trim()) {
    return "";
  }

  try {
    const result = await unified()
      .use(remarkParse)
      .use(remarkGfm)
      .use(remarkRehype)
      .use(rehypeSanitize)
      .use(rehypePrettyCode, {
        theme: "github-dark",
        keepBackground: false,
      })
      .use(rehypeStringify)
      .process(content);

    return String(result);
  } catch (error) {
    console.error("Markdown rendering error:", error);
    return `<pre>${content.replace(/</g, "&lt;").replace(/>/g, "&gt;")}</pre>`;
  }
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

  try {
    const result = await unified()
      .use(remarkParse)
      .use(remarkGfm)
      .use(remarkRehype)
      .use(rehypeSanitize)
      .use(rehypePrettyCode, {
        theme: "github-dark",
        keepBackground: false,
      })
      .use(rehypeStringify)
      .process(content);

    return String(result);
  } catch (error) {
    console.error("Markdown rendering error:", error);
    return `<pre>${content.replace(/</g, "&lt;").replace(/>/g, "&gt;")}</pre>`;
  }
}
