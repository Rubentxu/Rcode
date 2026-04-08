import { describe, it, expect } from "vitest";
import { render } from "solid-js/web";
import { StreamingTextPart } from "./StreamingTextPart";

// Helper to flush SolidJS updates
const flushUpdates = () => new Promise(resolve => setTimeout(resolve, 0));

describe("StreamingTextPart", () => {
  it("should render content", async () => {
    const container = document.createElement("div");
    render(() => StreamingTextPart({ content: "Hello world", throttleMs: 10 }), container);
    await flushUpdates();

    const part = container.querySelector("[data-component='streaming-text-part']");
    expect(part).toBeDefined();
  });

  it("should render multiple blocks separated by double newlines", async () => {
    const container = document.createElement("div");
    render(() => StreamingTextPart({ content: "First paragraph\n\nSecond paragraph", throttleMs: 10 }), container);
    await flushUpdates();

    const completedBlocks = container.querySelectorAll("[data-component='completed-block']");
    const activeBlock = container.querySelector("[data-component='active-block']");

    // First block should be completed
    expect(completedBlocks.length).toBe(1);
    // Second block should be active
    expect(activeBlock).toBeDefined();
  });

  it("should render incomplete code fence as pre element", async () => {
    const container = document.createElement("div");
    render(() => StreamingTextPart({ content: "```python\nprint(", throttleMs: 10 }), container);
    await flushUpdates();

    // The incomplete code fence should render as fallback with class "incomplete-code"
    const incompleteCode = container.querySelector(".incomplete-code");
    expect(incompleteCode).toBeDefined();
  });

  it("should handle empty content", async () => {
    const container = document.createElement("div");
    render(() => StreamingTextPart({ content: "", throttleMs: 10 }), container);
    await flushUpdates();

    const part = container.querySelector("[data-component='streaming-text-part']");
    expect(part).toBeDefined();
  });
});
