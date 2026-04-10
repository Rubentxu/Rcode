import { For, Show, Switch, Match, createSignal, onCleanup, createEffect, createMemo, onMount } from "solid-js";
import type { Session } from "../App";
import type { Message, MessagePart } from "../api/types";
import { createSSEClient, type SSEClient } from "../api/sse";
import { getApiBase } from "../api/config";
import DiffViewer from "./DiffViewer";
import { containsDiff, extractDiffBlocks } from "../api/diff";
import PromptInput from "./PromptInput";
import type { CommandContext } from "../commands";
import { TextPart } from "./parts/TextPart";
import { ReasoningBlock } from "./parts/ReasoningBlock";
import { ToolCallCard } from "./parts/ToolCallCard";
import { ToolResultCard } from "./parts/ToolResultCard";
import { AttachmentPart } from "./parts/AttachmentPart";
import { StreamingTextPart } from "./parts/StreamingTextPart";
import { StreamingToolCallCard } from "./parts/StreamingToolCallCard";
import { ReasoningStreamPanel } from "./parts/ReasoningStreamPanel";
import { TurnAvatar } from "./parts/TurnAvatar";
import { QuickActions } from "./parts/QuickActions";
import { createDraftStore, type DraftPart } from "../composables/useStreamingDraft";
import type {
  SSEStreamReasoningDelta,
  SSEStreamTextDelta,
  SSEStreamToolCallArg,
  SSEStreamToolCallEnd,
  SSEStreamToolCallStart,
  SSEStreamToolResult,
} from "../api/types";

/** Represents a conversational turn: one or more consecutive messages from the same role */
interface Turn {
  role: "user" | "assistant" | "system";
  messages: Message[];
}

interface SessionViewProps {
  session: Session;
  messages: Message[];
  isLoading: () => boolean;
  sseStatus: "connected" | "connecting" | "disconnected";
  onSubmit: (prompt: string) => void;
  onAbort: () => void;
  onSSEStatusChange?: (status: "connected" | "connecting" | "disconnected") => void;
  sessions: Session[];
  onCommandResult?: (result: { success: boolean; message: string; data?: unknown }) => void;
  onComplete?: () => void;
  onReloadMessages?: () => void;
  onError?: (error: string) => void;
  // MQA-3: Branch callback - creates new session seeded with conversation up to messageId
  onBranch?: (messageId: string, messagesUpTo: Message[]) => void;
  // MQA-2: Retry callback - replaces assistant response, re-submits user prompt without duplicating user message
  onRetry: (assistantMessageId: string, userPrompt: string) => void;
  // MQA-2: Initialize draft/optimistic shell explicitly (needed for retry to show draft immediately)
  initDraft?: (sessionId: string) => void;
  currentModel?: string;
}

