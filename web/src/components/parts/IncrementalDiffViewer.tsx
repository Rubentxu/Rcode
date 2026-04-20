import { type Component, createSignal, Show, createEffect, on } from "solid-js";
import { For } from "solid-js";
import { parseUnifiedDiff, type DiffFile, type DiffHunk, type DiffLine } from "../../api/diff";

// Props for the IncrementalDiffViewer component
export interface IncrementalDiffViewerProps {
  // Session ID to scope diff accumulation - diffs are cleared when session changes
  sessionId: string;
  // Callback to register a chunk handler with the parent (SessionView)
  onRegisterChunkHandler: (handler: (diffId: string, content: string, done: boolean) => void) => void;
}

/**
 * IncrementalDiffViewer displays streaming diff chunks from SSE.
 * It shows a progress indicator while chunks are accumulating and
 * renders the complete diff once all chunks have been received.
 *
 * Diff accumulation is scoped by sessionId - the diffMap is cleared
 * when the session changes to prevent stale chunks from persisting.
 */
export const IncrementalDiffViewer: Component<IncrementalDiffViewerProps> = (props) => {
  // Track all active diffs by diff_id
  const [diffMap, setDiffMap] = createSignal<Map<string, { content: string; chunkCount: number; done: boolean }>>(new Map());

  // CRITICAL 2 FIX: Clear diffMap when session changes to prevent stale diff content
  // This uses `on()` to track sessionId changes and reset accumulation
  createEffect(on(() => props.sessionId, (_newSessionId, _prevSessionId) => {
    if (_prevSessionId !== undefined && _prevSessionId !== _newSessionId) {
      // Session changed - clear accumulated diffs from previous session
      console.debug("[IncrementalDiffViewer] Session changed, clearing diffMap");
      setDiffMap(new Map());
    }
  }));

  // The first diff that has completed (done === true)
  const completedDiffId = () => {
    for (const [id, data] of diffMap()) {
      if (data.done) return id;
    }
    return null;
  };

  // The completed diff content (parsed)
  const completedDiffFiles = (): DiffFile[] => {
    const id = completedDiffId();
    if (!id) return [];
    const data = diffMap().get(id);
    if (!data) return [];
    return parseUnifiedDiff(data.content);
  };

  // Total chunks across all in-progress diffs (for progress indicator)
  const totalChunks = () => {
    let total = 0;
    for (const [, data] of diffMap()) {
      total += data.chunkCount;
    }
    return total;
  };

  // Is any diff currently streaming (not done)?
  const isStreaming = () => {
    for (const [, data] of diffMap()) {
      if (!data.done) return true;
    }
    return diffMap().size === 0;
  };

  // Chunk handler that parent (SessionView) calls when SSE diff_chunk events arrive
  const handleChunk = (diffId: string, content: string, done: boolean) => {
    setDiffMap((prev) => {
      const next = new Map(prev);
      const existing = next.get(diffId);
      if (existing) {
        next.set(diffId, {
          content: existing.content + content,
          chunkCount: existing.chunkCount + 1,
          done,
        });
      } else {
        next.set(diffId, { content, chunkCount: 1, done });
      }
      return next;
    });
  };

  // Register our chunk handler with the parent on mount
  createEffect(() => {
    props.onRegisterChunkHandler(handleChunk);
  });

  return (
    <div class="incremental-diff-viewer my-2">
      <Show
        when={completedDiffId()}
        fallback={
          <Show when={diffMap().size > 0}>
            <div class="flex items-center gap-2 px-3 py-2 bg-surface-container-low rounded-lg border border-outline-variant/30">
              <span class="material-symbols-outlined text-primary text-sm animate-spin" style={{ "font-size": "16px" }}>
                progress_activity
              </span>
              <span class="text-xs text-on-surface-variant">
                Receiving diff... {totalChunks()} chunk(s) received
              </span>
            </div>
          </Show>
        }
      >
        {/* Completed diff rendered using existing DiffViewer pattern */}
        <CompletedDiffView diffFiles={completedDiffFiles()} />
      </Show>
    </div>
  );
};

// Internal component to render the completed diff
interface CompletedDiffViewProps {
  diffFiles: DiffFile[];
}

