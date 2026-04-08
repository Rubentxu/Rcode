import { For, Show, Switch, Match, createSignal, onCleanup, createEffect, createMemo } from "solid-js";
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

interface SessionViewProps {
  session: Session;
  messages: Message[];
  isLoading: boolean;
  sseStatus: "connected" | "connecting" | "disconnected";
  onSubmit: (prompt: string) => void;
  onAbort: () => void;
  onSSEStatusChange?: (status: "connected" | "connecting" | "disconnected") => void;
  sessions: Session[];
  onCommandResult?: (result: { success: boolean; message: string; data?: unknown }) => void;
  onComplete?: () => void;
  onReloadMessages?: () => void;
  onError?: (error: string) => void;
}

export default function SessionView(props: SessionViewProps) {
  let sseClient: SSEClient | null = null;
  let connectedSessionId: string | null = null;
  const [streamingContent, setStreamingContent] = createSignal<string>("");
  
  // Create a derived message list that includes streaming content
  const displayMessages = () => {
    const msgs = [...props.messages];
    const lastMsg = msgs[msgs.length - 1];
    if (props.isLoading && streamingContent()) {
      if (lastMsg && lastMsg.role === "assistant") {
        msgs[msgs.length - 1] = {
          ...lastMsg,
          content: lastMsg.content + streamingContent(),
        };
      } else {
        msgs.push({
          id: "streaming-assistant",
          role: "assistant" as const,
          content: streamingContent(),
          created_at: new Date().toISOString(),
        });
      }
    }
    return msgs;
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
      onDelta: (event) => {
        // Backend sends the full accumulated text, so replace instead of append.
        setStreamingContent(event.accumulated_text);
      },
      onMessage: () => {
        props.onReloadMessages?.();
      },
      onDone: () => {
        setStreamingContent("");
        props.onReloadMessages?.();
        props.onComplete?.();
      },
      onError: (event) => {
        console.error("SSE error:", event.error);
        setStreamingContent("");
        props.onError?.(event.error);
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
    if (!props.isLoading) {
      setStreamingContent("");
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
    messages: props.messages.map((m) => ({ id: m.id, role: m.role, content: m.content })),
  };

  const handleCommandResult = (result: { success: boolean; message: string; data?: unknown }) => {
    props.onCommandResult?.(result);
  };

  return (
    <div style="display: flex; flex-direction: column; height: 100%;">
      <header style="height: 56px; border-bottom: 1px solid var(--border); display: flex; align-items: center; padding: 0 var(--space-4); background: var(--bg-secondary);">
        <h1 style="font-size: var(--text-lg); font-weight: 600; flex: 1;">
          {props.session.title || "New Session"}
        </h1>
        <ConnectionStatus status={props.sseStatus} />
      </header>
      
      <div style="flex: 1; overflow-y: auto; padding: var(--space-4);">
        <Show when={displayMessages().length === 0} fallback={
          <For each={displayMessages()}>
            {(message) => (
              <div data-component="message" data-role={message.role}>
                <div data-component="message-header">
                  <span style="font-size: var(--text-xs); font-weight: 600; text-transform: uppercase; color: var(--text-muted);">
                    {message.role}
                  </span>
                  <span style="font-size: var(--text-xs); color: var(--text-muted);">
                    {new Date(message.created_at).toLocaleTimeString()}
                  </span>
                </div>
                <div data-component="message-content">
                  <MessageContent message={message} />
                </div>
              </div>
            )}
          </For>
        }>
          <div data-component="empty-state" style="height: 200px;">
            <p data-component="empty-state-description">
              Start a conversation by typing a message below
            </p>
          </div>
        </Show>
      </div>

      <Show when={props.isLoading}>
        <div style="display: flex; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-4); border-top: 1px solid var(--border);">
          <div data-component="typing-indicator">
            <span data-component="typing-dot"></span>
            <span data-component="typing-dot"></span>
            <span data-component="typing-dot"></span>
          </div>
          <span style="font-size: var(--text-sm); color: var(--text-secondary);">Processing...</span>
          <button data-component="button" data-variant="abort" onClick={props.onAbort} style="margin-left: auto;">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
              <rect x="4" y="4" width="16" height="16" rx="2"/>
            </svg>
            Stop
          </button>
        </div>
      </Show>

      <PromptInput
        onSubmit={props.onSubmit}
        onCommand={handleCommandResult}
        disabled={props.isLoading}
        context={commandContext}
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
      fallback={<LegacyContent content={props.message.content} />}
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

function ConnectionStatus(props: { status: "connected" | "connecting" | "disconnected" }) {
  const statusColors = {
    connected: "var(--success)",
    connecting: "var(--warning)",
    disconnected: "var(--error)",
  };

  return (
    <div style="display: flex; align-items: center; gap: var(--space-2);">
      <span 
        data-component="status-dot" 
        data-status={props.status}
        style={{
          width: "8px",
          height: "8px",
          "border-radius": "50%",
          "background-color": statusColors[props.status],
        }}
      />
      <span style="font-size: var(--text-xs); color: var(--text-muted);">{props.status}</span>
    </div>
  );
}
