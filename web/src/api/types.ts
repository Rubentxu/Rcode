// =============================================================================
// Auto-generated types from Rust backend - DO NOT EDIT
// This file re-exports from generated-types.ts which is regenerated on changes
// to crates/core/src/message.rs
// =============================================================================
export * from './generated-types';

// Wire-format Role type (lowercase as sent over network)
// The Rust enum uses uppercase variants (User/Assistant/System) but serde
// serializes them as lowercase (user/assistant/system) via #[serde(rename_all = "lowercase")]
// We need this alias for code that expects the wire format
export type WireRole = "user" | "assistant" | "system";

// Backward-compatible alias for frontend code expecting the old MessageRole name
export type MessageRole = WireRole;

// Wire-format Message type with backward-compatible fields
// The Rust Message uses parts: Part[] but frontend historically used content: string
// We provide content as an optional computed field for compatibility
// session_id is optional because frontend creates messages before session association
export interface WireMessage {
  id: string;
  session_id?: string;  // Optional: frontend creates messages before session association
  role: WireRole;
  parts?: import('./generated-types').Part[];
  // Backward compat: first text part as content string
  content?: string;
  created_at: string;
}

// Backward-compatible Message type alias (same as WireMessage)
export type Message = WireMessage;

// Session types (defined in Rust session.rs, kept here for frontend use)
export type SessionStatus = "idle" | "running" | "completed";

export interface Session {
  id: string;
  title: string;
  status: SessionStatus;
  updated_at: string;
}

// Backward-compatible Part type with arguments included as unknown
// The Rust Part.tool_call.arguments is Box<serde_json::Value> which can't be
// represented in TypeScript, so we use unknown
// This is the type the frontend should use for received/sent messages
export type MessagePart = 
  | { type: 'text'; content: string }
  | { type: 'reasoning'; content: string }
  | { type: 'tool_call'; id: string; name: string; arguments?: unknown }
  | { type: 'tool_result'; tool_call_id: string; content: string; is_error: boolean }
  | { type: 'task_checklist'; items: { id: string; content: string; status: string; priority: string }[] }
  | { type: 'attachment'; id: string; name: string; mime_type: string; content?: unknown };

// =============================================================================
// SSE event types - these are defined in the server-side event system
// They are NOT auto-generated from Rust because the SSE event schema is
// defined in the TypeScript/SSE client code, not in Rust types
// =============================================================================

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

// Phase 3: New semantic SSE event types
// Derived from: crates/event/src/bus.rs Event enum streaming variants
// PSOT-5: These are manually synced with Rust. Full codegen from Event enum
// requires ts-rs support for #[serde(tag = "type")] internally tagged enums.
export interface SSEStreamTextDelta {
  type: "stream_text_delta";
  session_id: string;
  delta: string;
}

// Legacy: full accumulated text from streaming_progress events
// Unlike stream_text_delta (which appends), this REPLACES the text content
export interface SSEStreamTextSnapshot {
  type: "stream_text_snapshot";
  session_id: string;
  accumulated_text: string;
}

export interface SSEStreamReasoningDelta {
  type: "stream_reasoning_delta";
  session_id: string;
  delta: string;
}

export interface SSEStreamToolCallStart {
  type: "stream_tool_call_start";
  session_id: string;
  tool_call_id: string;
  name: string;
}

export interface SSEStreamToolCallArg {
  type: "stream_tool_call_args_delta";
  session_id: string;
  tool_call_id: string;
  value: string;
}

export interface SSEStreamToolCallEnd {
  type: "stream_tool_call_end";
  session_id: string;
  tool_call_id: string;
}

export interface SSEStreamToolResult {
  type: "stream_tool_result";
  session_id: string;
  tool_call_id: string;
  content: string;
  is_error: boolean;
}

export interface SSEStreamAssistantCommitted {
  type: "stream_assistant_committed";
  session_id: string;
}

export type SSEEventData = SSEMessageEvent | SSEDeltaEvent | SSEDoneEvent | SSEErrorEvent 
  | SSEStreamTextDelta | SSEStreamReasoningDelta | SSEStreamToolCallStart 
  | SSEStreamToolCallArg | SSEStreamToolCallEnd | SSEStreamToolResult 
  | SSEStreamAssistantCommitted;

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
  // Phase 3: New semantic event callbacks
  onTextDelta?: (event: SSEStreamTextDelta) => void;
  onReasoningDelta?: (event: SSEStreamReasoningDelta) => void;
  onToolCallStart?: (event: SSEStreamToolCallStart) => void;
  onToolCallArg?: (event: SSEStreamToolCallArg) => void;
  onToolCallEnd?: (event: SSEStreamToolCallEnd) => void;
  onToolResult?: (event: SSEStreamToolResult) => void;
  onAssistantCommitted?: (event: SSEStreamAssistantCommitted) => void;
}
