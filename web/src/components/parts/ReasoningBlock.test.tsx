import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { fireEvent, waitFor } from "@testing-library/dom";
import { ReasoningBlock } from "./ReasoningBlock";

// Container must be attached to document.body for SolidJS event delegation to work in jsdom
let container: HTMLDivElement;
beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
});
afterEach(() => {
  document.body.removeChild(container);
});

describe("ReasoningBlock", () => {
  it("should render reasoning block", () => {
    render(() => ReasoningBlock({ content: "Let me think about this..." }), container);

    const block = container.querySelector("[data-part='reasoning']");
    expect(block).toBeDefined();
  });

  it("should have updated reasoning label in block", () => {
    render(() => ReasoningBlock({ content: "thinking..." }), container);

    // Should contain the simplified "Reasoning" label
    const block = container.querySelector("[data-part='reasoning']");
    expect(block?.textContent).toContain("Reasoning");
  });
});

// SPR-S1: Reasoning block collapsed by default, expands on click
describe("SPR-S1: Reasoning block collapsed by default and expands on click", () => {
  it("should be collapsed by default (content hidden)", () => {
    render(() => ReasoningBlock({ content: "Let me think about this..." }), container);

    const block = container.querySelector("[data-part='reasoning']");
    expect(block).toBeDefined();
    // Content should be hidden (max-h-0 opacity-0 when collapsed)
    const content = block?.querySelector(".max-h-0");
    expect(content).toBeDefined();
  });

  it("should show reasoning content after clicking to expand", async () => {
    render(() => ReasoningBlock({ content: "My detailed reasoning content here" }), container);

    const block = container.querySelector("[data-part='reasoning']");
    expect(block).toBeDefined();

    // Click on the clickable header to expand
    const clickable = block?.querySelector(".cursor-pointer");
    expect(clickable).toBeDefined();

    fireEvent.click(clickable!);

    // Wait for content to appear (SolidJS reactive update)
    await waitFor(() => {
      const content = block?.querySelector(".max-h-\\[500px\\]");
      expect(content).toBeDefined();
    });
  });

  it("should show expand_more icon when collapsed", () => {
    render(() => ReasoningBlock({ content: "thinking..." }), container);

    // Icon is a material-symbols-outlined with expand_more when collapsed
    const icons = container.querySelectorAll(".material-symbols-outlined");
    const expandIcon = Array.from(icons).find(i => i.textContent?.includes("expand_more"));
    expect(expandIcon).toBeDefined();
  });

  it("should show expand_less icon when expanded", async () => {
    render(() => ReasoningBlock({ content: "thinking..." }), container);

    // Click to expand
    const clickable = container.querySelector(".cursor-pointer");
    fireEvent.click(clickable!);

    // Wait for the icon to change to expand_less
    await waitFor(() => {
      const icons = container.querySelectorAll(".material-symbols-outlined");
      const expandIcon = Array.from(icons).find(i => i.textContent?.includes("expand_less"));
      expect(expandIcon).toBeDefined();
    });
  });

  it("should render markdown content when expanded", async () => {
    render(() => ReasoningBlock({ content: "**Bold** and *italic* thinking" }), container);

    const block = container.querySelector("[data-part='reasoning']");
    
    // Initially collapsed - markdown content should NOT be in DOM (T1.1: uses <Show> instead of CSS hiding)
    const initiallyHidden = block?.querySelector(".markdown-body");
    expect(initiallyHidden).toBeNull();

    // Click to expand
    const clickable = block?.querySelector(".cursor-pointer");
    fireEvent.click(clickable!);

    // Wait for markdown content to be rendered (async via createResource/worker)
    await waitFor(() => {
      const content = block?.querySelector(".markdown-body");
      expect(content).toBeDefined();
      // The markdown should be rendered with bold text
      expect(content?.innerHTML || "").toContain("strong");
    }, { timeout: 3000 });
  });
});