export default function SessionView(props: SessionViewProps) {
  let sseClient: SSEClient | null = null;
  let connectedSessionId: string | null = null;
  let scrollContainerRef: HTMLDivElement | undefined;
  
  // Phase 3: Draft store for streaming parts
  const { draft, dispatch, clear: clearDraft, initOptimisticShell } = createDraftStore();
  
  // Phase 5: Scroll anchoring state
  const NEAR_BOTTOM_THRESHOLD_PX = 50;
  let isNearBottom = true;
  let rafScheduled = false;

  // Create a derived message list that includes persisted messages
  const displayMessages = () => {
    return [...props.messages];
  };

  /**
   * Groups consecutive messages by role into conversational turns.
   * CT-4: Grouping is purely presentational; persisted order and message IDs are NOT altered.
   */
  const turns = createMemo((): Turn[] => {
    const result: Turn[] = [];
    const msgs = displayMessages();
    let i = 0;
    while (i < msgs.length) {
      const msg = msgs[i];
      const role = msg.role as "user" | "assistant" | "system";
      const turnMessages: Message[] = [msg];
      let j = i + 1;
      // Group consecutive messages with the same role
      while (j < msgs.length && msgs[j].role === msg.role) {
        turnMessages.push(msgs[j]);
        j++;
      }
      result.push({ role, messages: turnMessages });
      i = j;
    }
    return result;
  });

  /**
   * Extracts the final text content from a message for copy action (MQA-1).
   * Prefers structured parts (text part), falls back to legacy content field.
   */
  const extractTextContent = (message: Message): string => {
    if (message.parts && message.parts.length > 0) {
      // Find the last text part - that's the final assistant response
      const textParts = message.parts.filter((p) => p.type === "text");
      if (textParts.length > 0) {
        const lastText = textParts[textParts.length - 1];
        return lastText.content;
      }
      // If no text parts but has other parts, return empty (can't copy non-text)
      return "";
    }
    // Fallback to legacy content
    return message.content ?? "";
  };

  /**
   * Finds the user prompt that preceded an assistant message for retry (MQA-2).
   * Returns the content of the preceding user message, or null if not found.
   */
  const findPrecedingUserPrompt = (assistantMessageId: string): string | null => {
    const msgs = displayMessages();
    const assistantIndex = msgs.findIndex((m) => m.id === assistantMessageId);
    if (assistantIndex <= 0) return null;

    // Find the preceding user message
    for (let i = assistantIndex - 1; i >= 0; i--) {
      if (msgs[i].role === "user") {
        // Return the text content of the user message
        return extractTextContent(msgs[i]);
      }
    }
    return null;
  };

  /**
   * MQA-2: Retry handler - removes assistant response and re-submits preceding user prompt.
   * MQA-6: Retry does NOT duplicate; it replaces the assistant response from that turn onward.
   * Uses the dedicated onRetry callback (not onSubmit) to avoid duplicating the user message.
   */
  const handleRetry = (assistantMessageId: string) => {
    const userPrompt = findPrecedingUserPrompt(assistantMessageId);
    if (!userPrompt) {
      console.warn("Retry: no preceding user prompt found for", assistantMessageId);
      return;
    }
    // Delegate to onRetry which handles truncation and API call without duplicating user message
    props.onRetry(assistantMessageId, userPrompt);
  };

  /**
   * MQA-3: Branch handler - creates new session seeded with conversation up to this message.
   */
  const handleBranch = (messageId: string) => {
    const msgs = displayMessages();
    const messageIndex = msgs.findIndex((m) => m.id === messageId);
    if (messageIndex < 0) {
      console.warn("Branch: message not found", messageId);
      return;
    }
    // Get all messages up to and including this one
    const messagesUpTo = msgs.slice(0, messageIndex + 1);
    props.onBranch?.(messageId, messagesUpTo);
  };
  
  // Phase 5: Scroll handler to track user's scroll position
  const handleScroll = () => {
    if (!scrollContainerRef) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollContainerRef;
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
    isNearBottom = distanceFromBottom <= NEAR_BOTTOM_THRESHOLD_PX;
  };
  
  // Phase 5: Scroll to bottom function
  const scrollToBottom = () => {
    if (!scrollContainerRef) return;
    scrollContainerRef.scrollTop = scrollContainerRef.scrollHeight;
  };
  
  // Phase 5: Auto-scroll when near bottom, using RAF to batch updates
  createEffect(() => {
    // Trigger on draft changes during loading
    if (props.isLoading() && draft() && isNearBottom && !rafScheduled) {
      rafScheduled = true;
      requestAnimationFrame(() => {
        scrollToBottom();
        rafScheduled = false;
      });
    }
  });

  // SS-3: Determine if skeleton should be shown (optimistic shell with no content yet)
  // Returns true when draft is optimistic AND has only the initial empty text part
  const showSkeleton = () => {
    const d = draft();
    if (!d || !d.isOptimistic) return false;
    // Optimistic shell with only the initial empty text part — show skeleton
    if (d.parts.length === 1 && d.parts[0].type === "text") {
      const textPart = d.parts[0] as { type: "text"; content: string };
      return textPart.content === "";
    }
    return false;
  };

  // Keep the per-session SSE connection open to avoid missing fast responses.
  const connectSSE = async (sessionId: string) => {
    if (sseClient && connectedSessionId === sessionId) {
      return;
    }

    if (sseClient) {
      sseClient.disconnect();
      sseClient = null;
      connectedSessionId = null;
    }

    const apiBase = await getApiBase();
    sseClient = createSSEClient({
      sessionId,
      apiBase,
      onStatusChange: (status) => {
        props.onSSEStatusChange?.(status);
      },
      // Legacy: accumulated text from old streaming_progress events
      onDelta: (event) => {
        // For backward compat, still handle legacy streaming_progress events
        // accumulated_text is the FULL accumulated text (not a delta),
        // so we use stream_text_snapshot to REPLACE text content, not append
        if (event.accumulated_text) {
          dispatch({ type: "stream_text_snapshot", session_id: sessionId, accumulated_text: event.accumulated_text });
        }
      },
      onMessage: () => {
        props.onReloadMessages?.();
      },
      onDone: () => {
        clearDraft();
        props.onReloadMessages?.();
        props.onComplete?.();
      },
      onError: (event) => {
        console.error("SSE error:", event.error);
        clearDraft();
        props.onError?.(event.error);
      },
      // Phase 3: New semantic event callbacks
      onTextDelta: (event) => {
        dispatch(event as SSEStreamTextDelta);
      },
      onReasoningDelta: (event) => {
        dispatch(event as SSEStreamReasoningDelta);
      },
      onToolCallStart: (event) => {
        dispatch(event as SSEStreamToolCallStart);
      },
      onToolCallArg: (event) => {
        dispatch(event as SSEStreamToolCallArg);
      },
      onToolCallEnd: (event) => {
        dispatch(event as SSEStreamToolCallEnd);
      },
      onToolResult: (event) => {
        dispatch(event as SSEStreamToolResult);
      },
      onAssistantCommitted: () => {
        clearDraft();
        props.onReloadMessages?.();
        props.onComplete?.();
      },
    });

    connectedSessionId = sessionId;
    sseClient.connect();
  };
  
  // Keep a session-scoped SSE subscription alive while the session is selected.
  createEffect(() => {
    const sessionId = props.session.id;
    void connectSSE(sessionId);
  });

  createEffect(() => {
    if (!props.isLoading()) {
      clearDraft();
    }
  });

  // SS-1: Initialize optimistic shell immediately when loading starts (before SSE events arrive)
  createEffect(() => {
    if (props.isLoading() && !draft()) {
      // Only init if we don't already have a draft (e.g., from a previous streaming session that was cleared)
      initOptimisticShell(props.session.id);
      // Also call the prop callback if provided (allows App to explicitly initialize draft, e.g., for retry)
      props.initDraft?.(props.session.id);
    }
  });

  // Clean up SSE on unmount or when loading stops
  onCleanup(() => {
    if (sseClient) {
      sseClient.disconnect();
      sseClient = null;
      connectedSessionId = null;
    }
  });

  // Build command context for slash commands
  const commandContext: CommandContext = {
    currentSessionId: props.session.id,
    sessions: props.sessions.map((s) => ({ id: s.id, title: s.title || "" })),
    messages: props.messages.map((m) => ({ id: m.id, role: m.role, content: m.content ?? "" })),
  };

  const handleCommandResult = (result: { success: boolean; message: string; data?: unknown }) => {
    props.onCommandResult?.(result);
  };

  return (
    <div class="flex flex-col h-full">
      <div
        ref={scrollContainerRef}
        class="flex-1 overflow-y-auto p-8 custom-scrollbar"
        onScroll={handleScroll}
      >
        {/* CT-3: Transcript uses max-width centering for readability */}
        <div data-component="transcript" class="max-w-5xl mx-auto w-full">
          <Show when={turns().length === 0 && !draft()} fallback={
            <>
              {/* Conversational turns with avatar slots */}
              <For each={turns()}>
                {(turn) => (
                  <div 
                    class={`turn turn--${turn.role}`}
                    data-turn-role={turn.role}
                  >
                    {/* CT-2: Avatar slot — omitted for system role's inline rendering */}
                    <Show when={turn.role !== "system"}>
                      <TurnAvatar role={turn.role} />
                    </Show>
                    <div class="turn-content">
                      <For each={turn.messages}>
                        {(message) => (
                          <div data-component="message" data-role={message.role}>
                            {/* CT-6: System messages render inline without avatar — show subtle role label */}
                            <Show when={message.role === "system"}>
                              <div data-component="message-header">
                                <span style="font-size: var(--text-xs); font-weight: 600; color: var(--text-muted);">
                                  system
                                </span>
                              </div>
                            </Show>
                            <div data-component="message-content">
                              <MessageContent message={message} />
                            </div>
                            {/* MQA-1, MQA-2, MQA-3: Quick actions for assistant messages only */}
                            <Show when={message.role === "assistant"}>
                              <QuickActions
                                messageId={message.id}
                                textContent={extractTextContent(message)}
                                onRetry={handleRetry}
                                onBranch={handleBranch}
                              />
                            </Show>
                          </div>
                        )}
                      </For>
                    </div>
                  </div>
                )}
              </For>
              {/* Phase 3: Show draft message during streaming as an assistant turn */}
              {/* SS-1: Optimistic shell appears immediately on submit BEFORE any SSE delta */}
              {/* SS-2: Abort button lives inside the shell, not in a separate bottom bar */}
              <Show when={props.isLoading() && draft()}>
                <div 
                  class="turn turn--assistant" 
                  data-turn-role="assistant"
                  data-streaming={draft()?.isOptimistic ? "optimistic" : "streaming"}
                >
                  <TurnAvatar role="assistant" />
                  <div class="turn-content">
                    <div data-component="message" data-role="assistant">
                      {/* SS-2: Abort button in shell header - no separate bottom bar */}
                      <div data-component="shell-header">
                        <span style="font-size: var(--text-xs); font-weight: 600; text-transform: uppercase; color: var(--text-muted);">
                          assistant
                        </span>
                        <Show when={draft()?.isOptimistic} fallback={
                          <span style="font-size: var(--text-xs); color: var(--text-muted);">
                            streaming...
                          </span>
                        }>
                          <span style="font-size: var(--text-xs); color: var(--text-muted);">
                            thinking...
                          </span>
                        </Show>
                        {/* SS-2: Stop button moved INTO the shell - no bottom bar needed */}
                        <button 
                          data-component="shell-abort" 
                          onClick={props.onAbort}
                          aria-label="Stop generation"
                        >
                          <svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor">
                            <rect x="4" y="4" width="16" height="16" rx="2"/>
                          </svg>
                          Stop
                        </button>
                      </div>
                      {/* SS-3: Skeleton → streaming content transition */}
                      <div data-component="message-content">
                        <Show 
                          when={!showSkeleton()}
                          fallback={<SkeletonContent />}
                        >
                          <DraftMessageContent parts={draft()!.parts} />
                        </Show>
                      </div>
                    </div>
                  </div>
                </div>
              </Show>
            </>
          }>
            <Show when={turns().length === 0 && !draft()}>
              <div data-component="empty-state" style="height: 200px;">
                <p data-component="empty-state-description">
                  Start a conversation by typing a message below
                </p>
              </div>
            </Show>
          </Show>
        </div>
      </div>

      {/* Bottom processing bar removed — abort control lives inside the assistant shell */}
      <Show when={false}>
        <div />
      </Show>

      <PromptInput
        onSubmit={props.onSubmit}
        onCommand={handleCommandResult}
        disabled={props.isLoading()}
        context={commandContext}
        currentModel={props.currentModel}
      />
    </div>
  );
}

