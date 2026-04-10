import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { fireEvent, waitFor } from "@testing-library/dom";
import { ToolCallCard } from "./ToolCallCard";

// Container must be attached to document.body for SolidJS event delegation to work in jsdom
let container: HTMLDivElement;
beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
});
afterEach(() => {
  document.body.removeChild(container);
});

describe("ToolCallCard", () => {
  it("should display tool name as header", () => {
    render(() => ToolCallCard({ id: "call_123", name: "bash", arguments: { cmd: "ls -la" } }), container);

    // Should contain the tool name in a pill-style card
    const card = container.querySelector("[data-part='tool_call']");
    expect(card).toBeDefined();
    expect(card?.textContent).toContain("bash");
  });

  it("should display formatted JSON arguments when expanded", async () => {
    render(() => ToolCallCard({ id: "call_123", name: "bash", arguments: { cmd: "ls", flags: ["-la"] } }), container);

    // Expand the args
    const expandBtn = container.querySelector("button");
    fireEvent.click(expandBtn!);

    // Wait for the expanded content to appear
    await waitFor(() => {
      const pre = container.querySelector("pre");
      expect(pre).toBeDefined();
      expect(pre?.textContent).toContain("cmd");
    });
  });

  it("should handle arguments as string", async () => {
    render(() => ToolCallCard({ id: "call_456", name: "echo", arguments: "hello world" }), container);

    // Expand to see arguments
    const expandBtn = container.querySelector("button");
    fireEvent.click(expandBtn!);

    await waitFor(() => {
      const pre = container.querySelector("pre");
      expect(pre?.textContent).toContain("hello world");
    });
  });

  it("should show expand button for args preview", () => {
    // Use long args to test truncation
    render(() => ToolCallCard({ id: "call_789", name: "bash", arguments: { cmd: "this is a very long command that should be truncated in the preview" } }), container);

    // Should have an expand button
    const expandBtn = container.querySelector("button");
    expect(expandBtn).toBeDefined();
  });
});
