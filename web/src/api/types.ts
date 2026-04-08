// Message types
export type MessageRole = "user" | "assistant" | "system";

// Union of all message part types matching backend Part enum
export type MessagePart =
  | { type: 'text'; content: string }
  | { type: 'reasoning'; content: string }
  | { type: 'tool_call'; id: string; name: string; arguments: unknown }
  | { type: 'tool_result'; tool_call_id: string; content: string; is_error: boolean }
  | { type: 'attachment'; id: string; name: string; mime_type: string; content: unknown };

export interface Message {
  id: string;
  role: MessageRole;
  content: string;
  created_at: string;
  parts?: MessagePart[];  // additive: backend may send parts[]
}

// Session types
export type SessionStatus = "idle" | "running" | "completed";

export interface Session {
  id: string;
  title: string;
  status: SessionStatus;
  updated_at: string;
}

// SSE event types
export type SSEEventType = "message_added" | "streaming_progress" | "agent_finished" | "error";

export interface SSEEvent {
  type: SSEEventType;
  data: unknown;
}

export interface SSEMessageEvent {
  type: "message_added";
  session_id: string;
  message_id: string;
}

export interface SSEDeltaEvent {
  type: "streaming_progress";
  session_id: string;
  accumulated_text: string;
  accumulated_reasoning: string;
}

export interface SSEDoneEvent {
  type: "agent_finished";
  session_id: string;
}

export interface SSEErrorEvent {
  type: "agent_error";
  session_id: string;
  agent_id: string;
  error: string;
}

export type SSEEventData = SSEMessageEvent | SSEDeltaEvent | SSEDoneEvent | SSEErrorEvent;

// Connection status
export type SSEStatus = "connected" | "connecting" | "disconnected";

// SSE Client config
export interface SSEConfig {
  sessionId: string;
  apiBase: string;
  onMessage?: (event: SSEMessageEvent) => void;
  onDelta?: (event: SSEDeltaEvent) => void;
  onDone?: (event: SSEDoneEvent) => void;
  onError?: (event: SSEErrorEvent) => void;
  onStatusChange?: (status: SSEStatus) => void;
}
