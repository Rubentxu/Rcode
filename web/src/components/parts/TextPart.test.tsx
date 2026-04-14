import { describe, it, expect } from "vitest";
import { TextPart } from "./TextPart";

describe("TextPart", () => {
  it("should render text part component", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "Hello, world!" });
    container.appendChild(result as Node);
    
    expect(container.querySelector(".text-part")).toBeDefined();
  });

  it("should handle empty string", () => {
    const result = TextPart({ content: "" });
    expect(result).toBeDefined();
  });

  // SEC-4: Script tags must be stripped
  it("should strip script tags from markdown content", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "<script>alert('xss')</script>Hello" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).not.toContain("<script>");
    expect(textPart?.innerHTML).not.toContain("alert");
  });

  // SEC-4: onerror handlers must be stripped (XSS payload)
  it("should strip onerror attribute from img tags (XSS payload)", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "<img src=x onerror=alert(1)>" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).not.toContain("onerror");
    expect(textPart?.innerHTML).not.toContain("alert");
  });

  // SEC-3: Code blocks must be preserved
  it("should preserve fenced code blocks", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "```js\nconsole.log('hello')\n```" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).toContain("<pre");
    expect(textPart?.innerHTML).toContain("<code");
    expect(textPart?.innerHTML).toContain("console.log");
  });

  // SEC-3: Inline code must be preserved
  it("should preserve inline code", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "Use `console.log()` for debugging" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).toContain("<code");
    expect(textPart?.innerHTML).toContain("console.log");
  });

  // SEC-3: Tables must be preserved
  it("should preserve GFM tables", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "| col1 | col2 |\n|------|------|\n| a    | b    |" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).toContain("<table");
    expect(textPart?.innerHTML).toContain("<thead");
    expect(textPart?.innerHTML).toContain("<tbody");
    expect(textPart?.innerHTML).toContain("<tr");
    expect(textPart?.innerHTML).toContain("<td");
  });

  // SEC-5: Safe links must be preserved
  it("should preserve safe http/https links", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "[click here](https://example.com)" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).toContain('href="https://example.com"');
    expect(textPart?.innerHTML).toContain("click here");
  });

  // SEC-4: javascript: links must be stripped
  it("should strip javascript: href values", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "[bad link](javascript:alert(1))" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    // The anchor should either have no href or href="#" - not javascript:
    const anchor = textPart?.querySelector("a");
    const href = anchor?.getAttribute("href") ?? "";
    expect(href).not.toContain("javascript:");
  });

  // SEC-3: Other GFM elements must be preserved
  it("should preserve blockquotes", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "> This is a quote" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).toContain("<blockquote");
  });

  it("should preserve headings", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "## Heading Two" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).toContain("<h2");
  });

  it("should preserve lists", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "- item 1\n- item 2" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).toContain("<ul");
    expect(textPart?.innerHTML).toContain("<li");
  });

  it("should preserve strong and em emphasis", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "**bold** and *italic*" });
    container.appendChild(result as Node);
    
    const textPart = container.querySelector(".text-part");
    expect(textPart?.innerHTML).toContain("<strong");
    expect(textPart?.innerHTML).toContain("<em");
  });
});
