import { describe, it, expect } from "vitest";
import { renderMarkdownToHtml } from "./MarkdownRenderer";

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
});
