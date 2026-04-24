import { For, Show, Switch, Match, createSignal, onCleanup, createEffect, createMemo, on } from "solid-js";
import type { Session } from "../stores";
import type { Message, MessagePart, SSEPermissionRequestedEvent, SSECompactionPerformedEvent, CompactionRecord, SSEDiffChunkEvent } from "../api/types";
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
import { PermissionPrompt } from "./parts/PermissionPrompt";
import { CompactionDivider } from "./parts/CompactionDivider";
import { IncrementalDiffViewer } from "./parts/IncrementalDiffViewer";
import TaskChecklistPanel from "./chat-workspace/TaskChecklistPanel";
import { deriveChecklistItems } from "./chat-workspace/checklist";
import { createDraftStore, type DraftPart } from "../composables/useStreamingDraft";
import type {
  SSEStreamReasoningDelta,
  SSEStreamTextDelta,
  SSEStreamToolCallArg,
  SSEStreamToolCallEnd,
  SSEStreamToolCallStart,
  SSEStreamToolResult,
  ToolErrorEvent,
  ProviderErrorEvent,
} from "../api/types";
import { useWorkspace } from "../context/WorkspaceContext";
import { showToast } from "./Toast";
import { createChatAutoScroll } from "../hooks/createChatAutoScroll";
import { JumpToBottomButton } from "./JumpToBottomButton";

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
  onModelChange?: (modelId: string) => void;
  currentAgent?: string | null;
  onAgentChange?: (agentId: string | null) => void;
  onTerminalToggle?: () => void;
}

