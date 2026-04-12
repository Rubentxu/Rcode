import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { StreamingToolCallCard } from "./StreamingToolCallCard";

let container: HTMLDivElement;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
});

afterEach(() => {
  document.body.removeChild(container);
});

describe("StreamingToolCallCard", () => {
  it("should display spinner when status is running", () => {
    render(() => (
      <StreamingToolCallCard
        id="call_123"
        name="bash"
        arguments_delta='{"cmd": "ls"}'
        status="running"
      />
    ), container);

    const card = container.querySelector("[data-component='streaming-tool-call-card']");
    expect(card).toBeDefined();
    expect(card?.getAttribute("data-status")).toBe("running");

    const spinner = container.querySelector("svg.spinner");
    expect(spinner).toBeDefined();
  });

  it("should show completed checkmark in simplified inline layout", () => {
    render(() => (
      <StreamingToolCallCard
        id="call_123"
        name="bash"
        arguments_delta='{"cmd": "ls"}'
        status="completed"
      />
    ), container);

    const card = container.querySelector("[data-component='streaming-tool-call-card']");
    expect(card?.getAttribute("data-status")).toBe("completed");

    const checkmark = Array.from(container.querySelectorAll("span")).find((span) => span.textContent === "✓");
    expect(checkmark).toBeDefined();
    expect(checkmark?.textContent).toBe("✓");
  });

  it("should display tool name", () => {
    render(() => (
      <StreamingToolCallCard
        id="call_123"
        name="bash"
        arguments_delta=""
        status="running"
      />
    ), container);

    expect(container.textContent).toContain("bash");
  });

  it("should not render arguments section in the simplified layout", () => {
    render(() => (
      <StreamingToolCallCard
        id="call_123"
        name="bash"
        arguments_delta='{"cmd": "ls -la"}'
        status="running"
      />
    ), container);

    const argsSection = container.querySelector("[data-component='tool-call-args']");
    expect(argsSection).toBeNull();
  });

  it("should not render expand toggle anymore", () => {
    render(() => (
      <StreamingToolCallCard
        id="call_123"
        name="bash"
        arguments_delta='{"cmd": "ls"}'
        status="running"
      />
    ), container);

    const toggleBtn = container.querySelector("[data-component='toggle-expand']") as HTMLButtonElement;
    expect(toggleBtn).toBeNull();
  });
});