const CompletedDiffView: Component<CompletedDiffViewProps> = (props) => {
  const [isCollapsed, setIsCollapsed] = createSignal(false);

  const totalChanges = () => {
    let additions = 0;
    let deletions = 0;
    for (const file of props.diffFiles) {
      for (const hunk of file.hunks) {
        for (const line of hunk.lines) {
          if (line.type === "add") additions++;
          else if (line.type === "remove") deletions++;
        }
      }
    }
    return { additions, deletions };
  };

  return (
    <div class="diff-viewer" data-collapsed={isCollapsed()}>
      <button
        class="diff-collapse-toggle flex items-center gap-2 px-3 py-2 bg-surface-container-low rounded-lg border border-outline-variant/30 hover:bg-surface-container-high transition-colors cursor-pointer w-full"
        onClick={() => setIsCollapsed(!isCollapsed())}
      >
        <span class="diff-collapse-icon text-xs text-on-surface-variant">
          {isCollapsed() ? "▶" : "▼"}
        </span>
        <span class="material-symbols-outlined text-secondary" style={{ "font-size": "14px" }}>
          merge_type
        </span>
        <span class="text-xs font-semibold text-on-surface-variant">
          Diff complete
        </span>
        <span class="text-xs text-outline ml-auto">
          {props.diffFiles.length} file(s),{" "}
          <span class="text-secondary">+{totalChanges().additions}</span>,{" "}
          <span class="text-error">-{totalChanges().deletions}</span>
        </span>
      </button>

      <Show when={!isCollapsed()}>
        <div class="diff-content mt-1">
          <For each={props.diffFiles}>
            {(file) => <FileDiffView file={file} />}
          </For>
        </div>
      </Show>
    </div>
  );
};

interface FileDiffViewProps {
  file: DiffFile;
}

const FileDiffView: Component<FileDiffViewProps> = (props) => {
  const fileChanges = () => {
    let additions = 0;
    let deletions = 0;
    for (const hunk of props.file.hunks) {
      for (const line of hunk.lines) {
        if (line.type === "add") additions++;
        else if (line.type === "remove") deletions++;
      }
    }
    return { additions, deletions };
  };

  return (
    <div class="diff-file border border-outline-variant/20 rounded-lg overflow-hidden mb-2">
      <div class="diff-file-header flex items-center gap-2 px-3 py-2 bg-surface-container-high">
        <span class="text-xs font-medium text-on-surface-variant">{props.file.filename}</span>
        <span class="text-xs text-outline ml-auto">
          <span class="text-secondary">+{fileChanges().additions}</span>,{" "}
          <span class="text-error">-{fileChanges().deletions}</span>
        </span>
      </div>
      <div class="diff-file-content">
        <For each={props.file.hunks}>
          {(hunk) => <HunkDiffView hunk={hunk} />}
        </For>
      </div>
    </div>
  );
};

interface HunkDiffViewProps {
  hunk: DiffHunk;
}

const HunkDiffView: Component<HunkDiffViewProps> = (props) => {
  return (
    <div class="diff-hunk">
      <div class="diff-hunk-header px-3 py-1 bg-surface-container-lowest text-xs text-outline font-mono">
        @@ -{props.hunk.oldStart},{props.hunk.oldLines} +{props.hunk.newStart},{props.hunk.newLines} @@
      </div>
      <div class="diff-hunk-content font-mono text-xs">
        <For each={props.hunk.lines}>
          {(line) => <DiffLineView line={line} />}
        </For>
      </div>
    </div>
  );
};

interface DiffLineViewProps {
  line: DiffLine;
}

const DiffLineView: Component<DiffLineViewProps> = (props) => {
  const lineClass = () => {
    if (props.line.type === "add") return "bg-secondary-container/30 text-secondary";
    if (props.line.type === "remove") return "bg-error-container/30 text-error";
    return "text-on-surface-variant";
  };

  const prefix = () => {
    if (props.line.type === "add") return "+";
    if (props.line.type === "remove") return "-";
    return " ";
  };

  return (
    <div class={`px-3 py-0.5 ${lineClass()}`}>
      <span class="inline-block w-6 text-right mr-2 select-none">{prefix()}</span>
      {props.line.content}
    </div>
  );
};
