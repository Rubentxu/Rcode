import { createSignal, onCleanup, Show, For } from "solid-js";

export interface Toast {
  id: string;
  type: "error" | "success" | "info" | "warning";
  message: string;
  duration?: number;
}

const [toasts, setToasts] = createSignal<Toast[]>([]);

export function showToast(toast: Omit<Toast, "id">) {
  const id = crypto.randomUUID();
  const newToast: Toast = { ...toast, id };
  setToasts((prev) => [...prev, newToast]);
  
  // Auto-remove after duration (default 5s)
  const duration = toast.duration ?? 5000;
  if (duration > 0) {
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, duration);
  }
  
  return id;
}

export function dismissToast(id: string) {
  setToasts((prev) => prev.filter((t) => t.id !== id));
}

export function clearAllToasts() {
  setToasts([]);
}

export function ToastContainer() {
  const getIcon = (type: Toast["type"]) => {
    switch (type) {
      case "error": return "✕";
      case "success": return "✓";
      case "warning": return "⚠";
      case "info": return "ℹ";
    }
  };

  const getBgColor = (type: Toast["type"]) => {
    switch (type) {
      case "error": return "rgba(239, 68, 68, 0.15)";
      case "success": return "rgba(34, 197, 94, 0.15)";
      case "warning": return "rgba(234, 179, 8, 0.15)";
      case "info": return "rgba(59, 130, 246, 0.15)";
    }
  };

  const getBorderColor = (type: Toast["type"]) => {
    switch (type) {
      case "error": return "rgba(239, 68, 68, 0.4)";
      case "success": return "rgba(34, 197, 94, 0.4)";
      case "warning": return "rgba(234, 179, 8, 0.4)";
      case "info": return "rgba(59, 130, 246, 0.4)";
    }
  };

  const getTextColor = (type: Toast["type"]) => {
    switch (type) {
      case "error": return "var(--error)";
      case "success": return "var(--success)";
      case "warning": return "var(--warning)";
      case "info": return "#3b82f6";
    }
  };

  return (
    <div
      style={{
        position: "fixed",
        bottom: "80px",
        right: "20px",
        display: "flex",
        "flex-direction": "column",
        gap: "8px",
        "z-index": "9999",
        "max-width": "380px",
      }}
    >
      <For each={toasts()}>
        {(toast) => (
          <div
            style={{
              display: "flex",
              "align-items": "flex-start",
              gap: "10px",
              padding: "12px 14px",
              background: getBgColor(toast.type),
              border: `1px solid ${getBorderColor(toast.type)}`,
              "border-radius": "8px",
              "box-shadow": "0 4px 12px rgba(0, 0, 0, 0.3)",
              animation: "slideIn 0.2s ease-out",
            }}
          >
            <span
              style={{
                "font-size": "14px",
                "font-weight": "600",
                color: getTextColor(toast.type),
                "flex-shrink": "0",
                width: "18px",
                "text-align": "center",
              }}
            >
              {getIcon(toast.type)}
            </span>
            <div style={{ flex: "1", "min-width": "0" }}>
              <p
                style={{
                  margin: "0",
                  "font-size": "13px",
                  color: "var(--text-primary)",
                  "line-height": "1.4",
                  "word-break": "break-word",
                }}
              >
                {toast.message}
              </p>
            </div>
            <button
              onClick={() => dismissToast(toast.id)}
              style={{
                background: "none",
                border: "none",
                padding: "0",
                cursor: "pointer",
                color: "var(--text-secondary)",
                "font-size": "12px",
                "flex-shrink": "0",
                opacity: "0.7",
              }}
              onMouseEnter={(e) => (e.currentTarget.style.opacity = "1")}
              onMouseLeave={(e) => (e.currentTarget.style.opacity = "0.7")}
            >
              ✕
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
