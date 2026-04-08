import { createSignal, type Accessor } from "solid-js";

// Phase 3 draft state types - matches persisted MessagePart[]
export type DraftPart =
  | { type: "text"; content: string }
  | { type: "reasoning"; content: string }
  | { type: "tool_call"; id: string; name: string; arguments_delta: string; status: "running" | "completed" }
  | { type: "tool_result"; tool_call_id: string; content: string; is_error: boolean };

export interface DraftMessage {
  id: string;
  parts: DraftPart[];
}

// Pure functions to apply SSE events to draft state
export function applyStreamEvent(draft: DraftMessage | null, event: {
  type: string;
  [key: string]: unknown;
}): DraftMessage {
  const session_id = event.session_id as string;
  const currentDraft = draft ?? { id: `draft-${session_id}-${Date.now()}`, parts: [] };

  switch (event.type) {
    case "stream_text_delta": {
      const delta = event.delta as string;
      const parts = [...currentDraft.parts];
      // Append to last text part or create new one
      if (parts.length > 0 && parts[parts.length - 1].type === "text") {
        const lastText = parts[parts.length - 1] as { type: "text"; content: string };
        parts[parts.length - 1] = { ...lastText, content: lastText.content + delta };
      } else {
        parts.push({ type: "text", content: delta });
      }
      return { ...currentDraft, parts };
    }

    case "stream_text_snapshot": {
      // Legacy streaming_progress: accumulated_text is the FULL text, not a delta
      // Replace the last text part's content entirely
      const accumulated_text = event.accumulated_text as string;
      const parts = [...currentDraft.parts];
      if (parts.length > 0 && parts[parts.length - 1].type === "text") {
        const lastText = parts[parts.length - 1] as { type: "text"; content: string };
        parts[parts.length - 1] = { ...lastText, content: accumulated_text };
      } else {
        parts.push({ type: "text", content: accumulated_text });
      }
      return { ...currentDraft, parts };
    }

    case "stream_reasoning_delta": {
      const delta = event.delta as string;
      const parts = [...currentDraft.parts];
      // Append to last reasoning part or create new one
      if (parts.length > 0 && parts[parts.length - 1].type === "reasoning") {
        const lastReasoning = parts[parts.length - 1] as { type: "reasoning"; content: string };
        parts[parts.length - 1] = { ...lastReasoning, content: lastReasoning.content + delta };
      } else {
        parts.push({ type: "reasoning", content: delta });
      }
      return { ...currentDraft, parts };
    }

    case "stream_tool_call_start": {
      const tool_call_id = event.tool_call_id as string;
      const name = event.name as string;
      const parts = [...currentDraft.parts];
      parts.push({
        type: "tool_call",
        id: tool_call_id,
        name,
        arguments_delta: "",
        status: "running",
      });
      return { ...currentDraft, parts };
    }

    case "stream_tool_call_args_delta": {
      const tool_call_id = event.tool_call_id as string;
      const value = event.value as string;
      const parts = currentDraft.parts.map((part) => {
        if (part.type === "tool_call" && part.id === tool_call_id) {
          return {
            ...part,
            arguments_delta: part.arguments_delta + value,
          };
        }
        return part;
      });
      return { ...currentDraft, parts };
    }

    case "stream_tool_call_end": {
      const tool_call_id = event.tool_call_id as string;
      const parts = currentDraft.parts.map((part) => {
        if (part.type === "tool_call" && part.id === tool_call_id) {
          return { ...part, status: "completed" as const };
        }
        return part;
      });
      return { ...currentDraft, parts };
    }

    case "stream_tool_result": {
      const tool_call_id = event.tool_call_id as string;
      const content = event.content as string;
      const is_error = event.is_error as boolean;
      const parts = [...currentDraft.parts];
      parts.push({
        type: "tool_result",
        tool_call_id,
        content,
        is_error,
      });
      return { ...currentDraft, parts };
    }

    case "stream_assistant_committed":
    case "agent_finished":
      // Clear draft on commit
      return null as unknown as DraftMessage;

    default:
      return currentDraft;
  }
}

// Create reactive draft store
export function createDraftStore(): {
  draft: Accessor<DraftMessage | null>;
  dispatch: (event: { type: string; [key: string]: unknown }) => void;
  clear: () => void;
} {
  const [draft, setDraft] = createSignal<DraftMessage | null>(null);

  return {
    draft,
    dispatch: (event) => {
      setDraft((current) => applyStreamEvent(current, event));
    },
    clear: () => {
      setDraft(null);
    },
  };
}
