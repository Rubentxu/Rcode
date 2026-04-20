import { type Component, createSignal, onCleanup, Show } from "solid-js";
import { showToast } from "../Toast";
import { getApiBase } from "../../api/config";

interface PermissionRequest {
  request_id: string;
  tool_name: string;
  description: string;
}

interface PermissionPromptProps {
  request: PermissionRequest;
  onGrant: (request_id: string) => void;
  onDeny: (request_id: string) => void;
  onAutoDeny: (request_id: string) => void;
}

/**
 * Modal that displays a permission request from the backend.
 * Shows tool name and description with Grant and Deny buttons.
 * Auto-deny after 60 seconds timeout.
 */
export const PermissionPrompt: Component<PermissionPromptProps> = (props) => {
  const [isGranting, setIsGranting] = createSignal(false);
  const [isDenying, setIsDenying] = createSignal(false);

  // Auto-deny timer - 60 seconds per spec R1.4
  let autoDenyTimer: ReturnType<typeof setTimeout> | null = null;

  const startAutoDenyTimer = () => {
    autoDenyTimer = setTimeout(() => {
      handleDeny(true);
    }, 60000); // 60 seconds
  };

  const clearAutoDenyTimer = () => {
    if (autoDenyTimer) {
      clearTimeout(autoDenyTimer);
      autoDenyTimer = null;
    }
  };

  onCleanup(() => {
    clearAutoDenyTimer();
  });

  // Start timer on mount
  startAutoDenyTimer();

  const handleGrant = async () => {
    if (isGranting() || isDenying()) return;
    setIsGranting(true);
    clearAutoDenyTimer();

    try {
      const apiBase = await getApiBase();
      const response = await fetch(`${apiBase}/permission/${props.request.request_id}/grant`, {
        method: "POST",
      });

      if (!response.ok) {
        showToast({ type: "error", message: "Failed to grant permission" });
        setIsGranting(false);
        return;
      }

      props.onGrant(props.request.request_id);
    } catch (error) {
      console.error("Failed to grant permission:", error);
      showToast({ type: "error", message: "Failed to grant permission" });
      setIsGranting(false);
    }
  };

  const handleDeny = async (isAutoDeny = false) => {
    if (isGranting() || isDenying()) return;
    setIsDenying(true);
    clearAutoDenyTimer();

    try {
      const apiBase = await getApiBase();
      const response = await fetch(`${apiBase}/permission/${props.request.request_id}/deny`, {
        method: "POST",
      });

      if (!response.ok) {
        showToast({ type: "error", message: "Failed to deny permission" });
        setIsDenying(false);
        return;
      }

      if (isAutoDeny) {
        showToast({ type: "info", message: "Permission auto-denied (timeout)" });
      }

      props.onDeny(props.request.request_id);
    } catch (error) {
      console.error("Failed to deny permission:", error);
      showToast({ type: "error", message: "Failed to deny permission" });
      setIsDenying(false);
    }
  };

  return (
    <div
      data-component="permission-prompt-overlay"
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0, 0, 0, 0.6)",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        "z-index": 9998,
      }}
      onClick={(e) => {
        // Don't close on overlay click - user must explicitly deny
        if (e.target === e.currentTarget) {
          e.stopPropagation();
        }
      }}
    >
      <div
        data-component="permission-prompt-modal"
        style={{
          background: "var(--surface-container-high)",
          border: "1px solid var(--outline-variant)",
          "border-radius": "12px",
          padding: "24px",
          "max-width": "480px",
          width: "90%",
          "box-shadow": "0 8px 32px rgba(0, 0, 0, 0.4)",
        }}
      >
        {/* Header */}
        <div
          data-component="permission-prompt-header"
          style={{
            display: "flex",
            "align-items": "center",
            gap: "12px",
            "margin-bottom": "16px",
          }}
        >
          <span
            class="material-symbols-outlined"
            style={{
              "font-size": "24px",
              color: "var(--primary)",
            }}
          >
            shield
          </span>
          <h2
            style={{
              margin: 0,
              "font-size": "18px",
              "font-weight": 600,
              color: "var(--text-primary)",
            }}
          >
            Permission Required
          </h2>
        </div>

        {/* Tool Info */}
        <div
          data-component="permission-prompt-tool-info"
          style={{
            "margin-bottom": "20px",
          }}
        >
          <div
            style={{
              "margin-bottom": "12px",
            }}
          >
            <span
              style={{
                "font-size": "12px",
                "font-weight": 600,
                color: "var(--text-secondary)",
                "text-transform": "uppercase",
                "letter-spacing": "0.5px",
              }}
            >
              Tool
            </span>
            <p
              style={{
                margin: "4px 0 0 0",
                "font-size": "16px",
                color: "var(--text-primary)",
                "font-family": "monospace",
              }}
            >
              {props.request.tool_name}
            </p>
          </div>

          <div>
            <span
              style={{
                "font-size": "12px",
                "font-weight": 600,
                color: "var(--text-secondary)",
                "text-transform": "uppercase",
                "letter-spacing": "0.5px",
              }}
            >
              Description
            </span>
            <p
              style={{
                margin: "4px 0 0 0",
                "font-size": "14px",
                color: "var(--text-secondary)",
                "line-height": 1.5,
              }}
            >
              {props.request.description}
            </p>
          </div>
        </div>

        {/* Actions */}
        <div
          data-component="permission-prompt-actions"
          style={{
            display: "flex",
            gap: "12px",
            "justify-content": "flex-end",
          }}
        >
          <button
            data-component="permission-deny-btn"
            onClick={() => handleDeny(false)}
            disabled={isGranting() || isDenying()}
            style={{
              padding: "10px 20px",
              "border-radius": "8px",
              "font-size": "14px",
              "font-weight": 500,
              cursor: isGranting() || isDenying() ? "not-allowed" : "pointer",
              background: "transparent",
              border: "1px solid var(--outline-variant)",
              color: "var(--text-secondary)",
              opacity: isGranting() || isDenying() ? 0.6 : 1,
              transition: "all 0.15s ease",
            }}
          >
            Deny
          </button>
          <button
            data-component="permission-grant-btn"
            onClick={handleGrant}
            disabled={isGranting() || isDenying()}
            style={{
              padding: "10px 20px",
              "border-radius": "8px",
              "font-size": "14px",
              "font-weight": 500,
              cursor: isGranting() || isDenying() ? "not-allowed" : "pointer",
              background: "var(--primary)",
              border: "none",
              color: "var(--on-primary)",
              opacity: isGranting() || isDenying() ? 0.6 : 1,
              transition: "all 0.15s ease",
            }}
          >
            <Show when={isGranting()} fallback="Allow Once">
              Allowing...
            </Show>
          </button>
        </div>

        {/* Timeout indicator */}
        <div
          style={{
            "margin-top": "16px",
            "text-align": "center",
            "font-size": "12px",
            color: "var(--text-muted)",
          }}
        >
          Auto-deny in 60 seconds
        </div>
      </div>
    </div>
  );
};
