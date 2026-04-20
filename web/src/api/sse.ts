import type {
  SSEConfig,
  SSEStatus,
  SSEEventData,
  SSEMessageEvent,
  SSEDeltaEvent,
  SSEDoneEvent,
  SSEErrorEvent,
  SSEStreamTextDelta,
  SSEStreamReasoningDelta,
  SSEStreamToolCallStart,
  SSEStreamToolCallArg,
  SSEStreamToolCallEnd,
  SSEStreamToolResult,
  SSEStreamAssistantCommitted,
  ToolErrorEvent,
  ProviderErrorEvent,
  SSEPermissionRequestedEvent,
  SSECompactionPerformedEvent,
  SSEDiffChunkEvent,
} from "./types";

const RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_DELAY_MS = 30000;

export class SSEClient {
  private eventSource: EventSource | null = null;
  private config: SSEConfig;
  private status: SSEStatus = "disconnected";
  private reconnectAttempts = 0;
  private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;

  constructor(config: SSEConfig) {
    this.config = config;
  }

  connect(): void {
    if (this.eventSource) {
      this.eventSource.close();
    }

    this.setStatus("connecting");

    const url = `${this.config.apiBase}/session/${this.config.sessionId}/events`;
    
    try {
      this.eventSource = new EventSource(url);

      this.eventSource.onopen = () => {
        this.reconnectAttempts = 0;
        console.info("SSE connected", { sessionId: this.config.sessionId, url });
        this.setStatus("connected");
      };

      this.eventSource.onerror = () => {
        console.warn("SSE transport error", { sessionId: this.config.sessionId, url });
        this.handleError();
      };

      // Handle custom event types from the SSE endpoint
      this.eventSource.addEventListener("message_added", (event: MessageEvent) => {
        this.handleEvent("message_added", event.data);
      });

      this.eventSource.addEventListener("streaming_progress", (event: MessageEvent) => {
        this.handleEvent("streaming_progress", event.data);
      });

      this.eventSource.addEventListener("agent_finished", (event: MessageEvent) => {
        this.handleEvent("agent_finished", event.data);
      });

      this.eventSource.addEventListener("agent_error", (event: MessageEvent) => {
        this.handleEvent("agent_error", event.data);
      });

      // Phase 3: New semantic event types
      this.eventSource.addEventListener("stream_text_delta", (event: MessageEvent) => {
        this.handleEvent("stream_text_delta", event.data);
      });

      this.eventSource.addEventListener("stream_reasoning_delta", (event: MessageEvent) => {
        this.handleEvent("stream_reasoning_delta", event.data);
      });

      this.eventSource.addEventListener("stream_tool_call_start", (event: MessageEvent) => {
        this.handleEvent("stream_tool_call_start", event.data);
      });

      this.eventSource.addEventListener("stream_tool_call_args_delta", (event: MessageEvent) => {
        this.handleEvent("stream_tool_call_args_delta", event.data);
      });

      this.eventSource.addEventListener("stream_tool_call_end", (event: MessageEvent) => {
        this.handleEvent("stream_tool_call_end", event.data);
      });

      this.eventSource.addEventListener("stream_tool_result", (event: MessageEvent) => {
        this.handleEvent("stream_tool_result", event.data);
      });

      this.eventSource.addEventListener("stream_assistant_committed", (event: MessageEvent) => {
        this.handleEvent("stream_assistant_committed", event.data);
      });

      // T-03: Error event listeners
      this.eventSource.addEventListener("tool_error", (event: MessageEvent) => {
        this.handleEvent("tool_error", event.data);
      });

      this.eventSource.addEventListener("provider_error", (event: MessageEvent) => {
        this.handleEvent("provider_error", event.data);
      });

      // Phase 1: Permission prompt events
      this.eventSource.addEventListener("permission_requested", (event: MessageEvent) => {
        this.handleEvent("permission_requested", event.data);
      });

      // Phase 1: Compaction events
      this.eventSource.addEventListener("compaction_performed", (event: MessageEvent) => {
        this.handleEvent("compaction_performed", event.data);
      });

      // Phase 3: Diff chunk event
      this.eventSource.addEventListener("diff_chunk", (event: MessageEvent) => {
        this.handleEvent("diff_chunk", event.data);
      });

    } catch (error) {
      console.error("Failed to create EventSource:", error);
      this.handleError();
    }
  }

