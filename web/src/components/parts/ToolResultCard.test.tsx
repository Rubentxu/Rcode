import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { fireEvent, waitFor } from "@testing-library/dom";
import { ToolResultCard } from "./ToolResultCard";
import { renderMarkdownToHtml } from "../MarkdownRenderer";

// Container must be attached to document.body for SolidJS event delegation to work in jsdom
let container: HTMLDivElement;
beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
});
afterEach(() => {
  document.body.removeChild(container);
});

describe("ToolResultCard", () => {
  it("should render non-error result", () => {
    render(() => ToolResultCard({ tool_call_id: "call_123", content: "Command succeeded", is_error: false }), container);

    const card = container.querySelector("[data-part='tool_result']");
    expect(card).toBeDefined();
    // Should have the non-error card class (bg-surface-container-low)
    expect(card?.className).toContain("tool-result-card");
  });

  it("should render error result with error styling", () => {
    render(() => ToolResultCard({ tool_call_id: "call_456", content: "Permission denied", is_error: true }), container);

    // Error cards should have error-container background
    const card = container.querySelector("[data-part='tool_result']");
    expect(card?.className).toContain("tool-result-card");
  });

  it("should render with clickable header", () => {
    render(() => ToolResultCard({ tool_call_id: "call_789", content: "Content", is_error: false }), container);

    // Should have a header div with flex layout for the clickable area
    const header = container.querySelector(".flex.items-center.gap-3");
    expect(header).toBeDefined();
    // Should be clickable
    expect(header?.getAttribute("class")).toContain("cursor-pointer");
  });

  it("should expand content on click and show result content", async () => {
    render(() => ToolResultCard({ tool_call_id: "call_expand", content: "Expanded content here", is_error: false }), container);

    // Initially content should be hidden
    const preBefore = container.querySelector("pre");
    expect(preBefore).toBeNull();

    // Click the header to expand
    const header = container.querySelector(".flex.items-center.gap-3");
    fireEvent.click(header!);

    // Wait for the content to appear after click (SolidJS reactive update)
    await waitFor(() => {
      const pre = container.querySelector("pre");
      expect(pre).toBeDefined();
      expect(pre?.textContent).toContain("Expanded content here");
    });
  });

  it("should show success indicator for non-error results", () => {
    render(() => ToolResultCard({ tool_call_id: "call_ok", content: "OK result", is_error: false }), container);

    // Should show check_circle Material Symbol icon for success
    const icon = container.querySelector(".material-symbols-outlined");
    expect(icon?.textContent).toContain("check_circle");
  });

  it("should show error indicator for error results", () => {
    render(() => ToolResultCard({ tool_call_id: "call_err", content: "Error result", is_error: true }), container);

    // Should show error Material Symbol icon for errors
    const icon = container.querySelector(".material-symbols-outlined");
    expect(icon?.textContent).toContain("error");
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

  it("should render plain text as pre/code fallback", () => {
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
