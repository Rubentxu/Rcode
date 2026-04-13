import { For, Show, Switch, Match, createSignal, onCleanup, createEffect, createMemo, onMount } from "solid-js";
import type { Session } from "../stores";
import type { Message, MessagePart } from "../api/types";
import { createSSEClient, type SSEClient } from "../api/sse";
import { getApiBase } from "../api/config";
import { prepare, layout } from "@chenglou/pretext";
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
import TaskChecklistPanel from "./chat-workspace/TaskChecklistPanel";
import { deriveChecklistItems } from "./chat-workspace/checklist";
import { createDraftStore, type DraftPart } from "../composables/useStreamingDraft";
import { createVirtualizer } from "@tanstack/solid-virtual";
import type {
  SSEStreamReasoningDelta,
  SSEStreamTextDelta,
  SSEStreamToolCallArg,
  SSEStreamToolCallEnd,
  SSEStreamToolCallStart,
  SSEStreamToolResult,
} from "../api/types";
import { useWorkspace } from "../context/WorkspaceContext";

/** Represents a conversational turn: one or more consecutive messages from the same role */
interface Turn {
  role: "user" | "assistant" | "system";
  messages: Message[];
}

interface SessionViewProps {
  session: Session;
  onSubmit: (prompt: string) => void;
  onAbort: () => void;
  onCommandResult?: (result: { success: boolean; message: string; data?: unknown }) => void;
  onComplete?: () => void;
  onReloadMessages?: () => Promise<void>;
  onError?: (error: string) => void;
  // MQA-3: Branch callback - creates new session seeded with conversation up to messageId
  onBranch?: (messageId: string, messagesUpTo: Message[]) => void;
  // MQA-2: Retry callback - replaces assistant response, re-submits user prompt without duplicating user message
  onRetry: (assistantMessageId: string, userPrompt: string) => void;
  currentModel?: string;
}

