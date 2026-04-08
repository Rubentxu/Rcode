import { describe, it, expect } from "vitest";
import { applyStreamEvent, type DraftMessage } from "./useStreamingDraft";

describe("useStreamingDraft applyStreamEvent", () => {
  const sessionId = "test-session";

  describe("stream_text_delta", () => {
    it("should create new text part when draft is null", () => {
      const result = applyStreamEvent(null, {
        type: "stream_text_delta",
        session_id: sessionId,
        delta: "Hello",
      });

      expect(result.parts).toHaveLength(1);
      expect(result.parts[0].type).toBe("text");
      expect((result.parts[0] as { type: "text"; content: string }).content).toBe("Hello");
    });

    it("should append to existing text part", () => {
      const existingDraft: DraftMessage = {
        id: "draft-1",
        parts: [{ type: "text", content: "Hello" }],
      };

      const result = applyStreamEvent(existingDraft, {
        type: "stream_text_delta",
        session_id: sessionId,
        delta: " world",
      });

      expect(result.parts).toHaveLength(1);
      expect((result.parts[0] as { type: "text"; content: string }).content).toBe("Hello world");
    });

    it("should create new text part when last part is not text", () => {
      const existingDraft: DraftMessage = {
        id: "draft-1",
        parts: [{ type: "reasoning", content: "thinking..." }],
      };

      const result = applyStreamEvent(existingDraft, {
        type: "stream_text_delta",
        session_id: sessionId,
        delta: "Hello",
      });

      expect(result.parts).toHaveLength(2);
      expect(result.parts[1].type).toBe("text");
      expect((result.parts[1] as { type: "text"; content: string }).content).toBe("Hello");
    });
  });

  describe("stream_text_snapshot (legacy accumulated_text)", () => {
    it("should replace text content when last part is text", () => {
      const existingDraft: DraftMessage = {
        id: "draft-1",
        parts: [{ type: "text", content: "Old content" }],
      };

      const result = applyStreamEvent(existingDraft, {
        type: "stream_text_snapshot",
        session_id: sessionId,
        accumulated_text: "New complete accumulated text",
      });

      expect(result.parts).toHaveLength(1);
      expect((result.parts[0] as { type: "text"; content: string }).content).toBe("New complete accumulated text");
    });

    it("should create new text part when no parts exist", () => {
      const result = applyStreamEvent(null, {
        type: "stream_text_snapshot",
        session_id: sessionId,
        accumulated_text: "Full text from legacy",
      });

      expect(result.parts).toHaveLength(1);
      expect(result.parts[0].type).toBe("text");
      expect((result.parts[0] as { type: "text"; content: string }).content).toBe("Full text from legacy");
    });
  });

  describe("stream_reasoning_delta", () => {
    it("should create reasoning part when draft is null", () => {
      const result = applyStreamEvent(null, {
        type: "stream_reasoning_delta",
        session_id: sessionId,
        delta: "thinking...",
      });

      expect(result.parts).toHaveLength(1);
      expect(result.parts[0].type).toBe("reasoning");
      expect((result.parts[0] as { type: "reasoning"; content: string }).content).toBe("thinking...");
    });

    it("should append to existing reasoning part", () => {
      const existingDraft: DraftMessage = {
        id: "draft-1",
        parts: [{ type: "reasoning", content: "Let me think" }],
      };

      const result = applyStreamEvent(existingDraft, {
        type: "stream_reasoning_delta",
        session_id: sessionId,
        delta: " about this",
      });

      expect(result.parts).toHaveLength(1);
      expect((result.parts[0] as { type: "reasoning"; content: string }).content).toBe("Let me think about this");
    });
  });

  describe("stream_tool_call_start", () => {
    it("should add tool_call part with running status", () => {
      const result = applyStreamEvent(null, {
        type: "stream_tool_call_start",
        session_id: sessionId,
        tool_call_id: "call_123",
        name: "bash",
      });

      expect(result.parts).toHaveLength(1);
      expect(result.parts[0].type).toBe("tool_call");
      const toolCall = result.parts[0] as { type: "tool_call"; id: string; name: string; arguments_delta: string; status: string };
      expect(toolCall.id).toBe("call_123");
      expect(toolCall.name).toBe("bash");
      expect(toolCall.arguments_delta).toBe("");
      expect(toolCall.status).toBe("running");
    });
  });

  describe("stream_tool_call_args_delta", () => {
    it("should append to matching tool_call arguments_delta", () => {
      const existingDraft: DraftMessage = {
        id: "draft-1",
        parts: [
          { type: "text", content: "Running command" },
          { type: "tool_call", id: "call_123", name: "bash", arguments_delta: '{"cmd":', status: "running" },
        ],
      };

      const result = applyStreamEvent(existingDraft, {
        type: "stream_tool_call_args_delta",
        session_id: sessionId,
        tool_call_id: "call_123",
        value: '"ls -la"}',
      });

      const toolCall = result.parts[1] as { type: "tool_call"; id: string; arguments_delta: string };
      expect(toolCall.arguments_delta).toBe('{"cmd":"ls -la"}');
    });
  });

  describe("stream_tool_call_end", () => {
    it("should set tool_call status to completed", () => {
      const existingDraft: DraftMessage = {
        id: "draft-1",
        parts: [
          { type: "tool_call", id: "call_123", name: "bash", arguments_delta: "{}", status: "running" },
        ],
      };

      const result = applyStreamEvent(existingDraft, {
        type: "stream_tool_call_end",
        session_id: sessionId,
        tool_call_id: "call_123",
      });

      const toolCall = result.parts[0] as { type: "tool_call"; status: string };
      expect(toolCall.status).toBe("completed");
    });
  });

  describe("stream_tool_result", () => {
    it("should add tool_result part", () => {
      const result = applyStreamEvent(null, {
        type: "stream_tool_result",
        session_id: sessionId,
        tool_call_id: "call_123",
        content: "/home/user",
        is_error: false,
      });

      expect(result.parts).toHaveLength(1);
      expect(result.parts[0].type).toBe("tool_result");
      const toolResult = result.parts[0] as { type: "tool_result"; tool_call_id: string; content: string; is_error: boolean };
      expect(toolResult.tool_call_id).toBe("call_123");
      expect(toolResult.content).toBe("/home/user");
      expect(toolResult.is_error).toBe(false);
    });

    it("should handle error tool results", () => {
      const result = applyStreamEvent(null, {
        type: "stream_tool_result",
        session_id: sessionId,
        tool_call_id: "call_456",
        content: "Permission denied",
        is_error: true,
      });

      const toolResult = result.parts[0] as { type: "tool_result"; is_error: boolean };
      expect(toolResult.is_error).toBe(true);
    });
  });

  describe("stream_assistant_committed", () => {
    it("should return null to clear draft", () => {
      const existingDraft: DraftMessage = {
        id: "draft-1",
        parts: [{ type: "text", content: "Some content" }],
      };

      const result = applyStreamEvent(existingDraft, {
        type: "stream_assistant_committed",
        session_id: sessionId,
      });

      expect(result).toBeNull();
    });
  });
});