interface MessageContentProps {
  message: Message;
}

function MessageContent(props: MessageContentProps) {
  // Route to structured parts if available, otherwise fall back to legacy content rendering
  const hasParts = () => Array.isArray(props.message.parts) && props.message.parts.length > 0;
  
  return (
    <Show 
      when={hasParts()} 
      fallback={<LegacyContent content={props.message.content ?? ""} />}
    >
      <StructuredParts parts={props.message.parts!} />
    </Show>
  );
}

// Renders structured message parts
function StructuredParts(props: { parts: MessagePart[] }) {
  return (
    <div data-component="structured-parts">
      <For each={props.parts}>
        {(part) => <PartRenderer part={part} />}
      </For>
    </div>
  );
}

// Router for each part type - unknown types are silently skipped (SMT-S3)
function PartRenderer(props: { part: MessagePart }) {
  const partType = props.part.type;
  
  if (partType === "text") {
    return <TextPart content={props.part.content} />;
  }
  
  if (partType === "reasoning") {
    return <ReasoningBlock content={props.part.content} />;
  }
  
  if (partType === "tool_call") {
    return <ToolCallCard 
      id={props.part.id} 
      name={props.part.name} 
      arguments={props.part.arguments} 
    />;
  }
  
  if (partType === "tool_result") {
    return <ToolResultCard 
      tool_call_id={props.part.tool_call_id} 
      content={props.part.content} 
      is_error={props.part.is_error} 
    />;
  }
  
  if (partType === "attachment") {
    return <AttachmentPart 
      id={props.part.id} 
      name={props.part.name} 
      mime_type={props.part.mime_type} 
      content={props.part.content} 
    />;
  }
  
  // Unknown part type - silently skip (SMT-S3)
  return null;
}

