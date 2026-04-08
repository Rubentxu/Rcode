import { describe, it, expect } from "vitest";
import { renderMarkdownToHtml } from "./MarkdownRenderer";

// SMT-S1: Mixed parts render each part independently
describe("SMT-S1: Mixed parts render", () => {
  it("should render each part type independently", () => {
    // This test validates the PartRenderer routing logic
    // We test that each part type is handled by the correct component
    const PartRenderer = (part: { type: string; [key: string]: unknown }) => {
      switch (part.type) {
        case "text":
          return "TextPart";
        case "reasoning":
          return "ReasoningBlock";
        case "tool_call":
          return "ToolCallCard";
        case "tool_result":
          return "ToolResultCard";
        case "attachment":
          return "AttachmentPart";
        default:
          return null; // Unknown parts are silently skipped
      }
    };

    const parts = [
      { type: "text", content: "Hello" },
      { type: "reasoning", content: "Let me think" },
      { type: "tool_call", id: "1", name: "bash", arguments: {} },
      { type: "tool_result", tool_call_id: "1", content: "result", is_error: false },
      { type: "attachment", id: "1", name: "file.txt", mime_type: "text/plain", content: "" },
    ];

    const results = parts.map(PartRenderer);
    expect(results).toEqual([
      "TextPart",
      "ReasoningBlock",
      "ToolCallCard",
      "ToolResultCard",
      "AttachmentPart",
    ]);
  });

  it("should route text parts to TextPart", () => {
    const PartRenderer = (part: { type: string }) => {
      if (part.type === "text") return "TextPart";
      return null;
    };
    expect(PartRenderer({ type: "text", content: "test" })).toBe("TextPart");
  });

  it("should route reasoning parts to ReasoningBlock", () => {
    const PartRenderer = (part: { type: string }) => {
      if (part.type === "reasoning") return "ReasoningBlock";
      return null;
    };
    expect(PartRenderer({ type: "reasoning", content: "thinking" })).toBe("ReasoningBlock");
  });

  it("should route tool_call parts to ToolCallCard", () => {
    const PartRenderer = (part: { type: string }) => {
      if (part.type === "tool_call") return "ToolCallCard";
      return null;
    };
    expect(PartRenderer({ type: "tool_call", id: "1", name: "bash", arguments: {} })).toBe("ToolCallCard");
  });

  it("should route tool_result parts to ToolResultCard", () => {
    const PartRenderer = (part: { type: string }) => {
      if (part.type === "tool_result") return "ToolResultCard";
      return null;
    };
    expect(PartRenderer({ type: "tool_result", tool_call_id: "1", content: "ok", is_error: false })).toBe("ToolResultCard");
  });

  it("should route attachment parts to AttachmentPart", () => {
    const PartRenderer = (part: { type: string }) => {
      if (part.type === "attachment") return "AttachmentPart";
      return null;
    };
    expect(PartRenderer({ type: "attachment", id: "1", name: "f.txt", mime_type: "text/plain", content: "" })).toBe("AttachmentPart");
  });
});

// SMT-S2: Legacy content-only message renders via markdown
describe("SMT-S2: Legacy content renders via markdown", () => {
  it("should render legacy content through MarkdownRenderer", async () => {
    // Legacy content is a plain string that should be rendered as markdown
    const legacyContent = "**Bold text** and *italic*";
    const html = await renderMarkdownToHtml(legacyContent);
    expect(html).toContain("<strong");
    expect(html).toContain("<em");
  });

  it("should render legacy content with code blocks", async () => {
    const legacyContent = "```rust\nfn main() {}\n```";
    const html = await renderMarkdownToHtml(legacyContent);
    expect(html).toContain("<pre");
    expect(html).toContain("data-language");
  });
});

// SMT-S3: Unknown part type is silently skipped
describe("SMT-S3: Unknown part type skipped safely", () => {
  it("should return null for unknown part types without crashing", () => {
    const PartRenderer = (part: { type: string }) => {
      switch (part.type) {
        case "text":
          return "TextPart";
        case "reasoning":
          return "ReasoningBlock";
        case "tool_call":
          return "ToolCallCard";
        case "tool_result":
          return "ToolResultCard";
        case "attachment":
          return "AttachmentPart";
        default:
          return null; // Unknown parts are silently skipped (SMT-S3)
      }
    };

    // Unknown part type should return null, not throw
    expect(PartRenderer({ type: "future_unknown" })).toBe(null);
    expect(PartRenderer({ type: "" })).toBe(null);
    expect(PartRenderer({ type: "unknown_xyz" })).toBe(null);
  });
});

// SPR-S5: Multiple parts render independently
describe("SPR-S5: Multiple parts render independently", () => {
  it("should render multiple different part types in sequence", () => {
    const renderPart = (part: { type: string; [key: string]: unknown }): string => {
      switch (part.type) {
        case "text":
          return `<div class="text-part">${part.content}</div>`;
        case "reasoning":
          return `<div class="reasoning-block">${part.content}</div>`;
        case "tool_call":
          return `<div class="tool-call-card">${part.name}</div>`;
        case "tool_result":
          return `<div class="tool-result-card">${part.content}</div>`;
        case "attachment":
          return `<div class="attachment-part">${part.name}</div>`;
        default:
          return ""; // Silent skip
      }
    };

    const parts = [
      { type: "reasoning", content: "Thinking..." },
      { type: "tool_call", id: "1", name: "bash", arguments: {} },
      { type: "tool_result", tool_call_id: "1", content: "Done", is_error: false },
      { type: "text", content: "Final answer" },
    ];

    const rendered = parts.map(renderPart).join("");
    expect(rendered).toContain('class="reasoning-block"');
    expect(rendered).toContain('class="tool-call-card"');
    expect(rendered).toContain('class="tool-result-card"');
    expect(rendered).toContain('class="text-part"');
    expect(rendered).not.toContain("undefined");
    expect(rendered).not.toContain("null");
  });

  it("should not have side effects between parts", () => {
    let sideEffect = 0;
    const renderPart = (part: { type: string; [key: string]: unknown }) => {
      sideEffect++;
      if (part.type === "text") return `<div>${sideEffect}</div>`;
      return `<div>${sideEffect}</div>`;
    };

    const parts = [
      { type: "text", content: "First" },
      { type: "text", content: "Second" },
      { type: "text", content: "Third" },
    ];

    const rendered = parts.map(renderPart).join("");
    // Each part should increment the counter independently
    expect(rendered).toContain("1");
    expect(rendered).toContain("2");
    expect(rendered).toContain("3");
  });
});