export default function SessionView(props: SessionViewProps) {
  // Get workspace context for messages, loading state, SSE status, and sessions
  const workspaceContext = useWorkspace();
  
  let sseClient: SSEClient | null = null;
  let connectedSessionId: string | null = null;
  let scrollContainerRef: HTMLDivElement | undefined;
  
  // T3.3: Use a signal for scroll element to allow reactive updates
  const [scrollElement, setScrollElement] = createSignal<HTMLDivElement | undefined>(undefined);
  
  // T3.3: Track whether virtualization is enabled (scroll element is available and virtualizer has items)
  // Only enable virtualization when both the scroll element exists AND the virtualizer has computed virtual items
  const isVirtualizationEnabled = () => {
    // Only enable if scroll element is set AND we have turns to render
    // This prevents virtualization from being enabled before the DOM is ready
    const hasScrollElement = scrollElement() !== undefined;
    const hasTurns = turns().length > 0;
    const hasVirtualItems = virtualItems().length > 0;
    
    // Enable virtualization only when scroll element is ready and we have virtual items computed
    // This ensures the virtualizer has had a chance to compute its state
    return hasScrollElement && hasTurns && hasVirtualItems;
  };
  
  // Phase 3: Draft store for streaming parts
  const { draft, dispatch, clear: clearDraft, initOptimisticShell } = createDraftStore();
  
  // Create a derived message list that includes persisted messages from workspace
  // T3.3: Must be defined BEFORE virtualizer since it references turns in its getter
  const displayMessages = () => {
    return workspaceContext.workspace.getMessages(props.session.id);
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
  
  // T3.3: Virtualizer for efficient rendering of long conversations
  // Pretext height estimation: pre-compute turn heights without DOM measurement.
  // Uses Intl.Segmenter + canvas.measureText for accurate line-count prediction.
  const PRETEXT_FONT = "14px system-ui";
  const PRETEXT_LINE_HEIGHT = 22;
  const PRETEXT_TURN_PADDING = 48; // avatar + gap + margin

  // Cache estimates by content hash to avoid re-computation
  const heightEstimateCache = new Map<string, number>();
  const estimateTurnHeight = (index: number): number => {
    const allTurns = turns();
    const turn = allTurns[index];
    if (!turn) return 200;

    // Extract text content from turn messages for measurement
    const text = turn.messages
      .map((m) => {
        if (m.parts) {
          return m.parts
            .filter((p) => p.type === "text")
            .map((p) => (p as { type: "text"; content: string }).content)
            .join(" ");
        }
        return m.content ?? "";
      })
      .join("\n");

    if (!text?.trim()) return PRETEXT_TURN_PADDING;

    // Cache key: text length + first 50 chars (avoids re-measuring same content)
    const cacheKey = `${text.length}:${text.slice(0, 50)}`;
    const cached = heightEstimateCache.get(cacheKey);
    if (cached !== undefined) return cached;

    try {
      // Get container width from scroll element, fallback to 700px
      const width = scrollElement()?.clientWidth ?? 700;
      const usableWidth = width - PRETEXT_TURN_PADDING;
      const prepared = prepare(text, PRETEXT_FONT);
      const result = layout(prepared, usableWidth, PRETEXT_LINE_HEIGHT);
      const estimated = result.height + PRETEXT_TURN_PADDING;

      // Cap the cache size
      if (heightEstimateCache.size > 500) {
        const keys = heightEstimateCache.keys();
        for (let i = 0; i < 100; i++) {
          heightEstimateCache.delete(keys.next().value as string);
        }
      }
      heightEstimateCache.set(cacheKey, estimated);
      return estimated;
    } catch {
      // Fallback: rough estimate
      return 200;
    }
  };

  // The virtualizer uses the scroll container as its scroll element
  // and only renders visible turns plus an overscan buffer
  // Pretext provides accurate height estimates without DOM measurement
  const virtualizer = createVirtualizer({
    get count() { return turns().length; },
    getScrollElement: () => scrollElement() ?? null,
    estimateSize: (index: number) => estimateTurnHeight(index),
    overscan: 5, // Render 5 extra items above/below viewport for smooth scrolling
    measureElement: (element: Element | null) => {
      if (!element) return 0;
      return (element as HTMLElement).getBoundingClientRect().height;
    },
  });
  
  // T3.3: Get virtual items reactively for rendering
  const virtualItems = () => virtualizer.getVirtualItems();
  const totalSize = () => virtualizer.getTotalSize();
  
  // Phase 5: Scroll anchoring state
  const NEAR_BOTTOM_THRESHOLD_PX = 100;
  let rafScheduled = false;
  let lastTurnCount = 0;

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
  
  // Phase 5: isNearBottom as a function (not a mutable let)
  const isNearBottom = () => {
    if (!scrollContainerRef) return true;
    const { scrollTop, scrollHeight, clientHeight } = scrollContainerRef;
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
    return distanceFromBottom <= NEAR_BOTTOM_THRESHOLD_PX;
  };
  
  // Phase 5: Scroll handler to track user's scroll position
  const handleScroll = () => {
    // No-op: scroll tracking is handled by the scroll event listener added in onMount
  };
  
  // Phase 5: Scroll to bottom function with RAF batching
  const scrollToBottom = () => {
    if (!scrollContainerRef || rafScheduled) return;
    rafScheduled = true;
    requestAnimationFrame(() => {
      if (scrollContainerRef) {
        // Use scrollTop instead of scrollTo for JSDOM compatibility
        scrollContainerRef.scrollTop = scrollContainerRef.scrollHeight;
      }
      rafScheduled = false;
    });
  };
  
  // Phase 5: Scroll to bottom on mount
  onMount(() => {
    scrollToBottom();
    
    // Add scroll event listener to track manual scrolls
    const container = scrollContainerRef;
    if (container) {
      const onScroll = () => {
        // isNearBottom() is called here to check current scroll position
        // We don't need to store it - just triggering the effect is enough
      };
      container.addEventListener('scroll', onScroll);
      onCleanup(() => container.removeEventListener('scroll', onScroll));
    }
  });
  
  // Phase 5: Auto-scroll when turns increase from history load
  createEffect(() => {
    const currentTurnCount = turns().length;
    if (currentTurnCount > lastTurnCount && isNearBottom()) {
      scrollToBottom();
    }
    lastTurnCount = currentTurnCount;
  });
  
  // Phase 5: Auto-scroll on session change
  createEffect(() => {
    const sessionId = props.session.id;
    if (sessionId) {
      lastTurnCount = 0;
      scrollToBottom();
    }
  });
  
  // Phase 5: Auto-scroll during loading when near bottom
  createEffect(() => {
    if (workspaceContext.workspace.isLoading(props.session.id) && draft() && isNearBottom()) {
      scrollToBottom();
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
        // Map API SSEStatus to workspace SSEStatus
        // API: "connected" | "connecting" | "disconnected"
        // Workspace: "idle" | "connecting" | "connected" | "error"
        const mappedStatus = status === "disconnected" ? "idle" : status;
        workspaceContext.setSseStatus(mappedStatus);
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
        // DON'T clear the draft yet! The backend may not have persisted
        // the assistant message yet. We need to:
        // 1. Reload messages from backend
        // 2. If backend has the assistant response → clear draft, mark complete
        // 3. If backend doesn't have it yet → keep draft visible, retry
        const reloadPromise = props.onReloadMessages?.();
        if (reloadPromise instanceof Promise) {
          reloadPromise.then(() => {
            // Give the backend a moment to persist if it hasn't yet.
            // The merge logic in loadMessages will preserve local state
            // if backend is behind. Once messages are loaded, we can
            // safely clear the draft and mark complete.
            clearDraft();
            props.onComplete?.();
          });
        } else {
          clearDraft();
          props.onComplete?.();
        }
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
        // Reload first, then mark complete — same ordering fix as onDone
        const reloadPromise = props.onReloadMessages?.();
        if (reloadPromise instanceof Promise) {
          reloadPromise.then(() => props.onComplete?.());
        } else {
          props.onComplete?.();
        }
      },
    });

    connectedSessionId = sessionId;
    sseClient.connect();
  };
  
  // Keep a session-scoped SSE subscription alive while the session is selected.
  // Clear height cache on session change since turns change completely.
  createEffect(() => {
    const sessionId = props.session.id;
    heightEstimateCache.clear();
    void connectSSE(sessionId);
  });

  // Only clear draft when loading stops AND there's no active SSE connection.
  // This prevents clearing the draft prematurely when:
  // - onDone fires but backend hasn't persisted the response yet
  // - isLoading transitions during SSE reconnection
  createEffect(() => {
    if (!workspaceContext.workspace.isLoading(props.session.id) && !sseClient) {
      clearDraft();
    }
  });

  // SS-1: Initialize optimistic shell immediately when loading starts (before SSE events arrive)
  createEffect(() => {
    if (workspaceContext.workspace.isLoading(props.session.id) && !draft()) {
      // Only init if we don't already have a draft (e.g., from a previous streaming session that was cleared)
      initOptimisticShell(props.session.id);
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
    sessions: workspaceContext.sessions().map((s) => ({ id: s.id, title: s.title || "" })),
    messages: workspaceContext.workspace.getMessages(props.session.id).map((m) => ({ id: m.id, role: m.role, content: m.content ?? "" })),
  };

  const handleCommandResult = (result: { success: boolean; message: string; data?: unknown }) => {
    props.onCommandResult?.(result);
  };

  const checklistItems = () => deriveChecklistItems(workspaceContext.workspace.getMessages(props.session.id));

  return (
    <div class="flex flex-col h-full">
      {/* Chat context bar — sticky top inside center pane */}
      <div data-component="chat-context-bar">
        <span data-context-chip>
          <span class="material-symbols-outlined" style="font-size: 13px;">smart_toy</span>
          {props.currentModel || "assistant"}
        </span>
        <Show when={props.session}>
          <span data-context-chip>
            <span class="material-symbols-outlined" style="font-size: 13px;">description</span>
            {props.session.title || "Untitled"}
          </span>
        </Show>
        <span style="margin-left: auto; display: inline-flex; align-items: center; gap: 4px;">
          <span style={{
            width: "6px",
            height: "6px",
            "border-radius": "50%",
            background: workspaceContext.sseStatus() === "connected" ? "var(--secondary)" : workspaceContext.sseStatus() === "connecting" ? "var(--tertiary)" : "var(--outline)",
          }} />
          {workspaceContext.sseStatus()}
        </span>
      </div>

      {/* Scrollable transcript area */}
      <div
        ref={(el) => {
          scrollContainerRef = el;
          setScrollElement(el);
        }}
        class="flex-1 overflow-y-auto px-4 md:px-8 py-6 custom-scrollbar"
        onScroll={handleScroll}
      >
        <TaskChecklistPanel items={checklistItems()} />
        {/* CT-3: Transcript uses fluid width from CSS variable */}
        <div data-component="transcript" class="w-full">
          <Show when={turns().length === 0 && !draft()} fallback={
            <>
              {/* T3.3: Virtualized turns for efficient rendering when scroll element is available */}
              {/* Falls back to non-virtualized rendering when scroll element is not available (e.g., in tests) */}
              <Show when={isVirtualizationEnabled()} fallback={
                /* Fallback: non-virtualized rendering for tests or when scroll element isn't available */
                <For each={turns()}>
                  {(turn) => (
                    <Show when={turn.role === "system"} fallback={
                      /* Assistant: document-style (full width, transparent) */
                      <Show when={turn.role === "assistant"} fallback={
                        /* User: bubble (right-aligned, contained) */
                        <div
                          class="turn turn--user"
                          data-component="user-bubble-message"
                          data-turn-role={turn.role}
                        >
                          <TurnAvatar role={turn.role} />
                          <div class="turn-content">
                            <For each={turn.messages}>
                              {(message) => (
                                <div data-component="message" data-role={message.role}>
                                  <div data-component="message-content">
                                    <MessageContent message={message} />
                                  </div>
                                </div>
                              )}
                            </For>
                          </div>
                        </div>
                      }>
                        <div
                          class="turn turn--assistant"
                          data-component="assistant-document-message"
                          data-turn-role={turn.role}
                        >
                          <TurnAvatar role={turn.role} />
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
                      </Show>
                    }>
                      {/* System: minimal inline rendering */}
                      <div
                        class="turn turn--system"
                        data-turn-role={turn.role}
                      >
                        <div class="turn-content">
                          <For each={turn.messages}>
                            {(message) => (
                              <div data-component="message" data-role={message.role}>
                                <div data-component="message-header">
                                  <span style="font-size: var(--text-xs); font-weight: 600; color: var(--text-muted);">
                                    system
                                  </span>
                                </div>
                                <div data-component="message-content">
                                  <MessageContent message={message} />
                                </div>
                              </div>
                            )}
                          </For>
                        </div>
                      </div>
                    </Show>
                  )}
                </For>
              }>
                {/* Virtualized rendering - uses absolute positioning */}
                <div 
                  style={{
                    height: `${totalSize()}px`,
                    position: "relative",
                    width: "100%",
                  }}
                >
                  <For each={virtualItems()}>
                    {(virtualItem) => {
                      const turn = () => turns()[virtualItem.index];
                      return (
                        <div
                          style={{
                            position: "absolute",
                            top: 0,
                            left: 0,
                            width: "100%",
                            height: `${virtualItem.size}px`,
                            transform: `translateY(${virtualItem.start}px)`,
                          }}
                          ref={(el) => {
                            // T3.3: Measure element after render for dynamic height
                            requestAnimationFrame(() => {
                              virtualizer.measureElement(el);
                            });
                          }}
                        >
                          {/* T3.3: Render turn content using the same structure as before */}
                          <Show when={turn().role === "system"} fallback={
                            /* Assistant: document-style (full width, transparent) */
                            <Show when={turn().role === "assistant"} fallback={
                              /* User: bubble (right-aligned, contained) */
                              <div
                                class="turn turn--user"
                                data-component="user-bubble-message"
                                data-turn-role={turn().role}
                              >
                                <TurnAvatar role={turn().role} />
                                <div class="turn-content">
                                  <For each={turn().messages}>
                                    {(message) => (
                                      <div data-component="message" data-role={message.role}>
                                        <div data-component="message-content">
                                          <MessageContent message={message} />
                                        </div>
                                      </div>
                                    )}
                                  </For>
                                </div>
                              </div>
                            }>
                              <div
                                class="turn turn--assistant"
                                data-component="assistant-document-message"
                                data-turn-role={turn().role}
                              >
                                <TurnAvatar role={turn().role} />
                                <div class="turn-content">
                                  <For each={turn().messages}>
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
                            </Show>
                          }>
                            {/* System: minimal inline rendering */}
                            <div
                              class="turn turn--system"
                              data-turn-role={turn().role}
                            >
                              <div class="turn-content">
                                <For each={turn().messages}>
                                  {(message) => (
                                    <div data-component="message" data-role={message.role}>
                                      <div data-component="message-header">
                                        <span style="font-size: var(--text-xs); font-weight: 600; color: var(--text-muted);">
                                          system
                                        </span>
                                      </div>
                                      <div data-component="message-content">
                                        <MessageContent message={message} />
                                      </div>
                                    </div>
                                  )}
                                </For>
                              </div>
                            </div>
                          </Show>
                        </div>
                      );
                    }}
                  </For>
                </div>
              </Show>
              {/* T3.3: Draft/streaming turn rendered OUTSIDE the virtualizer as real DOM */}
              {/* Phase 3: Show draft message during streaming as an assistant turn */}
              {/* SS-1: Optimistic shell appears immediately on submit BEFORE any SSE delta */}
              {/* SS-2: Abort button lives inside the shell, not in a separate bottom bar */}
              <Show when={workspaceContext.workspace.isLoading(props.session.id) && draft()}>
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
        disabled={workspaceContext.workspace.isLoading(props.session.id)}
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
    return (
      <div data-component="tool-event-row">
        <div class="tool-event-icon">
          <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' 1;">terminal</span>
        </div>
        <span class="tool-event-name">{props.part.name}</span>
        <ToolCallCard 
          id={props.part.id} 
          name={props.part.name} 
          arguments={props.part.arguments} 
        />
      </div>
    );
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
      {/* Simulate 2 lines of varying width for natural skeleton appearance */}
      <span data-component="skeleton-line" style="width: 65%;">.</span>
      <span data-component="skeleton-line" style="width: 45%;">.</span>
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