// Legacy content rendering for backward compatibility (SMT-4 fallback)
// When parts are absent, render content through MarkdownRenderer for proper formatting
function LegacyContent(props: { content: string }) {
  const hasDiff = createMemo(() => containsDiff(props.content));
  
  const contentBlocks = createMemo(() => {
    if (!hasDiff()) {
      return null;
    }
    return extractDiffBlocks(props.content);
  });
  
  return (
    <Show when={hasDiff()} fallback={<TextPart content={props.content} />}>
      <div class="message-with-diffs">
        <For each={contentBlocks()}>
          {(block) => {
            // Check if this block is a diff or regular content
            if (containsDiff(block)) {
              return <DiffViewer diff={block} collapsible={true} defaultCollapsed={false} />;
            } else {
              // Non-diff blocks still go through markdown rendering
              return <TextPart content={block} />;
            }
          }}
        </For>
      </div>
    </Show>
  );
}

// Phase 3: Renders structured draft parts during streaming
function DraftMessageContent(props: { parts: DraftPart[] }) {
  return (
    <div data-component="draft-parts">
      <For each={props.parts}>
        {(part) => <DraftPartRenderer part={part} />}
      </For>
    </div>
  );
}

// SS-1 / SS-3: Skeleton content shown in optimistic shell before first SSE delta arrives
// Smoothly transitions to real content when first delta is received
function SkeletonContent() {
  return (
    <div data-component="skeleton-content">
      {/* Simulate 3 lines of varying width for natural skeleton appearance */}
      <span data-component="skeleton-line" style="width: 65%;">.</span>
      <span data-component="skeleton-line" style="width: 45%;">.</span>
      <span data-component="skeleton-line" style="width: 80%;">.</span>
    </div>
  );
}

