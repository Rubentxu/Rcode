/**
 * PromptInput - Text input component with slash command and @mention support
 */

import {
  createSignal,
  createEffect,
  Show,
  For,
  onCleanup,
  type Component,
} from "solid-js";
import {
  findCommands,
  parseCommand,
  executeCommand,
  type Command,
  type CommandContext,
} from "../commands";
import {
  searchFiles,
  type FileResult,
  getFileIcon,
  getFileIconColor,
} from "../mentions/file-search";
import ChatContextFooter from "./chat-workspace/ChatContextFooter";
import {
  shouldShowAutocomplete,
  getMentionQuery,
  getPartialMentionAtCursor,
} from "../mentions/index";
import CommandPalette from "./CommandPalette";
import { type PendingAttachment } from "../api/types";
import { showToast } from "./Toast";

interface PromptInputProps {
  onSubmit: (prompt: string, attachments?: PendingAttachment[]) => void;
  onCommand: (result: { success: boolean; message: string; data?: unknown }) => void;
  disabled?: boolean;
  context: CommandContext;
  currentModel?: string;
}

interface AutocompleteState {
  visible: boolean;
  query: string;
  results: FileResult[];
  selectedIndex: number;
  loading: boolean;
}

export default function PromptInput(props: PromptInputProps) {
  let textareaRef: HTMLTextAreaElement | undefined;
  let wrapperRef: HTMLDivElement | undefined;

  const [inputValue, setInputValue] = createSignal("");
  const [showPalette, setShowPalette] = createSignal(false);
  const [palettePosition, setPalettePosition] = createSignal({ top: 0, left: 0 });
  const [commandQuery, setCommandQuery] = createSignal("");
  const [matchedCommands, setMatchedCommands] = createSignal<Command[]>([]);

  // @mention autocomplete state
  const [autocomplete, setAutocomplete] = createSignal<AutocompleteState>({
    visible: false,
    query: "",
    results: [],
    selectedIndex: 0,
    loading: false,
  });

  // Phase 2: Pending attachments for drag-drop/paste
  const [pendingAttachments, setPendingAttachments] = createSignal<PendingAttachment[]>([]);
  const [isDragging, setIsDragging] = createSignal(false);

  const MAX_FILE_SIZE = 10 * 1024 * 1024; // 10MB

  let searchTimeout: number | undefined;

  // Cleanup
  onCleanup(() => {
    if (searchTimeout) clearTimeout(searchTimeout);
    // Phase 2: Revoke object URLs for pending attachments
    for (const att of pendingAttachments()) {
      if (att.preview_url) URL.revokeObjectURL(att.preview_url);
    }
  });

  // Update command matches when query changes
  createEffect(() => {
    const query = commandQuery();
    if (query.startsWith("/")) {
      const prefix = query.slice(1);
      setMatchedCommands(findCommands(prefix));
    } else {
      setMatchedCommands([]);
    }
  });

  // Position palette near textarea
  const updatePalettePosition = () => {
    if (!textareaRef) return;

    const rect = textareaRef.getBoundingClientRect();
    setPalettePosition({
      top: rect.bottom + 4,
      left: rect.left,
    });
  };

  const handleInput = (e: Event) => {
    const target = e.target as HTMLTextAreaElement;
    const value = target.value;
    setInputValue(value);

    const cursorPos = target.selectionStart ?? 0;
    const textBeforeCursor = value.slice(0, cursorPos);

    // Detect @mention trigger
    if (shouldShowAutocomplete(value, cursorPos)) {
      const query = getMentionQuery(value, cursorPos);
      setAutocomplete(prev => ({
        ...prev,
        visible: true,
        query,
        selectedIndex: 0,
        loading: true,
      }));

      setShowPalette(false);
      setCommandQuery("");

      // Debounce search
      if (searchTimeout) clearTimeout(searchTimeout);
      searchTimeout = window.setTimeout(async () => {
        const results = await searchFiles(query, 8);
        setAutocomplete(prev => ({
          ...prev,
          results,
          loading: false,
        }));
      }, 150);
      return;
    } else {
      setAutocomplete(prev => ({ ...prev, visible: false }));
    }

    // Detect slash command trigger
    const slashMatch = textBeforeCursor.match(/(?:^|\s)\/([^\s]*)$/);

    if (slashMatch) {
      const query = slashMatch[0].startsWith(" ")
        ? `/${slashMatch[1]}`
        : `/${slashMatch[1]}`;
      setCommandQuery(query);
      setShowPalette(true);
      updatePalettePosition();
    } else {
      setShowPalette(false);
      setCommandQuery("");
    }
  };

  const handleSelectCommand = (command: Command) => {
    if (!textareaRef) return;

    // Replace the command in the input
    const value = inputValue();
    const cursorPos = textareaRef.selectionStart;
    const textBeforeCursor = value.slice(0, cursorPos);
    const textAfterCursor = value.slice(cursorPos);

    // Find and replace the command
    const slashMatch = textBeforeCursor.match(/(?:^|\s)\/[^\s]*$/);
    if (slashMatch) {
      const beforeCommand = textBeforeCursor.slice(
        0,
        slashMatch.index !== undefined ? slashMatch.index : 0
      );
      const newValue = beforeCommand + `/${command.name}` + textAfterCursor;
      setInputValue(newValue);

      // Move cursor after command
      setTimeout(() => {
        if (textareaRef) {
          const newCursorPos = beforeCommand.length + command.name.length + 1;
          textareaRef.setSelectionRange(newCursorPos, newCursorPos);
          textareaRef.focus();
        }
      }, 0);
    }

    setShowPalette(false);
    setCommandQuery("");
  };

  /**
   * Insert selected file as mention
   */
  const insertMention = (file: FileResult) => {
    const text = inputValue();
    const cursorPosition = textareaRef?.selectionStart ?? 0;
    const mentionInfo = getPartialMentionAtCursor(text, cursorPosition);

    if (!mentionInfo) return;

    // Replace @query with @path
    const before = text.slice(0, mentionInfo.startIndex);
    const after = text.slice(cursorPosition);
    const newText = `${before}@${file.path}${after}`;

    setInputValue(newText);
    setAutocomplete(prev => ({ ...prev, visible: false }));

    // Set cursor after the inserted mention
    requestAnimationFrame(() => {
      if (textareaRef) {
        const newPosition = mentionInfo.startIndex + file.path.length + 1;
        textareaRef.selectionStart = newPosition;
        textareaRef.selectionEnd = newPosition;
        textareaRef.focus();
      }
    });
  };

  const handleSubmit = async () => {
    const value = inputValue().trim();
    if (!value || props.disabled) return;

    // Check if it's a command
    if (value.startsWith("/")) {
      const parsed = parseCommand(value);
      if (parsed) {
        const result = await executeCommand(parsed.name, { name: parsed.rawArgs }, props.context);
        props.onCommand(result);

        if (result.success) {
          // Clear input on successful command
          setInputValue("");
          // Clear pending attachments on successful command
          clearPendingAttachments();
        }
        return;
      }
    }

    // Regular prompt submission - pass current attachments
    // NOTE: Backend currently doesn't support file attachments (PromptRequest only accepts { prompt, model_id })
    // Attachments are captured here for future backend support
    const attachments = pendingAttachments();
    props.onSubmit(value, attachments);
    setInputValue("");
    // Clear pending attachments after submit
    clearPendingAttachments();
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    const ac = autocomplete();

    // Handle @mention autocomplete navigation
    if (ac.visible) {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          e.stopPropagation();
          setAutocomplete(prev => ({
            ...prev,
            selectedIndex: Math.min(prev.selectedIndex + 1, prev.results.length - 1),
          }));
          return;

        case "ArrowUp":
          e.preventDefault();
          e.stopPropagation();
          setAutocomplete(prev => ({
            ...prev,
            selectedIndex: Math.max(prev.selectedIndex - 1, 0),
          }));
          return;

        case "Enter":
          e.preventDefault();
          e.stopPropagation();
          if (!ac.loading && ac.results.length > 0) {
            insertMention(ac.results[ac.selectedIndex]);
            return;
          }
          break;

        case "Escape":
          e.preventDefault();
          e.stopPropagation();
          setAutocomplete(prev => ({ ...prev, visible: false }));
          return;

        case "Tab":
          if (!ac.loading && ac.results.length > 0) {
            e.preventDefault();
            e.stopPropagation();
            insertMention(ac.results[ac.selectedIndex]);
            return;
          }
          break;
      }
    }

    // If palette is open and visible, handle navigation
    if (showPalette() && matchedCommands().length > 0) {
      if (["ArrowDown", "ArrowUp", "Enter", "Escape"].includes(e.key)) {
        // Let CommandPalette handle these
        return;
      }
    }

    // Submit on Enter (without Shift)
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void handleSubmit();
    }
  };

  const handleClose = () => {
    setShowPalette(false);
    setCommandQuery("");
    textareaRef?.focus();
  };

  // Phase 2: Drag-drop handlers
  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer?.types.includes("Files")) {
      setIsDragging(true);
    }
  };

  const handleDragLeave = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    // Only set to false if we're leaving the wrapper entirely
    const rect = wrapperRef?.getBoundingClientRect();
    if (rect) {
      const { clientX, clientY } = e;
      if (
        clientX < rect.left ||
        clientX > rect.right ||
        clientY < rect.top ||
        clientY > rect.bottom
      ) {
        setIsDragging(false);
      }
    }
  };

  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);

    const files = e.dataTransfer?.files;
    if (files) {
      addFiles(Array.from(files));
    }
  };

  const handlePaste = (e: ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;

    for (const item of items) {
      if (item.kind === "file") {
        const file = item.getAsFile();
        if (file) {
          addFiles([file]);
        }
      }
    }
  };

  const addFiles = (files: File[]) => {
    for (const file of files) {
      if (file.size > MAX_FILE_SIZE) {
        showToast({
          type: "error",
          message: `File too large: ${file.name} (max 10MB)`,
          duration: 4000,
        });
        continue;
      }

      const id = crypto.randomUUID();
      const preview_url = file.type.startsWith("image/")
        ? URL.createObjectURL(file)
        : undefined;

      const attachment: PendingAttachment = {
        id,
        file,
        name: file.name,
        size: file.size,
        mime_type: file.type,
        preview_url,
      };

      setPendingAttachments((prev) => [...prev, attachment]);
    }
  };

  const removeAttachment = (id: string) => {
    setPendingAttachments((prev) => {
      const att = prev.find((a) => a.id === id);
      if (att?.preview_url) {
        URL.revokeObjectURL(att.preview_url);
      }
      return prev.filter((a) => a.id !== id);
    });
  };

  // Clear all pending attachments and revoke object URLs
  const clearPendingAttachments = () => {
    const attachments = pendingAttachments();
    for (const att of attachments) {
      if (att.preview_url) {
        URL.revokeObjectURL(att.preview_url);
      }
    }
    setPendingAttachments([]);
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  };

  return (
    <div
      ref={wrapperRef}
      class="px-4 md:px-8 py-4 bg-surface-container-lowest"
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {/* Phase 2: Drop zone overlay */}
      <Show when={isDragging()}>
        <div class="absolute inset-0 z-10 flex items-center justify-center rounded-xl bg-primary/10 border-2 border-dashed border-primary pointer-events-none">
          <div class="flex flex-col items-center gap-2 text-primary">
            <span class="material-symbols-outlined text-4xl">file_download</span>
            <span class="text-sm font-semibold">Drop to attach</span>
          </div>
        </div>
      </Show>

      <div class="w-full max-w-[var(--transcript-max-width)] mx-auto relative">
        <div class="bg-surface-container rounded-xl p-4 focus-within:ring-2 focus-within:ring-primary/60 focus-within:shadow-[0_0_0_2px_rgba(46,91,255,0.15)] transition-all duration-200" style="border: 1px solid var(--outline-variant);"
          onFocusIn={(e) => {
            const el = e.currentTarget as HTMLElement;
            el.style.borderColor = "var(--primary)";
          }}
          onFocusOut={(e) => {
            const el = e.currentTarget as HTMLElement;
            el.style.borderColor = "var(--outline-variant)";
          }}
        >
          <div class="relative">
            <textarea
              ref={textareaRef}
              data-component="textarea"
              aria-label="Message input"
              aria-multiline="true"
              class="w-full bg-transparent border-none text-on-surface placeholder:text-on-surface-variant/40 resize-none focus:ring-0 text-sm leading-relaxed min-h-[80px]"
              placeholder="Type a message, / for commands, @ to mention files..."
              disabled={props.disabled}
              value={inputValue()}
              onInput={handleInput}
              onKeyDown={handleKeyDown}
              onPaste={handlePaste}
            />

            <CommandPalette
              commands={matchedCommands()}
              query={commandQuery()}
              visible={showPalette()}
              position={palettePosition()}
              onSelect={handleSelectCommand}
              onClose={handleClose}
            />

            {/* @mention autocomplete dropdown */}
            <Show when={autocomplete().visible && (autocomplete().results.length > 0 || autocomplete().loading)}>
              <MentionAutocomplete
                results={autocomplete().results}
                selectedIndex={autocomplete().selectedIndex}
                loading={autocomplete().loading}
                query={autocomplete().query}
                onSelect={insertMention}
              />
            </Show>

            {/* Phase 2: Pending attachments chips */}
            <Show when={pendingAttachments().length > 0}>
              <div class="flex flex-wrap gap-2 mt-2">
                <For each={pendingAttachments()}>
                  {(attachment) => (
                    <div class="flex items-center gap-1.5 px-2 py-1 rounded-lg bg-surface-container-high text-xs">
                      <Show when={attachment.preview_url} fallback={
                        <span class="material-symbols-outlined text-[14px] text-outline">attach_file</span>
                      }>
                        <img
                          src={attachment.preview_url}
                          alt={attachment.name}
                          class="w-6 h-6 object-cover rounded"
                        />
                      </Show>
                      <span class="truncate max-w-[100px]" title={attachment.name}>
                        {attachment.name}
                      </span>
                      <span class="text-outline shrink-0">
                        {formatFileSize(attachment.size)}
                      </span>
                      <button
                        onClick={() => removeAttachment(attachment.id)}
                        class="shrink-0 hover:text-error transition-colors"
                        title="Remove attachment"
                      >
                        <span class="material-symbols-outlined text-[12px]">close</span>
                      </button>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>

          <div class="flex items-center justify-between mt-3 pt-3 border-t border-outline-variant/10">
            <div class="flex items-center gap-1">
              <button aria-label="Attach file" class="p-1.5 hover:bg-surface-variant rounded-lg transition-colors duration-200 group" title="Attach file">
                <span class="material-symbols-outlined text-outline group-hover:text-primary transition-colors text-[18px]">attach_file</span>
              </button>
              <button aria-label="Open terminal" class="p-1.5 hover:bg-surface-variant rounded-lg transition-colors duration-200 group" title="Terminal">
                <span class="material-symbols-outlined text-outline group-hover:text-primary transition-colors text-[18px]">terminal</span>
              </button>
              <button aria-label="Context settings" class="p-1.5 hover:bg-surface-variant rounded-lg transition-colors duration-200 group" title="Context settings">
                <span class="material-symbols-outlined text-outline group-hover:text-primary transition-colors text-[18px]">settings_ethernet</span>
              </button>
              <span class="text-[10px] text-outline-variant/50 pl-1 hidden sm:flex items-center gap-1">
                <kbd class="px-1 py-0.5 rounded text-[9px] font-mono bg-surface-container border border-outline-variant/20">/</kbd>
                <span>commands</span>
                <span class="opacity-40 mx-1">·</span>
                <kbd class="px-1 py-0.5 rounded text-[9px] font-mono bg-surface-container border border-outline-variant/20">@</kbd>
                <span>files</span>
              </span>
            </div>

            <div class="flex items-center gap-3">
              <span class="text-[10px] text-outline-variant/60">
                <kbd class="px-1 py-0.5 rounded text-[9px] font-mono bg-surface-container-high border border-outline-variant/20">Shift+Enter</kbd> new line
              </span>
            <button
              aria-label="Send message"
              data-component="prompt-submit"
              disabled={props.disabled || !inputValue().trim()}
              onClick={handleSubmit}
              class="bg-primary-container text-on-primary-container px-5 py-1.5 rounded-lg font-bold text-sm flex items-center gap-1.5 hover:opacity-90 active:scale-95 transition-all duration-200 disabled:opacity-40 disabled:cursor-not-allowed"
            >
                <span>Send</span>
                <span class="material-symbols-outlined text-sm">send</span>
              </button>
            </div>
          </div>

          <ChatContextFooter
            agentLabel="Sdd-Orchestrator"
            modelLabel={props.currentModel ? props.currentModel.split('/')[1] || props.currentModel : "Claude"}
            presetLabel="Predeterminado"
          />
        </div>
      </div>
    </div>
  );
}

/**
 * Mention autocomplete dropdown component
 */
interface MentionAutocompleteProps {
  results: FileResult[];
  selectedIndex: number;
  loading: boolean;
  query: string;
  onSelect: (file: FileResult) => void;
}

const MentionAutocomplete: Component<MentionAutocompleteProps> = (props) => {
  return (
    <div
      data-component="mention-autocomplete"
      class="absolute top-full left-0 right-0 mt-2 bg-surface-container border border-outline-variant/20 rounded-xl shadow-2xl max-h-[300px] overflow-hidden z-[1000]"
    >
      {/* Header */}
      <div class="flex items-center gap-2 px-3 py-2 border-b border-outline-variant/10 text-xs text-outline">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
          <path d="M1 2.5A1.5 1.5 0 012.5 1h11A1.5 1.5 0 0115 2.5v11a1.5 1.5 0 01-1.5 1.5h-11A1.5 1.5 0 011 13.5v-11zM2.5 2a.5.5 0 00-.5.5v11a.5.5 0 00.5.5h11a.5.5 0 00.5-.5v-11a.5.5 0 00-.5-.5h-11z"/>
        </svg>
        <span>Files</span>
        <Show when={props.query}>
          <span class="text-primary">@{props.query}</span>
        </Show>
      </div>

      {/* Results */}
      <div class="max-h-[240px] overflow-auto">
        <Show when={props.loading}>
          <div class="flex items-center justify-center gap-2 py-4 text-sm text-outline">
            <LoadingSpinner />
            Searching...
          </div>
        </Show>

        <Show when={!props.loading && props.results.length === 0}>
          <div class="py-4 text-center text-sm text-outline">
            No files found
          </div>
        </Show>

        <Show when={!props.loading && props.results.length > 0}>
          <For each={props.results}>
            {(file, index) => (
              <div
                data-component="mention-autocomplete-item"
                data-selected={index() === props.selectedIndex ? "true" : "false"}
                onClick={() => props.onSelect(file)}
                class={`flex items-center gap-2 px-3 py-2 cursor-pointer transition-all ${
                  index() === props.selectedIndex
                    ? "bg-surface-container-high"
                    : "hover:bg-surface-container-high/50"
                }`}
              >
                <FileIconForDropdown file={file} />
                <div class="flex-1 min-w-0">
                  <div class="text-sm text-on-surface truncate">
                    {file.name}
                  </div>
                  <div class="text-xs text-outline truncate">
                    {file.path}
                  </div>
                </div>
                <Show when={index() === props.selectedIndex}>
                  <span class="text-xs text-outline">⏎</span>
                </Show>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

/**
 * File icon for dropdown
 */
function FileIconForDropdown(props: { file: FileResult }) {
  const icon = () => getFileIcon(props.file.extension);
  const color = () => getFileIconColor(props.file.extension);

  const icons: Record<string, string> = {
    rust: `<path d="M.1 9.2c-.1-.1-.1-.2-.1-.3V4.2c0-.1.1-.2.2-.3l.2-.1h11.4c.1 0 .2.1.2.2l-.1 4.7c0 .1-.1.2-.2.3H8.9c-.1 0-.2.1-.3.2l-.1.1H.2c-.1-.1-.1 0-.1-.1z M1.4 8.5h9.4V5.7H1.4z M4.5 7.4h3.2v.8H4.5zm0 1.4h3.2v.8H4.5z" fill="currentColor"/>`,
    file: `<path d="M13 0H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V2c0-1.1-.9-2-2-2zm0 15H4V2h7v5h5v8z" fill="currentColor"/>`,
  };

  const iconPath = icons[icon()] ?? icons.file;

  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 16 16"
      class="flex-shrink-0"
      style={{ color: color() }}
      innerHTML={iconPath}
    />
  );
}

/**
 * Loading spinner component
 */
function LoadingSpinner() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      class="animate-spin"
      style={{ color: "var(--outline)" }}
    >
      <path
        fill="currentColor"
        d="M12 4V2A10 10 0 0 0 2 12h2a8 8 0 0 1 8-8z"
      />
    </svg>
  );
}
