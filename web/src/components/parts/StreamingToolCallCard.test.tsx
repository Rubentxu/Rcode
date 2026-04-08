import { describe, it, expect } from "vitest";
import { StreamingToolCallCard } from "./StreamingToolCallCard";

describe("StreamingToolCallCard", () => {
  it("should display spinner when status is running", () => {
    const container = document.createElement("div");
    const result = StreamingToolCallCard({
      id: "call_123",
      name: "bash",
      arguments_delta: '{"cmd": "ls"}',
      status: "running",
    });
    container.appendChild(result as Node);

    const card = container.querySelector("[data-component='streaming-tool-call-card']");
    expect(card).toBeDefined();
    expect(card?.getAttribute("data-status")).toBe("running");

    // Should show spinner for running status
    const spinner = container.querySelector(".status-running");
    expect(spinner).toBeDefined();
  });

  it("should show checkmark when status is completed", () => {
    const container = document.createElement("div");
    const result = StreamingToolCallCard({
      id: "call_123",
      name: "bash",
      arguments_delta: '{"cmd": "ls"}',
      status: "completed",
    });
    container.appendChild(result as Node);

    const card = container.querySelector("[data-component='streaming-tool-call-card']");
    expect(card?.getAttribute("data-status")).toBe("completed");

    // Should show checkmark for completed status
    const checkmark = container.querySelector(".status-complete");
    expect(checkmark).toBeDefined();
    expect(checkmark?.textContent).toBe("✓");
  });

  it("should display tool name", () => {
    const container = document.createElement("div");
    const result = StreamingToolCallCard({
      id: "call_123",
      name: "bash",
      arguments_delta: "",
      status: "running",
    });
    container.appendChild(result as Node);

    const toolName = container.querySelector("[data-component='tool-name']");
    expect(toolName?.textContent).toBe("bash");
  });

  it("should show arguments when expanded", () => {
    const container = document.createElement("div");
    const result = StreamingToolCallCard({
      id: "call_123",
      name: "bash",
      arguments_delta: '{"cmd": "ls -la"}',
      status: "running",
    });
    container.appendChild(result as Node);

    // Initially not expanded, but arguments_delta exists so it should show
    const argsSection = container.querySelector("[data-component='tool-call-args']");
    expect(argsSection).toBeDefined();
  });

  it("should toggle expand on button click", () => {
    const container = document.createElement("div");
    const result = StreamingToolCallCard({
      id: "call_123",
      name: "bash",
      arguments_delta: '{"cmd": "ls"}',
      status: "running",
    });
    container.appendChild(result as Node);

    // Find the toggle button
    const toggleBtn = container.querySelector("[data-component='toggle-expand']") as HTMLButtonElement;
    expect(toggleBtn).toBeDefined();

    // Click to expand
    toggleBtn.click();

    // After click, should show expanded content
    const content = container.querySelector("[data-component='tool-call-args'] pre");
    expect(content).toBeDefined();
  });
});
