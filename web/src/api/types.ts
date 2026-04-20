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
  | { type: 'tool_call'; id: string; name: string; arguments?: unknown; source?: string }
  | { type: 'tool_result'; tool_call_id: string; content: string; is_error: boolean; truncated?: boolean }
  | { type: 'task_checklist'; items: { id: string; content: string; status: string; priority: string }[] }
  | { type: 'attachment'; id: string; name: string; mime_type: string; content?: unknown };

// Phase 2: Pending attachment for drag-drop/paste in PromptInput
export interface PendingAttachment {
  id: string;
  file: File;
  name: string;
  size: number;
  mime_type: string;
  preview_url?: string;
}

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

// T-03: New semantic SSE event types for errors
export interface ToolErrorEvent {
  type: "tool_error";
  session_id: string;
  tool_id: string;
  error: string;
  duration_ms?: number;
}

export interface ProviderErrorEvent {
  type: "provider_error";
  provider_id: string;
  error: string;
}

// Phase 1: Permission prompt SSE event
export interface SSEPermissionRequestedEvent {
  type: "permission_requested";
  request_id: string;
  tool_name: string;
  description: string;
}

// Phase 1: Compaction SSE event
export interface SSECompactionPerformedEvent {
  type: "compaction_performed";
  session_id: string;
  original_count: number;
  new_count: number;
  tokens_saved: number;
}

// Phase 1: Compaction record for local state
export interface CompactionRecord {
  session_id: string;
  original_count: number;
  new_count: number;
  tokens_saved: number;
  timestamp: number;
}

// Phase 3: Diff chunk SSE event
// CRITICAL 3: Backend may send either `done` or `is_final` - we support both defensively
export interface SSEDiffChunkEvent {
  type: "diff_chunk";
  session_id: string;
  diff_id: string;
  content: string;
  // Support both field names - spec says `is_final`, implementation uses `done`
  done?: boolean;
  is_final?: boolean;
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
  | SSEStreamAssistantCommitted
  // T-03: Error event types
  | ToolErrorEvent | ProviderErrorEvent
  // Phase 1: Permission and compaction events
  | SSEPermissionRequestedEvent | SSECompactionPerformedEvent
  // Phase 3: Diff chunk event
  | SSEDiffChunkEvent;

// Provider protocol type
export type ProviderProtocol = "openai_compat" | "anthropic_compat" | "google";

// Auth state for providers - derived exclusively via resolve_auth()
export interface AuthState {
  connected: boolean;
  source: 'auth_json' | 'env' | 'config' | 'none';
  kind: 'api_key' | 'oauth' | 'env' | 'none';
  label: string;
  env_key?: string;
  can_disconnect: boolean;
}

// Model auth info from GET /models
export interface ModelAuthInfo {
  connected: boolean;
  source: 'auth_json' | 'env' | 'config' | 'none';
  badge: string | null;
}

// Provider info from GET /config/providers
export interface ProviderInfo {
  id: string;
  name: string;
  display_name: string;
  protocol: ProviderProtocol;
  native: boolean;
  supports_custom_base_url: boolean;
  auth: AuthState;
  base_url: string | null;
  enabled: boolean;
  models_count: number;
  configured?: boolean; // true when provider is set up and ready to use
}

// Model info from GET /models (extends backend model with protocol info)
export interface ModelInfo {
  id: string;
  provider: string;
  display_name?: string;
  protocol?: ProviderProtocol;
  is_compatible?: boolean;
  catalog_source: 'api' | 'fallback' | 'config';
  auth: ModelAuthInfo;
  enabled: boolean;
}

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
  // T-03: Error event callbacks
  onToolError?: (event: ToolErrorEvent) => void;
  onProviderError?: (event: ProviderErrorEvent) => void;
  // Phase 1: Permission prompt callbacks
  onPermissionRequested?: (event: SSEPermissionRequestedEvent) => void;
  // Phase 1: Compaction callbacks
  onCompactionPerformed?: (event: SSECompactionPerformedEvent) => void;
  // Phase 3: Diff chunk callback
  onDiffChunk?: (event: SSEDiffChunkEvent) => void;
}
