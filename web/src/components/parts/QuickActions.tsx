import { type Component, createSignal, Show } from "solid-js";
import { showToast } from "../Toast";

export interface QuickActionsProps {
  messageId: string;
  textContent: string;
  onRetry: (messageId: string) => void;
  onBranch: (messageId: string) => void;
}

/**
 * QuickActions - Per-message action buttons (copy, retry)
 * Rendered inline beside the message content.
 * 
 * MQA-4: Actions appear on hover/focus of the message row
 * MQA-5: Copy shows transient "Copied!" feedback via Toast
 * MQA-6: Retry removes the assistant response and re-submits the preceding user prompt
 */
export const QuickActions: Component<QuickActionsProps> = (props) => {
  const [copied, setCopied] = createSignal(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(props.textContent);
      setCopied(true);
      showToast({ type: "success", message: "Copied!", duration: 2000 });
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
      showToast({ type: "error", message: "Failed to copy", duration: 2000 });
    }
  };

  const handleRetry = () => {
    props.onRetry(props.messageId);
  };

  return (
    <div
      class="quick-actions"
      data-component="message-actions"
      role="group"
      aria-label="Message actions"
    >
      {/* Copy */}
      <button
        class="quick-action-btn"
        data-action="copy"
        onClick={handleCopy}
        aria-label="Copy message"
        title="Copy"
        disabled={copied()}
      >
        <Show when={copied()} fallback={
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2"/>
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>
          </svg>
        }>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="20 6 9 17 4 12"/>
          </svg>
        </Show>
      </button>

      {/* Retry */}
      <button
        class="quick-action-btn"
        data-action="retry"
        onClick={handleRetry}
        aria-label="Retry this response"
        title="Retry"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 2v6h-6"/>
          <path d="M3 12a9 9 0 0 1 15-6.7L21 8"/>
          <path d="M3 22v-6h6"/>
          <path d="M21 12a9 9 0 0 1-15 6.7L3 16"/>
        </svg>
      </button>
    </div>
  );
};
