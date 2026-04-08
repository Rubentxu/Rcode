import { describe, it, expect } from "vitest";
import { ToolCallCard } from "./ToolCallCard";

describe("ToolCallCard", () => {
  it("should display tool name as header", () => {
    const container = document.createElement("div");
    const result = ToolCallCard({ id: "call_123", name: "bash", arguments: { cmd: "ls -la" } });
    container.appendChild(result as Node);
    
    const header = container.querySelector(".tool-call-header");
    expect(header).toBeDefined();
    expect(header?.textContent).toContain("bash");
  });

  it("should display formatted JSON arguments", () => {
    const container = document.createElement("div");
    const result = ToolCallCard({ id: "call_123", name: "bash", arguments: { cmd: "ls", flags: ["-la"] } });
    container.appendChild(result as Node);
    
    const pre = container.querySelector("pre code");
    expect(pre).toBeDefined();
    expect(pre?.textContent).toContain('"cmd"');
  });

  it("should handle arguments as string", () => {
    const container = document.createElement("div");
    const result = ToolCallCard({ id: "call_456", name: "echo", arguments: "hello world" });
    container.appendChild(result as Node);
    
    const pre = container.querySelector("pre code");
    expect(pre?.textContent).toContain("hello world");
  });
});
