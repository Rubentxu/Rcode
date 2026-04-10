import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  enhanceMermaidDiagrams,
  MarkdownRenderer,
  renderMarkdownToHtml,
  resetMarkdownRendererTestState,
} from "./MarkdownRenderer";

const mermaidInitialize = vi.fn();
const mermaidRender = vi.fn();

vi.mock("mermaid", () => ({
  default: {
    initialize: mermaidInitialize,
    render: mermaidRender,
  },
}));

beforeEach(() => {
  cleanup();
  mermaidInitialize.mockReset();
  mermaidRender.mockReset();
  resetMarkdownRendererTestState();
});

afterEach(() => {
  cleanup();
});

describe("MarkdownRenderer - GFM Elements", () => {
  describe("headings", () => {
    it("should render h1 heading", async () => {
      const html = await renderMarkdownToHtml("# Title");
      expect(html).toContain("<h1");
      expect(html).toContain("Title");
    });

    it("should render h2 heading", async () => {
      const html = await renderMarkdownToHtml("## Subtitle");
      expect(html).toContain("<h2");
      expect(html).toContain("Subtitle");
    });

    it("should render h3-h6 headings", async () => {
      const html = await renderMarkdownToHtml("### H3\n#### H4\n##### H5\n###### H6");
      expect(html).toContain("<h3");
      expect(html).toContain("<h4");
      expect(html).toContain("<h5");
      expect(html).toContain("<h6");
    });
  });

  describe("text formatting", () => {
    it("should render bold text", async () => {
      const html = await renderMarkdownToHtml("**bold text**");
      expect(html).toContain("<strong");
    });

    it("should render italic text", async () => {
      const html = await renderMarkdownToHtml("*italic text*");
      expect(html).toContain("<em");
    });

    it("should render inline code", async () => {
      const html = await renderMarkdownToHtml("`inline code`");
      expect(html).toContain("<code");
    });
  });

  describe("lists", () => {
    it("should render unordered lists", async () => {
      const html = await renderMarkdownToHtml("- item 1\n- item 2\n- item 3");
      expect(html).toContain("<ul");
      expect(html).toContain("<li");
    });

    it("should render ordered lists", async () => {
      const html = await renderMarkdownToHtml("1. first\n2. second\n3. third");
      expect(html).toContain("<ol");
      expect(html).toContain("<li");
    });
  });

  describe("tables (GFM)", () => {
    it("should render tables", async () => {
      const html = await renderMarkdownToHtml(
        "| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |"
      );
      expect(html).toContain("<table");
      expect(html).toContain("<thead");
      expect(html).toContain("<tbody");
    });
  });

  describe("blockquotes", () => {
    it("should render blockquotes", async () => {
      const html = await renderMarkdownToHtml("> This is a quote");
      expect(html).toContain("<blockquote");
    });
  });

  describe("code blocks", () => {
    it("should render fenced code blocks with language", async () => {
      const html = await renderMarkdownToHtml("```rust\nfn main() {}\n```");
      expect(html).toContain("<code");
      expect(html).toContain("data-language=\"rust\"");
    });

    it("should render fenced code blocks without language as text", async () => {
      const html = await renderMarkdownToHtml("```\nplain code\n```");
      expect(html).toContain("<code");
    });
  });

  describe("links", () => {
    it("should render links", async () => {
      const html = await renderMarkdownToHtml("[Link text](https://example.com)");
      expect(html).toContain("<a");
      expect(html).toContain('href="https://example.com"');
    });
  });

  describe("empty and whitespace input", () => {
    it("should handle empty string without error", async () => {
      const html = await renderMarkdownToHtml("");
      expect(html).toBe("");
    });

    it("should handle whitespace-only input without error", async () => {
      const html = await renderMarkdownToHtml("   \n\n  \t  ");
      expect(html).toBe("");
    });

    it("should handle null-like content", async () => {
      const html = await renderMarkdownToHtml("   ");
      expect(html).toBe("");
    });
  });

  describe("sanitization", () => {
    it("should sanitize potentially harmful HTML", async () => {
      const html = await renderMarkdownToHtml("<script>alert('xss')</script>Regular text");
      // Should not contain script tags
      expect(html).not.toContain("<script>");
      expect(html).not.toContain("alert");
    });
  });

  describe("KaTeX", () => {
    it("should render inline math", async () => {
      const html = await renderMarkdownToHtml("Inline math $a^2 + b^2 = c^2$ works.");
      expect(html).toContain('class="katex"');
      expect(html).toContain('annotation encoding="application/x-tex">a^2 + b^2 = c^2</annotation>');
    });

    it("should render display math", async () => {
      const html = await renderMarkdownToHtml("$$\n\\int_0^1 x^2 dx\n$$");
      expect(html).toContain('class="katex-display"');
      expect(html).toContain("∫");
    });

    it("should keep invalid LaTeX as katex error output", async () => {
      const html = await renderMarkdownToHtml("$\\notacommand{$");
      expect(html).toContain('class="katex-error"');
    });
  });

  describe("Mermaid", () => {
    it("should render Mermaid fences as lazy placeholders", async () => {
      const html = await renderMarkdownToHtml("```mermaid\ngraph TD\n  A-->B\n```");

      const container = document.createElement("div");
      container.innerHTML = html;

      const block = container.querySelector("pre.mermaid");
      expect(block).not.toBeNull();
      expect(block?.getAttribute("data-mermaid-source")).toBe("graph TD\n  A-->B");
      expect(block?.getAttribute("data-mermaid-status")).toBe("pending");
    });

    it("should not load Mermaid when no diagram is present", async () => {
      const { container } = render(() => <MarkdownRenderer content="Regular paragraph only." />);

      await waitFor(() => {
        expect(container.querySelector("p")).not.toBeNull();
      });

      expect(mermaidInitialize).not.toHaveBeenCalled();
      expect(mermaidRender).not.toHaveBeenCalled();
    });

    it("should lazy load Mermaid only when a diagram is present", async () => {
      mermaidRender.mockResolvedValue({ svg: '<svg><text>diagram</text></svg>' });

      const { container } = render(() => (
        <MarkdownRenderer content={"```mermaid\ngraph TD\n  A-->B\n```"} />
      ));

      await waitFor(() => {
        expect(mermaidInitialize).toHaveBeenCalledTimes(1);
        expect(mermaidRender).toHaveBeenCalledTimes(1);
      });

      const block = container.querySelector("pre.mermaid");
      expect(block?.getAttribute("data-mermaid-status")).toBe("rendered");
      expect(block?.innerHTML).toContain("<svg>");
    });

    it("should show Mermaid fallback when render fails", async () => {
      mermaidRender.mockRejectedValue(new Error("invalid diagram"));

      const container = document.createElement("div");
      container.innerHTML =
        '<pre class="mermaid" data-mermaid-source="graph TD" data-mermaid-status="pending">graph TD</pre>';

      await enhanceMermaidDiagrams(container);

      const block = container.querySelector("pre.mermaid");
      expect(block?.getAttribute("data-mermaid-status")).toBe("error");
      expect(block?.textContent).toContain("Unable to render Mermaid diagram.");
    });
  });
});
