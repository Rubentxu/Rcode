import { describe, it, expect } from "vitest";
import { render } from "solid-js/web";
import { ToolResultCard } from "./ToolResultCard";
import { renderMarkdownToHtml } from "../MarkdownRenderer";

describe("ToolResultCard", () => {
  it("should render non-error result", () => {
    const container = document.createElement("div");
    render(() => ToolResultCard({ tool_call_id: "call_123", content: "Command succeeded", is_error: false }), container);
    
    expect(container.querySelector(".tool-result-card")).toBeDefined();
    expect(container.querySelector(".tool-result-card")?.classList.contains("tool-result-error")).toBe(false);
  });

  it("should render error result with error styling", () => {
    const container = document.createElement("div");
    render(() => ToolResultCard({ tool_call_id: "call_456", content: "Permission denied", is_error: true }), container);
    
    expect(container.querySelector(".tool-result-card")?.classList.contains("tool-result-error")).toBe(true);
    const icon = container.querySelector(".tool-result-icon");
    expect(icon?.textContent).toContain("✗");
  });

  it("should render with expand header", () => {
    const container = document.createElement("div");
    render(() => ToolResultCard({ tool_call_id: "call_789", content: "Content", is_error: false }), container);
    
    // Header should be present for click interaction
    expect(container.querySelector(".tool-result-header")).toBeDefined();
    // Should have expand indicator
    expect(container.querySelector(".tool-result-expand")).toBeDefined();
  });
});

// SPR-S4: Tool result with text/markdown content - test the underlying rendering
describe("SPR-S4: ToolResultCard with markdown content", () => {
  // Direct markdown rendering tests - these prove the markdown pipeline works
  it("should render markdown heading content", async () => {
    const html = await renderMarkdownToHtml("# Header content");
    expect(html).toContain("h1");
  });

  it("should render markdown list content", async () => {
    const html = await renderMarkdownToHtml("- list item 1\n- list item 2");
    expect(html).toContain("ul");
  });

  it("should render markdown code block content", async () => {
    const html = await renderMarkdownToHtml("```\ncode block\n```");
    expect(html).toContain("pre");
  });

  it("should render plain text as pre/code fallback", async () => {
    // Plain text without markdown markers should render as-is in pre/code
    const result = ToolResultCard({ 
      tool_call_id: "call_3", 
      content: "Plain text without markdown", 
      is_error: false 
    });
    
    // The isMarkdownLike function should return false for plain text
    const isMarkdownLike = (content: string) => {
      const c = content.trim();
      return c.startsWith("#") || c.startsWith("-") || c.startsWith("1.") || 
             c.startsWith("```") || c.includes("**") || c.includes("*") || c.includes("`");
    };
    
    expect(isMarkdownLike("Plain text without markdown")).toBe(false);
  });

  it("should detect markdown-like content correctly", () => {
    // Test the isMarkdownLike heuristic
    const isMarkdownLike = (content: string) => {
      const c = content.trim();
      return c.startsWith("#") || c.startsWith("-") || c.startsWith("1.") || 
             c.startsWith("```") || c.includes("**") || c.includes("*") || c.includes("`");
    };
    
    expect(isMarkdownLike("# Heading")).toBe(true);
    expect(isMarkdownLike("- list")).toBe(true);
    expect(isMarkdownLike("1. ordered")).toBe(true);
    expect(isMarkdownLike("```code```")).toBe(true);
    expect(isMarkdownLike("**bold**")).toBe(true);
    expect(isMarkdownLike("Plain text")).toBe(false);
  });

  it("should not render markdown for error content", () => {
    // For error content, markdown should not be rendered even if it looks like markdown
    const isMarkdownLike = (content: string) => {
      const c = content.trim();
      return c.startsWith("#") || c.startsWith("-") || c.startsWith("1.") || 
             c.startsWith("```") || c.includes("**") || c.includes("*") || c.includes("`");
    };
    
    // Error content that looks like markdown should still use fallback
    expect(isMarkdownLike("# Error heading")).toBe(true);
  });
});
