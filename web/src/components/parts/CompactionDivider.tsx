import { type Component } from "solid-js";

interface CompactionDividerProps {
  originalCount: number;
  newCount: number;
  tokensSaved: number;
}

/**
 * Visual divider shown in the chat when context compaction occurs.
 * Displays "Context compacted" with original→new message count and tokens saved.
 * Styled as a subtle horizontal divider with info text.
 */
export const CompactionDivider: Component<CompactionDividerProps> = (props) => {
  return (
    <div
      data-component="compaction-divider"
      style={{
        display: "flex",
        "align-items": "center",
        gap: "12px",
        padding: "12px 16px",
        margin: "8px 0",
      }}
    >
      <div
        style={{
          flex: 1,
          height: "1px",
          background: "var(--outline-variant)",
        }}
      />
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "8px",
          padding: "6px 12px",
          background: "var(--surface-container-low)",
          border: "1px solid var(--outline-variant)",
          "border-radius": "16px",
        }}
      >
        <span
          class="material-symbols-outlined"
          style={{
            "font-size": "14px",
            color: "var(--text-secondary)",
          }}
        >
          compression
        </span>
        <span
          style={{
            "font-size": "12px",
            color: "var(--text-secondary)",
            "white-space": "nowrap",
          }}
        >
          Context compacted: {props.originalCount} → {props.newCount} messages ({props.tokensSaved.toLocaleString()} tokens saved)
        </span>
      </div>
      <div
        style={{
          flex: 1,
          height: "1px",
          background: "var(--outline-variant)",
        }}
      />
    </div>
  );
};