// Phase 3: Router for draft part types during streaming
function DraftPartRenderer(props: { part: DraftPart }) {
  const partType = props.part.type;
  
  if (partType === "text") {
    return <StreamingTextPart content={props.part.content} />;
  }
  
  if (partType === "reasoning") {
    return <ReasoningStreamPanel content={props.part.content} />;
  }
  
  if (partType === "tool_call") {
    return (
      <StreamingToolCallCard 
        id={props.part.id}
        name={props.part.name}
        arguments_delta={props.part.arguments_delta}
        status={props.part.status}
      />
    );
  }
  
  if (partType === "tool_result") {
    return <ToolResultCard 
      tool_call_id={props.part.tool_call_id} 
      content={props.part.content} 
      is_error={props.part.is_error} 
    />;
  }
  
  // Unknown part type - silently skip
  return null;
}

function ConnectionStatus(props: { status: "connected" | "connecting" | "disconnected" }) {
  const statusColors = {
    connected: "var(--secondary)",
    connecting: "var(--tertiary)",
    disconnected: "var(--outline)",
  };

  return (
    <div class="flex items-center gap-2">
      <span
        data-component="status-dot"
        data-status={props.status}
        class="relative flex h-2 w-2"
      >
        <Show when={props.status === "connected"}>
          <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-secondary opacity-75"></span>
        </Show>
        <span
          class={`relative inline-flex rounded-full h-2 w-2`}
          style={{ "background-color": statusColors[props.status] }}
        ></span>
      </span>
      <span class="text-xs text-outline capitalize">{props.status}</span>
    </div>
  );
}