  disconnect(): void {
    this.clearReconnectTimeout();
    
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    
    this.setStatus("disconnected");
  }

  getStatus(): SSEStatus {
    return this.status;
  }

  private setStatus(status: SSEStatus): void {
    if (this.status !== status) {
      this.status = status;
      this.config.onStatusChange?.(status);
    }
  }

  private handleEvent(type: string, rawData: string): void {
    try {
      const data = JSON.parse(rawData) as SSEEventData;
      console.debug("SSE event received", { sessionId: this.config.sessionId, type, data });
      
      switch (type) {
        case "message_added":
          this.config.onMessage?.(data as SSEMessageEvent);
          break;
        case "streaming_progress":
          this.config.onDelta?.(data as SSEDeltaEvent);
          break;
        case "agent_finished":
          this.config.onDone?.(data as SSEDoneEvent);
          break;
        case "agent_error":
          this.config.onError?.(data as SSEErrorEvent);
          break;
        // Phase 3: New semantic event types
        case "stream_text_delta":
          this.config.onTextDelta?.(data as SSEStreamTextDelta);
          break;
        case "stream_reasoning_delta":
          this.config.onReasoningDelta?.(data as SSEStreamReasoningDelta);
          break;
        case "stream_tool_call_start":
          this.config.onToolCallStart?.(data as SSEStreamToolCallStart);
          break;
        case "stream_tool_call_args_delta":
          this.config.onToolCallArg?.(data as SSEStreamToolCallArg);
          break;
        case "stream_tool_call_end":
          this.config.onToolCallEnd?.(data as SSEStreamToolCallEnd);
          break;
        case "stream_tool_result":
          this.config.onToolResult?.(data as SSEStreamToolResult);
          break;
        case "stream_assistant_committed":
          this.config.onAssistantCommitted?.(data as SSEStreamAssistantCommitted);
          break;
        // T-03: Error event types
        case "tool_error":
          this.config.onToolError?.(data as ToolErrorEvent);
          break;
        case "provider_error":
          this.config.onProviderError?.(data as ProviderErrorEvent);
          break;
        // Phase 1: Permission prompt events
        case "permission_requested":
          this.config.onPermissionRequested?.(data as SSEPermissionRequestedEvent);
          break;
        // Phase 1: Compaction events
        case "compaction_performed":
          this.config.onCompactionPerformed?.(data as SSECompactionPerformedEvent);
          break;
        // Phase 3: Diff chunk event
        case "diff_chunk":
          this.config.onDiffChunk?.(data as SSEDiffChunkEvent);
          break;
        default:
          console.warn("Unknown SSE event type:", type);
      }
    } catch (error) {
      console.error("Failed to parse SSE event data:", error);
    }
  }

  private handleError(): void {
    console.warn("SSE disconnected, scheduling reconnect", {
      sessionId: this.config.sessionId,
      reconnectAttempts: this.reconnectAttempts,
    });
    this.eventSource?.close();
    this.eventSource = null;
    this.setStatus("disconnected");
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    this.clearReconnectTimeout();
    
    const delay = Math.min(
      RECONNECT_DELAY_MS * Math.pow(2, this.reconnectAttempts),
      MAX_RECONNECT_DELAY_MS
    );

    this.reconnectTimeout = setTimeout(() => {
      this.reconnectAttempts++;
      console.info("SSE reconnecting", {
        sessionId: this.config.sessionId,
        reconnectAttempts: this.reconnectAttempts,
        delay,
      });
      this.connect();
    }, delay);
  }

  private clearReconnectTimeout(): void {
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }
  }
}

// Factory function for creating SSE client with hooks
export function createSSEClient(config: SSEConfig): SSEClient {
  return new SSEClient(config);
}
