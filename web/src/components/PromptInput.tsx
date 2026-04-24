/**
 * PromptInput - Compact single-row input with model selector, reasoning effort,
 * slash command / @mention support, and file attachments.
 */

import {
  createSignal,
  createEffect,
  createMemo,
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
import {
  shouldShowAutocomplete,
  getMentionQuery,
  getPartialMentionAtCursor,
} from "../mentions/index";
import CommandPalette from "./CommandPalette";
import ChatContextFooter from "./chat-workspace/ChatContextFooter";
import { type PendingAttachment } from "../api/types";
import { showToast } from "./Toast";
import { useProviderModels } from "../hooks/useProviderModels";

// Models that support reasoning / extended thinking
const REASONING_MODEL_PATTERNS = [
  /^anthropic\/claude-opus/i,
  /^anthropic\/claude-sonnet/i,
  /^openai\/o1/i,
  /^openai\/o3/i,
  /^openai\/o4/i,
];

function modelSupportsReasoning(modelId: string): boolean {
  return REASONING_MODEL_PATTERNS.some((re) => re.test(modelId));
}

interface PromptInputProps {
  onSubmit: (prompt: string, attachments?: PendingAttachment[]) => void;
  onCommand: (result: { success: boolean; message: string; data?: unknown }) => void;
  disabled?: boolean;
  context: CommandContext;
  currentModel?: string;
  onModelChange?: (modelId: string) => void;
  currentAgent?: string | null;
  onAgentChange?: (agentId: string | null) => void;
  onTerminalToggle?: () => void;
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
  let fileInputRef: HTMLInputElement | undefined;

  const [inputValue, setInputValue] = createSignal("");
  const [showPalette, setShowPalette] = createSignal(false);
  const [palettePosition, setPalettePosition] = createSignal({ top: 0, left: 0 });
  const [commandQuery, setCommandQuery] = createSignal("");
  const [matchedCommands, setMatchedCommands] = createSignal<Command[]>([]);
  const [showModelDropdown, setShowModelDropdown] = createSignal(false);
  const [reasoningEffort, setReasoningEffort] = createSignal<"low" | "medium" | "high">("medium");

  // @mention autocomplete state
  const [autocomplete, setAutocomplete] = createSignal<AutocompleteState>({
    visible: false,
    query: "",
    results: [],
    selectedIndex: 0,
    loading: false,
  });

  // Pending attachments for drag-drop/paste/file picker
  const [pendingAttachments, setPendingAttachments] = createSignal<PendingAttachment[]>([]);
  const [isDragging, setIsDragging] = createSignal(false);

  const MAX_FILE_SIZE = 10 * 1024 * 1024; // 10MB

  let searchTimeout: number | undefined;

  // Load available models
  const { allModels, loading: modelsLoading } = useProviderModels('enabled-only');

  const currentModelId = () => props.currentModel ?? "";

  const currentModelLabel = createMemo(() => {
    const id = currentModelId();
    if (!id) return "Select model";
    const found = allModels.find((m) => m.id === id);
    if (found?.display_name) return found.display_name;
    // fallback: strip provider prefix
    return id.split("/")[1] ?? id;
  });

  const showsReasoning = createMemo(() => modelSupportsReasoning(currentModelId()));

  // Cleanup
  onCleanup(() => {
    if (searchTimeout) clearTimeout(searchTimeout);
    for (const att of pendingAttachments()) {
      if (att.preview_url) URL.revokeObjectURL(att.preview_url);
    }
  });

  // Auto-resize textarea
  const resizeTextarea = () => {
    if (!textareaRef) return;
    textareaRef.style.height = "auto";
    const newHeight = Math.min(textareaRef.scrollHeight, 240);
    textareaRef.style.height = `${newHeight}px`;
  };

  createEffect(() => {
    inputValue();
    resizeTextarea();
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
    setPalettePosition({ top: rect.bottom + 4, left: rect.left });
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
      setAutocomplete(prev => ({ ...prev, visible: true, query, selectedIndex: 0, loading: true }));
      setShowPalette(false);
      setCommandQuery("");

      if (searchTimeout) clearTimeout(searchTimeout);
      searchTimeout = window.setTimeout(async () => {
        const results = await searchFiles(query, 8);
        setAutocomplete(prev => ({ ...prev, results, loading: false }));
      }, 150);
      return;
    } else {
      setAutocomplete(prev => ({ ...prev, visible: false }));
    }

    // Detect slash command trigger
    const slashMatch = textBeforeCursor.match(/(?:^|\s)\/([^\s]*)$/);
    if (slashMatch) {
      const query = slashMatch[0].startsWith(" ") ? `/${slashMatch[1]}` : `/${slashMatch[1]}`;
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
    const value = inputValue();
    const cursorPos = textareaRef.selectionStart;
    const textBeforeCursor = value.slice(0, cursorPos);
    const textAfterCursor = value.slice(cursorPos);
    const slashMatch = textBeforeCursor.match(/(?:^|\s)\/[^\s]*$/);
    if (slashMatch) {
      const beforeCommand = textBeforeCursor.slice(0, slashMatch.index !== undefined ? slashMatch.index : 0);
      const newValue = beforeCommand + `/${command.name}` + textAfterCursor;
      setInputValue(newValue);
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

  const insertMention = (file: FileResult) => {
    const text = inputValue();
    const cursorPosition = textareaRef?.selectionStart ?? 0;
    const mentionInfo = getPartialMentionAtCursor(text, cursorPosition);
    if (!mentionInfo) return;
    const before = text.slice(0, mentionInfo.startIndex);
    const after = text.slice(cursorPosition);
    const newText = `${before}@${file.path}${after}`;
    setInputValue(newText);
    setAutocomplete(prev => ({ ...prev, visible: false }));
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

    if (value.startsWith("/")) {
      const parsed = parseCommand(value);
      if (parsed) {
        const result = await executeCommand(parsed.name, { name: parsed.rawArgs }, props.context);
        props.onCommand(result);
        if (result.success) {
          setInputValue("");
          clearPendingAttachments();
        }
        return;
      }
    }

    const attachments = pendingAttachments();
    props.onSubmit(value, attachments);
    setInputValue("");
    clearPendingAttachments();
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    const ac = autocomplete();

    if (ac.visible) {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault(); e.stopPropagation();
          setAutocomplete(prev => ({ ...prev, selectedIndex: Math.min(prev.selectedIndex + 1, prev.results.length - 1) }));
          return;
        case "ArrowUp":
          e.preventDefault(); e.stopPropagation();
          setAutocomplete(prev => ({ ...prev, selectedIndex: Math.max(prev.selectedIndex - 1, 0) }));
          return;
        case "Enter":
          e.preventDefault(); e.stopPropagation();
          if (!ac.loading && ac.results.length > 0) { insertMention(ac.results[ac.selectedIndex]); return; }
          break;
        case "Escape":
          e.preventDefault(); e.stopPropagation();
          setAutocomplete(prev => ({ ...prev, visible: false }));
          return;
        case "Tab":
          if (!ac.loading && ac.results.length > 0) {
            e.preventDefault(); e.stopPropagation();
            insertMention(ac.results[ac.selectedIndex]);
            return;
          }
          break;
      }
    }

    if (showPalette() && matchedCommands().length > 0) {
      if (["ArrowDown", "ArrowUp", "Enter", "Escape"].includes(e.key)) return;
    }

    if (e.key === "Escape" && showModelDropdown()) {
      setShowModelDropdown(false);
      return;
    }

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

  // Drag-drop handlers
  const handleDragOver = (e: DragEvent) => {
    e.preventDefault(); e.stopPropagation();
    if (e.dataTransfer?.types.includes("Files")) setIsDragging(true);
  };

  const handleDragLeave = (e: DragEvent) => {
    e.preventDefault(); e.stopPropagation();
    const rect = wrapperRef?.getBoundingClientRect();
    if (rect) {
      const { clientX, clientY } = e;
      if (clientX < rect.left || clientX > rect.right || clientY < rect.top || clientY > rect.bottom) {
        setIsDragging(false);
      }
    }
  };

  const handleDrop = (e: DragEvent) => {
    e.preventDefault(); e.stopPropagation();
    setIsDragging(false);
    const files = e.dataTransfer?.files;
    if (files) addFiles(Array.from(files));
  };

  const handlePaste = (e: ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of items) {
      if (item.kind === "file") {
        const file = item.getAsFile();
        if (file) addFiles([file]);
      }
    }
  };

  const addFiles = (files: File[]) => {
    for (const file of files) {
      if (file.size > MAX_FILE_SIZE) {
        showToast({ type: "error", message: `File too large: ${file.name} (max 10MB)`, duration: 4000 });
        continue;
      }
      const id = crypto.randomUUID();
      const preview_url = file.type.startsWith("image/") ? URL.createObjectURL(file) : undefined;
      setPendingAttachments((prev) => [...prev, { id, file, name: file.name, size: file.size, mime_type: file.type, preview_url }]);
    }
  };

  const removeAttachment = (id: string) => {
    setPendingAttachments((prev) => {
      const att = prev.find((a) => a.id === id);
      if (att?.preview_url) URL.revokeObjectURL(att.preview_url);
      return prev.filter((a) => a.id !== id);
    });
  };

  const clearPendingAttachments = () => {
    for (const att of pendingAttachments()) {
      if (att.preview_url) URL.revokeObjectURL(att.preview_url);
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

  const handleFileInputChange = (e: Event) => {
    const input = e.target as HTMLInputElement;
    if (input.files) addFiles(Array.from(input.files));
    input.value = "";
  };

  return (
    <div
      ref={wrapperRef}
      class="px-4 md:px-8 py-3 bg-surface-container-lowest"
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {/* Hidden file input */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        class="hidden"
        onChange={handleFileInputChange}
        aria-hidden="true"
      />

      {/* Drop zone overlay */}
      <Show when={isDragging()}>
        <div class="absolute inset-0 z-10 flex items-center justify-center rounded-xl bg-primary/10 border-2 border-dashed border-primary pointer-events-none">
          <div class="flex flex-col items-center gap-2 text-primary">
            <span class="material-symbols-outlined text-4xl">file_download</span>
            <span class="text-sm font-semibold">Drop to attach</span>
          </div>
        </div>
      </Show>

      <div class="w-full max-w-[var(--transcript-max-width)] mx-auto relative">
        <div
          class="bg-surface-container rounded-xl px-4 pt-3 pb-2 focus-within:ring-2 focus-within:ring-primary/60 focus-within:shadow-[0_0_0_2px_rgba(46,91,255,0.15)] transition-all duration-200"
          style="border: 1px solid var(--outline-variant);"
          onFocusIn={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "var(--primary)"; }}
          onFocusOut={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "var(--outline-variant)"; }}
        >
          {/* Textarea — auto-resize, starts at 1 row */}
          <div class="relative">
            <textarea
              ref={textareaRef}
              data-component="textarea"
              aria-label="Message input"
              aria-multiline="true"
              class="w-full bg-transparent border-none text-on-surface placeholder:text-on-surface-variant/40 resize-none focus:ring-0 text-sm leading-relaxed overflow-hidden"
              style="min-height: 36px; max-height: 240px;"
              rows={1}
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

            {/* @mention autocomplete */}
            <Show when={autocomplete().visible && (autocomplete().results.length > 0 || autocomplete().loading)}>
              <MentionAutocomplete
                results={autocomplete().results}
                selectedIndex={autocomplete().selectedIndex}
                loading={autocomplete().loading}
                query={autocomplete().query}
                onSelect={insertMention}
              />
            </Show>

            {/* Pending attachment chips */}
            <Show when={pendingAttachments().length > 0}>
              <div class="flex flex-wrap gap-2 mt-2">
                <For each={pendingAttachments()}>
                  {(attachment) => (
                    <div class="flex items-center gap-1.5 px-2 py-1 rounded-lg bg-surface-container-high text-xs">
                      <Show when={attachment.preview_url} fallback={
                        <span class="material-symbols-outlined text-[14px] text-outline">attach_file</span>
                      }>
                        <img src={attachment.preview_url} alt={attachment.name} class="w-6 h-6 object-cover rounded" />
                      </Show>
                      <span class="truncate max-w-[100px]" title={attachment.name}>{attachment.name}</span>
                      <span class="text-outline shrink-0">{formatFileSize(attachment.size)}</span>
                      <button onClick={() => removeAttachment(attachment.id)} class="shrink-0 hover:text-error transition-colors" title="Remove attachment">
                        <span class="material-symbols-outlined text-[12px]">close</span>
                      </button>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>

          {/* Action bar — single compact row */}
          <div class="flex items-center justify-between mt-1 pt-2 border-t border-outline-variant/10">

            {/* Left: toolbar icons + model selector + reasoning */}
            <div class="flex items-center gap-0.5 flex-wrap">

              {/* Attach file */}
              <button
                aria-label="Attach file"
                class="p-1.5 hover:bg-surface-variant rounded-lg transition-colors duration-200 group"
                title="Attach file"
                onClick={() => fileInputRef?.click()}
              >
                <span class="material-symbols-outlined text-outline group-hover:text-primary transition-colors text-[18px]">attach_file</span>
              </button>

              {/* Terminal toggle */}
              <Show when={props.onTerminalToggle}>
                <button
                  aria-label="Toggle terminal"
                  class="p-1.5 hover:bg-surface-variant rounded-lg transition-colors duration-200 group"
                  title="Toggle terminal"
                  onClick={() => props.onTerminalToggle?.()}
                >
                  <span class="material-symbols-outlined text-outline group-hover:text-primary transition-colors text-[18px]">terminal</span>
                </button>
              </Show>

              {/* Separator */}
              <span class="w-px h-4 bg-outline-variant/20 mx-1 shrink-0" />

              {/* Model selector */}
              <div class="relative">
                <button
                  aria-label="Select model"
                  class="flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-surface-variant transition-colors duration-200 text-[11px] text-on-surface-variant max-w-[160px]"
                  title="Change model"
                  onClick={() => setShowModelDropdown((v) => !v)}
                >
                  <span class="material-symbols-outlined text-[13px] text-outline shrink-0">psychology</span>
                  <span class="truncate">{currentModelLabel()}</span>
                  <span class="material-symbols-outlined text-[12px] text-outline shrink-0">expand_more</span>
                </button>

                <Show when={showModelDropdown()}>
                  {/* Backdrop */}
                  <div class="fixed inset-0 z-[999]" onClick={() => setShowModelDropdown(false)} />
                  {/* Dropdown */}
                  <div class="absolute bottom-full left-0 mb-2 w-64 bg-surface-container border border-outline-variant/20 rounded-xl shadow-2xl z-[1000] max-h-[320px] overflow-y-auto">
                    <div class="px-3 py-2 border-b border-outline-variant/10 text-[10px] text-outline uppercase tracking-wider font-medium">
                      Models
                    </div>
                    <Show when={modelsLoading}>
                      <div class="flex items-center justify-center gap-2 py-4 text-sm text-outline">
                        <LoadingSpinner />
                        <span>Loading…</span>
                      </div>
                    </Show>
                    <Show when={!modelsLoading && allModels.length === 0}>
                      <div class="py-3 px-3 text-xs text-outline">No models configured. Open Settings to add a provider.</div>
                    </Show>
                    <For each={allModels}>
                      {(model) => (
                        <button
                          class={`w-full text-left flex items-center gap-2 px-3 py-2 text-xs transition-colors ${
                            model.id === currentModelId()
                              ? "bg-primary/10 text-primary"
                              : "hover:bg-surface-container-high text-on-surface"
                          }`}
                          onClick={() => {
                            props.onModelChange?.(model.id);
                            setShowModelDropdown(false);
                          }}
                        >
                          <span class="material-symbols-outlined text-[13px] text-outline shrink-0">
                            {model.id === currentModelId() ? "radio_button_checked" : "radio_button_unchecked"}
                          </span>
                          <div class="flex flex-col min-w-0">
                            <span class="truncate font-medium">{model.display_name ?? model.id.split("/")[1] ?? model.id}</span>
                            <span class="text-[10px] text-outline truncate">{model.provider}</span>
                          </div>
                          <Show when={modelSupportsReasoning(model.id)}>
                            <span class="ml-auto shrink-0 text-[9px] px-1.5 py-0.5 rounded bg-tertiary/15 text-tertiary font-medium">thinking</span>
                          </Show>
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </div>

              {/* Reasoning effort — only when current model supports it */}
              <Show when={showsReasoning()}>
                <div class="flex items-center gap-0.5 ml-1">
                  <span class="material-symbols-outlined text-[13px] text-outline" title="Reasoning effort">neurology</span>
                  <For each={(["low", "medium", "high"] as const)}>
                    {(level) => (
                      <button
                        class={`px-1.5 py-0.5 rounded text-[10px] font-medium transition-colors ${
                          reasoningEffort() === level
                            ? "bg-tertiary/20 text-tertiary"
                            : "text-outline hover:text-on-surface hover:bg-surface-variant"
                        }`}
                        onClick={() => setReasoningEffort(level)}
                        title={`Reasoning: ${level}`}
                      >
                        {level[0].toUpperCase() + level.slice(1)}
                      </button>
                    )}
                  </For>
                </div>
              </Show>

            </div>

            {/* Right: hint + send */}
            <div class="flex items-center gap-2 shrink-0">
              <span class="text-[10px] text-outline-variant/50 hidden sm:block">
                <kbd class="px-1 py-0.5 rounded text-[9px] font-mono bg-surface-container-high border border-outline-variant/20">⇧↵</kbd>
                {" "}new line
              </span>
              <button
                aria-label="Send message"
                data-component="prompt-submit"
                disabled={props.disabled || !inputValue().trim()}
                 onClick={handleSubmit}
                 class="bg-primary-container text-on-primary-container px-4 py-1.5 rounded-lg font-bold text-sm flex items-center gap-1 hover:opacity-90 active:scale-95 transition-all duration-200 disabled:opacity-40 disabled:cursor-not-allowed"
               >
                 <span>Send</span>
                 <span class="material-symbols-outlined text-sm">send</span>
               </button>
             </div>
           </div>
           <ChatContextFooter
             currentAgent={props.currentAgent}
             onAgentChange={props.onAgentChange}
             modelLabel={currentModelLabel()}
           />
         </div>
       </div>
     </div>
   );
}

// ─── Mention autocomplete dropdown ─────────────────────────────────────────

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
      <div class="flex items-center gap-2 px-3 py-2 border-b border-outline-variant/10 text-xs text-outline">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
          <path d="M1 2.5A1.5 1.5 0 012.5 1h11A1.5 1.5 0 0115 2.5v11a1.5 1.5 0 01-1.5 1.5h-11A1.5 1.5 0 011 13.5v-11zM2.5 2a.5.5 0 00-.5.5v11a.5.5 0 00.5.5h11a.5.5 0 00.5-.5v-11a.5.5 0 00-.5-.5h-11z"/>
        </svg>
        <span>Files</span>
        <Show when={props.query}><span class="text-primary">@{props.query}</span></Show>
      </div>
      <div class="max-h-[240px] overflow-auto">
        <Show when={props.loading}>
          <div class="flex items-center justify-center gap-2 py-4 text-sm text-outline">
            <LoadingSpinner />Searching...
          </div>
        </Show>
        <Show when={!props.loading && props.results.length === 0}>
          <div class="py-4 text-center text-sm text-outline">No files found</div>
        </Show>
        <Show when={!props.loading && props.results.length > 0}>
          <For each={props.results}>
            {(file, index) => (
              <div
                data-component="mention-autocomplete-item"
                data-selected={index() === props.selectedIndex ? "true" : "false"}
                onClick={() => props.onSelect(file)}
                class={`flex items-center gap-2 px-3 py-2 cursor-pointer transition-all ${
                  index() === props.selectedIndex ? "bg-surface-container-high" : "hover:bg-surface-container-high/50"
                }`}
              >
                <FileIconForDropdown file={file} />
                <div class="flex-1 min-w-0">
                  <div class="text-sm text-on-surface truncate">{file.name}</div>
                  <div class="text-xs text-outline truncate">{file.path}</div>
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

// ─── File icon ──────────────────────────────────────────────────────────────

function FileIconForDropdown(props: { file: FileResult }) {
  const icon = () => getFileIcon(props.file.extension);
  const color = () => getFileIconColor(props.file.extension);

  const icons: Record<string, string> = {
    rust: `<path d="M.1 9.2c-.1-.1-.1-.2-.1-.3V4.2c0-.1.1-.2.2-.3l.2-.1h11.4c.1 0 .2.1.2.2l-.1 4.7c0 .1-.1.2-.2.3H8.9c-.1 0-.2.1-.3.2l-.1.1H.2c-.1-.1-.1 0-.1-.1z M1.4 8.5h9.4V5.7H1.4z M4.5 7.4h3.2v.8H4.5zm0 1.4h3.2v.8H4.5z" fill="currentColor"/>`,
    file: `<path d="M13 0H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V2c0-1.1-.9-2-2-2zm0 15H4V2h7v5h5v8z" fill="currentColor"/>`,
  };

  const iconPath = icons[icon()] ?? icons.file;
  return <svg width="16" height="16" viewBox="0 0 16 16" class="flex-shrink-0" style={{ color: color() }} innerHTML={iconPath} />;
}

// ─── Loading spinner ────────────────────────────────────────────────────────

function LoadingSpinner() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" class="animate-spin" style={{ color: "var(--outline)" }}>
      <path fill="currentColor" d="M12 4V2A10 10 0 0 0 2 12h2a8 8 0 0 1 8-8z" />
    </svg>
  );
}