export default function SessionView(props: SessionViewProps) {
  // Get workspace context for messages, loading state, SSE status, and sessions
  const workspaceContext = useWorkspace();
  
  let sseClient: SSEClient | null = null;
  let connectedSessionId: string | null = null;
  let scrollContainerRef: HTMLDivElement | undefined;
  
  // Phase 3: Draft store for streaming parts
  const { draft, dispatch, clear: clearDraft, initOptimisticShell } = createDraftStore();

  // Phase 1: Permission request queue - multiple permissions can arrive while modal is open
  const [permissionQueue, setPermissionQueue] = createSignal<SSEPermissionRequestedEvent[]>([]);

  // Phase 1: Compaction records for the current session
  const [compactionRecords, setCompactionRecords] = createSignal<CompactionRecord[]>([]);

  // MEDIUM 1 FIX: Clear permission queue when session changes to prevent stale permissions
  createEffect(on(() => props.session.id, (_newId, _prevId) => {
    if (_prevId !== undefined && _prevId !== _newId) {
      console.debug("[SessionView] Session changed, clearing permission queue");
      setPermissionQueue([]);
    }
  }));

  // Phase 3: Ref to hold the diff chunk handler registered by IncrementalDiffViewer
  let diffChunkHandlerRef: ((diffId: string, content: string, done: boolean) => void) | null = null;
  
  // Create a derived message list that includes persisted messages from workspace
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

  // Auto-scroll: ref to draft element for scrollIntoView
  let draftElementRef: HTMLElement | undefined;

  const autoScroll = createChatAutoScroll({
    scrollEl: () => scrollContainerRef,
    virtualizer: () => undefined,
    draftRef: () => draftElementRef,
    bottomThreshold: 48,
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

  // Phase 1: Permission queue handlers - called by PermissionPrompt modal
  const handlePermissionGrant = (_request_id: string) => {
    setPermissionQueue((prev) => prev.slice(1));
  };

  const handlePermissionDeny = (_request_id: string) => {
    setPermissionQueue((prev) => prev.slice(1));
  };

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
        // DO NOT reload on every message_added event.
        // The user message was already appended optimistically, and reloading
        // here calls setMessages() which — even without the loadingState reset —
        // causes unnecessary network churn while the assistant is still streaming.
        // We reload once in onAssistantCommitted / onDone when streaming is complete.
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
      // T-05: Error event callbacks
      onToolError: (event: ToolErrorEvent) => {
        showToast({ type: "error", message: `Tool error: ${event.error}` });
      },
      onProviderError: (event: ProviderErrorEvent) => {
        showToast({ type: "error", message: `Provider error: ${event.error}` });
      },
      // Phase 1: Permission prompt callback - enqueue incoming permission requests
      onPermissionRequested: (event: SSEPermissionRequestedEvent) => {
        console.info("Permission requested:", event);
        setPermissionQueue((prev) => [...prev, event]);
      },
      // Phase 1: Compaction callback - record compaction events for the session
      onCompactionPerformed: (event: SSECompactionPerformedEvent) => {
        console.info("Compaction performed:", event);
        if (event.session_id === sessionId) {
          setCompactionRecords((prev) => [
            ...prev,
            {
              session_id: event.session_id,
              original_count: event.original_count,
              new_count: event.new_count,
              tokens_saved: event.tokens_saved,
              timestamp: Date.now(),
            },
          ]);
        }
      },
      // Phase 3: Diff chunk callback - forward to IncrementalDiffViewer via ref
      // CRITICAL 3: Handle both `done` (current impl) and `is_final` (spec) defensively
      onDiffChunk: (event: SSEDiffChunkEvent) => {
        console.info("Diff chunk received:", event);
        if (event.session_id === sessionId && diffChunkHandlerRef) {
          // Support both done and is_final - prefer is_final if present, fall back to done
          const isDone = event.is_final ?? event.done ?? false;
          diffChunkHandlerRef(event.diff_id, event.content, isDone);
        }
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
    <>
      {/* Phase 1: Permission prompt modal - shown when there's a pending permission request */}
      <Show when={permissionQueue().length > 0}>
        <PermissionPrompt
          request={permissionQueue()[0]}
          onGrant={handlePermissionGrant}
          onDeny={handlePermissionDeny}
          onAutoDeny={handlePermissionDeny}
        />
      </Show>

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
        <span style="margin-left: auto; display: inline-flex; align-items: center; gap: 4px;" aria-label={`SSE status: ${workspaceContext.sseStatus()}`}>
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
        ref={(el) => { scrollContainerRef = el; }}
        data-component="chat-scroll-container"
        class="flex-1 overflow-y-auto px-4 md:px-8 py-3 custom-scrollbar relative"
        onScroll={autoScroll.handleScroll}
        role="log"
        aria-label="Chat transcript"
      >
        <TaskChecklistPanel items={checklistItems()} />
        {/* CT-3: Transcript uses fluid width from CSS variable */}
        <div data-component="transcript" class="w-full">
          <Show when={turns().length === 0 && !draft()} fallback={
            <>
              {/* Simple flow layout — no virtualizer, no absolute positioning */}
              <For each={turns()}>
                {(turn) => (
                  <Show when={turn.role === "system"} fallback={
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
                      {/* Assistant: document-style (full width, transparent) */}
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
                                <Show when={message.role === "system"}>
                                  <div data-component="message-header">
                                    <span style="font-size: var(--text-xs); font-weight: 600; color: var(--text-muted);">
                                      system
                                    </span>
                                  </div>
                                </Show>
                                <div data-component="message-content" class="message-content-with-actions">
                                  <MessageContent message={message} />
                                </div>
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
                    <div class="turn turn--system" data-turn-role={turn.role}>
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
              {/* Draft/streaming turn rendered after persisted turns */}
              {/* Phase 3: Show draft message during streaming as an assistant turn */}
              {/* SS-1: Optimistic shell appears immediately on submit BEFORE any SSE delta */}
              {/* SS-2: Abort button lives inside the shell, not in a separate bottom bar */}
              {/* FIX: Show draft whenever it exists — NOT gated on isLoading.
                   Previously: isLoading && draft() caused the draft to vanish the moment
                   onMessage triggered a setMessages() reload that reset loadingState to idle,
                   even though SSE tokens were still arriving. Now: draft() drives visibility;
                   clearDraft() is only called on terminal events (onDone, onAssistantCommitted,
                   onError) so the draft persists for the full streaming lifetime. */}
              <Show when={draft()}>
                <div
                  class="turn turn--assistant"
                  data-turn-role="assistant"
                  data-streaming={draft()?.isOptimistic ? "optimistic" : "streaming"}
                  ref={el => draftElementRef = el as HTMLElement}
                >
                  <TurnAvatar role="assistant" />
                  <div class="turn-content">
                    <div data-component="message" data-role="assistant">
                      {/* SS-2: Enhanced shell header with clearer state/content separation */}
                      <div data-component="shell-header">
                        <div data-component="shell-header-label">
                          <span class="material-symbols-outlined" style="font-size: 14px;">smart_toy</span>
                          <span>Assistant</span>
                        </div>
                        <div data-component="shell-status">
                          <Show when={!draft()?.isOptimistic} fallback={
                            <>
                              <span data-component="streaming-dot" />
                              <span>Thinking...</span>
                            </>
                          }>
                            <span data-component="streaming-dot" />
                            <span>Streaming...</span>
                          </Show>
                        </div>
                        {/* SS-2: Stop button integrated in shell header */}
                        <button
                          data-component="shell-abort"
                          onClick={props.onAbort}
                          aria-label="Stop generation"
                        >
                          <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
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
              <div data-component="empty-state" role="status" aria-live="polite" style="height: 200px;">
                <p data-component="empty-state-description">
                  Start a conversation by typing a message below
                </p>
              </div>
            </Show>
          </Show>
        </div>

        {/* Phase 1: Compaction dividers - shown when there are compaction records */}
        <Show when={compactionRecords().length > 0}>
          <For each={compactionRecords()}>
            {(record) => (
              <CompactionDivider
                originalCount={record.original_count}
                newCount={record.new_count}
                tokensSaved={record.tokens_saved}
              />
            )}
          </For>
        </Show>

        {/* Phase 3: Incremental diff viewer for streaming diff chunks */}
        <IncrementalDiffViewer
          sessionId={props.session.id}
          onRegisterChunkHandler={(handler) => {
            diffChunkHandlerRef = handler;
          }}
        />

        {/* JumpToBottomButton — anchored to scroll container via absolute positioning */}
        <JumpToBottomButton
          isVisible={() => !autoScroll.isFollowing()}
          onClick={() => autoScroll.resume()}
        />
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
        onModelChange={props.onModelChange}
        currentAgent={props.currentAgent}
        onAgentChange={props.onAgentChange}
        onTerminalToggle={props.onTerminalToggle}
      />
    </div>
    </>
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
    <div data-component="structured-parts" role="group" aria-label="Message parts">
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
      <div data-component="tool-event-row" aria-label={`tool: ${props.part.name}`}>
        <div class="tool-event-icon">
          <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' 1;">terminal</span>
        </div>
        <span class="tool-event-name">{props.part.name}</span>
        <ToolCallCard 
          id={props.part.id} 
          name={props.part.name} 
          arguments={props.part.arguments} 
          source={props.part.source}
        />
      </div>
    );
  }
  
  if (partType === "tool_result") {
    return <ToolResultCard 
      tool_call_id={props.part.tool_call_id} 
      content={props.part.content} 
      is_error={props.part.is_error} 
      truncated={props.part.truncated}
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
        source={props.part.source}
      />
    );
  }
  
  if (partType === "tool_result") {
    return <ToolResultCard 
      tool_call_id={props.part.tool_call_id} 
      content={props.part.content} 
      is_error={props.part.is_error} 
      truncated={props.part.truncated}
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
