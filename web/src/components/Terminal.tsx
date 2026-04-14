import { createSignal, createEffect, onMount, For, Show } from "solid-js";
import {
  executeCommand,
  type TerminalOutput,
  parseCommand,
  isCommonCommand,
} from "../api/terminal";

interface TerminalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface OutputLine {
  id: string;
  type: "stdout" | "stderr" | "command" | "exit" | "system";
  content: string;
  timestamp: number;
}

export default function Terminal(props: TerminalProps) {
  const [output, setOutput] = createSignal<OutputLine[]>([]);
  const [command, setCommand] = createSignal("");
  const [history, setHistory] = createSignal<string[]>([]);
  const [historyIndex, setHistoryIndex] = createSignal(-1);
  const [isExecuting, setIsExecuting] = createSignal(false);
  const [panelHeight, setPanelHeight] = createSignal(300);

  let outputRef: HTMLDivElement | undefined;
  let inputRef: HTMLInputElement | undefined;
  let isResizing = false;
  let startY = 0;
  let startHeight = 0;

  // Auto-scroll to bottom when output changes
  createEffect(() => {
    output(); // dependency
    if (outputRef) {
      outputRef.scrollTop = outputRef.scrollHeight;
    }
  });

  // Focus input when terminal opens
  createEffect(() => {
    if (props.isOpen && inputRef) {
      inputRef.focus();
    }
  });

  const addOutput = (line: OutputLine) => {
    setOutput((prev) => [...prev, line]);
  };

  const executeCurrentCommand = async () => {
    const cmd = command().trim();
    if (!cmd || isExecuting()) return;

    // Add command to output
    addOutput({
      id: crypto.randomUUID(),
      type: "command",
      content: `$ ${cmd}`,
      timestamp: Date.now(),
    });

    // Add to history
    setHistory((prev) => [cmd, ...prev.slice(0, 49)]);
    setHistoryIndex(-1);
    setCommand("");
    setIsExecuting(true);

    try {
      await executeCommand(cmd, {
        onOutput: (termOutput) => {
          addOutput({
            id: crypto.randomUUID(),
            type: termOutput.type,
            content: termOutput.data,
            timestamp: termOutput.timestamp,
          });
        },
      });
    } catch (error) {
      addOutput({
        id: crypto.randomUUID(),
        type: "stderr",
        content: `Error: ${error}`,
        timestamp: Date.now(),
      });
    } finally {
      setIsExecuting(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      executeCurrentCommand();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      navigateHistory(1);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      navigateHistory(-1);
    } else if (e.key === "l" && e.ctrlKey) {
      e.preventDefault();
      clearOutput();
    } else if (e.key === "c" && e.ctrlKey) {
      e.preventDefault();
      addOutput({
        id: crypto.randomUUID(),
        type: "command",
        content: `$ ${command()} ^C`,
        timestamp: Date.now(),
      });
      setCommand("");
    }
  };

  const navigateHistory = (direction: number) => {
    const hist = history();
    if (hist.length === 0) return;

    let newIndex = historyIndex() + direction;
    if (newIndex < 0) newIndex = 0;
    if (newIndex >= hist.length) {
      setHistoryIndex(-1);
      setCommand("");
      return;
    }

    setHistoryIndex(newIndex);
    setCommand(hist[newIndex]);
  };

  const clearOutput = () => {
    setOutput([]);
  };

  const startResize = (e: MouseEvent) => {
    isResizing = true;
    startY = e.clientY;
    startHeight = panelHeight();
    document.addEventListener("mousemove", handleResize);
    document.addEventListener("mouseup", stopResize);
  };

  const handleResize = (e: MouseEvent) => {
    if (!isResizing) return;
    const delta = startY - e.clientY;
    const newHeight = Math.min(Math.max(150, startHeight + delta), 600);
    setPanelHeight(newHeight);
  };

  const stopResize = () => {
    isResizing = false;
    document.removeEventListener("mousemove", handleResize);
    document.removeEventListener("mouseup", stopResize);
  };

  const getHighlightedCommand = () => {
    const { head, args } = parseCommand(command());
    if (!head) return <span>{command()}</span>;

    const isCommon = isCommonCommand(head);
    return (
      <>
        <span data-terminal-command={isCommon ? "common" : "other"}>{head}</span>
        {args.length > 0 && <span> {args.join(" ")}</span>}
      </>
    );
  };

  return (
    <Show when={props.isOpen}>
      <div
        data-terminal
        role="region"
        aria-label="Terminal"
        style={{
          height: `${panelHeight()}px`,
          "border-top": "1px solid var(--border)",
          background: "var(--bg-primary)",
          display: "flex",
          "flex-direction": "column",
        }}
      >
        {/* Terminal Header */}
        <div
          data-terminal-header
          style={{
            display: "flex",
            "align-items": "center",
            padding: "0 var(--space-3)",
            height: "36px",
            background: "var(--bg-secondary)",
            "border-bottom": "1px solid var(--border)",
            cursor: "ns-resize",
            "user-select": "none",
          }}
          onMouseDown={startResize}
        >
          <div
            style={{
              display: "flex",
              gap: "var(--space-2)",
              "align-items": "center",
            }}
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              style={{ color: "var(--text-muted)" }}
            >
              <polyline points="4 17 10 11 4 5" />
              <line x1="12" y1="19" x2="20" y2="19" />
            </svg>
            <span
              style={{
                "font-size": "var(--text-xs)",
                "font-family": "var(--font-mono)",
                color: "var(--text-secondary)",
              }}
            >
              Terminal
            </span>
          </div>
          <div style={{ "margin-left": "auto", display: "flex", gap: "var(--space-2)" }}>
            <button
              data-component="button"
              data-variant="ghost"
              onClick={clearOutput}
              aria-label="Clear terminal"
              style={{ padding: "4px 8px", "font-size": "var(--text-xs)" }}
            >
              <svg
                width="12"
                height="12"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M3 6h18M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2" />
              </svg>
            </button>
            <button
              data-component="button"
              data-variant="ghost"
              onClick={props.onClose}
              aria-label="Close terminal"
              style={{ padding: "4px 8px", "font-size": "var(--text-xs)" }}
            >
              <svg
                width="12"
                height="12"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>
        </div>

        {/* Terminal Output */}
        <div
          ref={outputRef}
          data-terminal-output
          role="log"
          aria-live="polite"
          aria-label="Terminal output"
          style={{
            flex: 1,
            overflow: "auto",
            padding: "var(--space-3)",
            "font-family": "var(--font-mono)",
            "font-size": "var(--text-sm)",
            "line-height": 1.5,
          }}
        >
          <Show
            when={output().length > 0}
            fallback={
              <div
                style={{
                  color: "var(--text-muted)",
                  "font-style": "italic",
                }}
              >
                Terminal ready. Type a command to execute.
              </div>
            }
          >
            <For each={output()}>
              {(line) => (
                <div
                  data-terminal-line={line.type}
                  style={{
                    color:
                      line.type === "stderr"
                        ? "var(--error)"
                        : line.type === "command"
                          ? "var(--text-primary)"
                          : line.type === "exit"
                            ? "var(--text-muted)"
                            : line.type === "system"
                              ? "var(--info)"
                              : "var(--text-secondary)",
                    "white-space": "pre-wrap",
                    "word-break": "break-all",
                  }}
                >
                  <Show when={line.type === "command"}>
                    <span style={{ color: "var(--accent)" }}>
                      {line.content}
                    </span>
                  </Show>
                  <Show when={line.type !== "command"}>
                    {line.content}
                  </Show>
                </div>
              )}
            </For>
          </Show>
          <Show when={isExecuting()}>
            <div
              data-terminal-line="executing"
              style={{
                color: "var(--warning)",
                "white-space": "pre-wrap",
              }}
            >
              <span style={{ display: "inline-block", animation: "terminal-blink 1s infinite" }}>
                ▋
              </span>
            </div>
          </Show>
        </div>

        {/* Terminal Input */}
        <div
          data-terminal-input
          style={{
            display: "flex",
            "align-items": "center",
            padding: "var(--space-2) var(--space-3)",
            "border-top": "1px solid var(--border)",
            background: "var(--bg-secondary)",
          }}
        >
          <span
            style={{
              "font-family": "var(--font-mono)",
              "font-size": "var(--text-sm)",
              color: "var(--accent)",
              "margin-right": "var(--space-2)",
            }}
          >
            $
          </span>
          <input
            ref={inputRef}
            type="text"
            value={command()}
            onInput={(e) => setCommand(e.currentTarget.value)}
            onKeyDown={handleKeyDown}
            disabled={isExecuting()}
            aria-label="Terminal command input"
            style={{
              flex: 1,
              background: "transparent",
              border: "none",
              outline: "none",
              "font-family": "var(--font-mono)",
              "font-size": "var(--text-sm)",
              color: "var(--text-primary)",
            }}
            spellcheck={false}
            autocomplete="off"
          />
        </div>
      </div>
    </Show>
  );
}
