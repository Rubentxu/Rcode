import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
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
  it("should display tool name in compact inline layout", () => {
    render(() => ToolCallCard({ id: "call_123", name: "bash", arguments: { cmd: "ls -la" } }), container);

    const card = container.querySelector("[data-part='tool_call']");
    expect(card).toBeDefined();
    expect(card?.className).toContain("inline-flex");
    expect(card?.textContent).toContain("bash");
  });

  it("should show running indicator inline", () => {
    render(() => ToolCallCard({ id: "call_123", name: "bash", arguments: { cmd: "ls", flags: ["-la"] }, status: "running" }), container);

    const statusIcon = Array.from(container.querySelectorAll(".material-symbols-outlined")).find(
      (icon) => icon.textContent?.includes("progress_activity"),
    );

    expect(statusIcon).toBeDefined();
  });

  it("should show success indicator inline", () => {
    render(() => ToolCallCard({ id: "call_456", name: "echo", arguments: "hello world", status: "success" }), container);

    const statusIcon = Array.from(container.querySelectorAll(".material-symbols-outlined")).find(
      (icon) => icon.textContent?.includes("check_circle"),
    );

    expect(statusIcon).toBeDefined();
  });

  it("should not render expandable arguments UI anymore", () => {
    render(() => ToolCallCard({ id: "call_789", name: "bash", arguments: { cmd: "this is a very long command that should be truncated in the preview" } }), container);

    const expandBtn = container.querySelector("button");
    const pre = container.querySelector("pre");

    expect(expandBtn).toBeNull();
    expect(pre).toBeNull();
  });
});
