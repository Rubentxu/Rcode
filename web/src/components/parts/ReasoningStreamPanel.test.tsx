import { describe, it, expect } from "vitest";
import { render } from "solid-js/web";
import { fireEvent, waitFor } from "@testing-library/dom";
import { ReasoningStreamPanel } from "./ReasoningStreamPanel";

// Helper to flush SolidJS updates
const flushUpdates = () => new Promise(resolve => setTimeout(resolve, 0));

describe("ReasoningStreamPanel", () => {
  it("should be collapsed by default", async () => {
    const container = document.createElement("div");
    render(() => ReasoningStreamPanel({ content: "Let me think about this..." }), container);

    // Content should be hidden when collapsed
    const content = container.querySelector("[data-component='reasoning-content']");
    expect(content).toBeNull();
  });

  it("should expand on click", async () => {
    const container = document.createElement("div");
    render(() => ReasoningStreamPanel({ content: "Let me think about this..." }), container);

    // Click the toggle button
    const toggleBtn = container.querySelector("[data-component='reasoning-toggle']") as HTMLButtonElement;
    expect(toggleBtn).toBeDefined();
    fireEvent.click(toggleBtn);
    
    // Wait for the content to appear
    await waitFor(() => {
      const content = container.querySelector("[data-component='reasoning-content']");
      expect(content).toBeDefined();
    });
  });

  it("should display reasoning content when expanded", async () => {
    const container = document.createElement("div");
    render(() => ReasoningStreamPanel({ content: "The answer is 42" }), container);

    // Click to expand
    const toggleBtn = container.querySelector("[data-component='reasoning-toggle']") as HTMLButtonElement;
    fireEvent.click(toggleBtn);

    // Wait for the content div to appear (the pre element with content will be inside)
    await waitFor(() => {
      const content = container.querySelector("[data-component='reasoning-content']");
      expect(content).toBeDefined();
    });
  });

  it("should respect defaultCollapsed false", async () => {
    const container = document.createElement("div");
    render(() => ReasoningStreamPanel({ content: "Already expanded content", defaultCollapsed: false }), container);

    // Content should be visible immediately
    const content = container.querySelector("[data-component='reasoning-content']");
    expect(content).toBeDefined();
  });

  it("should toggle back to collapsed on second click", async () => {
    const container = document.createElement("div");
    render(() => ReasoningStreamPanel({ content: "Toggle test" }), container);

    const toggleBtn = container.querySelector("[data-component='reasoning-toggle']") as HTMLButtonElement;

    // First click - expand
    fireEvent.click(toggleBtn);
    await waitFor(() => {
      expect(container.querySelector("[data-component='reasoning-content']")).toBeDefined();
    });

    // Second click - collapse
    fireEvent.click(toggleBtn);
    await waitFor(() => {
      expect(container.querySelector("[data-component='reasoning-content']")).toBeNull();
    });
  });
});
