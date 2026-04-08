import { describe, it, expect } from "vitest";
import { render } from "solid-js/web";
import { fireEvent } from "@testing-library/dom";
import { ReasoningBlock } from "./ReasoningBlock";

// Helper to flush SolidJS updates
const flushUpdates = () => new Promise(resolve => setTimeout(resolve, 0));

describe("ReasoningBlock", () => {
  it("should render reasoning block", () => {
    const container = document.createElement("div");
    render(() => ReasoningBlock({ content: "Let me think about this..." }), container);
    
    const details = container.querySelector("details");
    expect(details).toBeDefined();
  });

  it("should have reasoning label in summary", () => {
    const container = document.createElement("div");
    render(() => ReasoningBlock({ content: "thinking..." }), container);
    
    const summary = container.querySelector(".reasoning-summary");
    expect(summary?.textContent).toContain("Reasoning");
  });
});

// SPR-S1: Reasoning block collapsed by default, expands on click
describe("SPR-S1: Reasoning block collapsed by default and expands on click", () => {
  it("should be collapsed by default", () => {
    const container = document.createElement("div");
    render(() => ReasoningBlock({ content: "Let me think about this..." }), container);
    
    const details = container.querySelector("details");
    expect(details).toBeDefined();
    // details element should NOT have 'open' attribute by default
    expect(details?.hasAttribute("open")).toBe(false);
  });

  it("should show reasoning content after clicking to expand", async () => {
    const container = document.createElement("div");
    render(() => ReasoningBlock({ content: "My detailed reasoning content here" }), container);
    
    const details = container.querySelector("details");
    expect(details).toBeDefined();
    
    // Initially collapsed
    expect(details?.hasAttribute("open")).toBe(false);
    
    // Click on summary to expand
    const summary = container.querySelector("summary");
    expect(summary).toBeDefined();
    
    fireEvent.click(summary!);
    await flushUpdates();
    
    // After click, should be expanded
    expect(details?.hasAttribute("open")).toBe(true);
  });

  it("should show collapsed icon when collapsed", () => {
    const container = document.createElement("div");
    render(() => ReasoningBlock({ content: "thinking..." }), container);
    
    const icon = container.querySelector(".reasoning-icon");
    expect(icon?.textContent).toContain("▶"); // Right arrow when collapsed
  });

  it("should show expanded icon when expanded", async () => {
    const container = document.createElement("div");
    render(() => ReasoningBlock({ content: "thinking..." }), container);
    
    // Click to expand
    const summary = container.querySelector("summary");
    fireEvent.click(summary!);
    await flushUpdates();
    
    const icon = container.querySelector(".reasoning-icon");
    expect(icon?.textContent).toContain("▼"); // Down arrow when expanded
  });

  it("should render markdown content when expanded", async () => {
    const container = document.createElement("div");
    render(() => ReasoningBlock({ content: "**Bold** and *italic* thinking" }), container);
    
    // Initially collapsed
    const details = container.querySelector("details");
    expect(details?.hasAttribute("open")).toBe(false);
    
    // Click to expand
    const summary = container.querySelector("summary");
    fireEvent.click(summary!);
    await flushUpdates();
    
    // After expansion, markdown content should be visible in .reasoning-content
    const content = container.querySelector(".reasoning-content");
    expect(content).toBeDefined();
    // Check that markdown was rendered - bold becomes <strong>
    expect(content?.innerHTML || "").toContain("strong");
  });
});
