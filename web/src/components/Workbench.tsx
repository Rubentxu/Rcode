import { createSignal, createEffect, Show } from "solid-js";
import type { Session } from "../stores";
import WorkbenchTopNav from "./WorkbenchTopNav";
import WorkbenchLeftRail from "./WorkbenchLeftRail";
import WorkbenchOutline from "./WorkbenchOutline";
import { ResizeHandle } from "./ResizeHandle";
import ProjectRail from "./ProjectRail";
import { useProjectContext } from "../context/ProjectContext";

// Width constants
const DEFAULT_WIDTH = 256;
const MIN_WIDTH = 180;
const MAX_WIDTH = 480;
const CENTER_MIN = 300;

// localStorage helpers
function loadWidths(): { left: number; right: number } {
  try {
    const saved = localStorage.getItem("rcode-workbench-widths");
    if (saved) {
      const parsed = JSON.parse(saved);
      const left = typeof parsed.left === 'number' ? Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, parsed.left)) : DEFAULT_WIDTH;
      const right = typeof parsed.right === 'number' ? Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, parsed.right)) : DEFAULT_WIDTH;
      return { left, right };
    }
  } catch {}
  return { left: DEFAULT_WIDTH, right: DEFAULT_WIDTH };
}

function persistWidths(left: number, right: number) {
  try {
    localStorage.setItem("rcode-workbench-widths", JSON.stringify({ left, right }));
  } catch {}
}

interface WorkbenchProps {
  sessions: Session[];
  currentSession: Session | null;
  currentModel: string;
  sseStatus: "connected" | "connecting" | "disconnected";
  terminalOpen: boolean;
  showSettings: boolean;
  onSelectSession: (session: Session) => void;
  onNewSession: () => void;
  onModelChange: (model: string) => void;
  onTerminalToggle: () => void;
  onSettingsClick: () => void;
  onSelectFile?: (path: string) => void;
  children: any; // SessionView or EmptySessionView
  // T4.4: Active file path for reveal/highlight in explorer
  activeFilePath?: string | null;
}

export default function Workbench(props: WorkbenchProps) {
  const projectContext = useProjectContext();
  const [outlineOpen, setOutlineOpen] = createSignal(false);

  // Use external prop if provided, otherwise use internal signal
  const [internalActiveFilePath, setInternalActiveFilePath] = createSignal<string | null>(null);

  // REQ-4.5: Sync with external prop when it changes
  const activeFilePath = () => props.activeFilePath ?? internalActiveFilePath();

  const setActiveFilePath = (path: string | null) => {
    // Only update internal if external is not provided
    if (props.activeFilePath === undefined) {
      setInternalActiveFilePath(path);
    }
  };

  // Resizable column widths
  const [leftWidth, setLeftWidth] = createSignal(loadWidths().left);
  const [rightWidth, setRightWidth] = createSignal(loadWidths().right);
  const [savedRightWidth, setSavedRightWidth] = createSignal(DEFAULT_WIDTH);
  const [isDragging, setIsDragging] = createSignal(false);
  
  // Track if outline has ever been opened to avoid calling handleRightReset on first mount
  let hasOutlineBeenOpened = false;
  
  // Track if this is the first mount of ResizeHandle to skip reset on initial mount
  let isFirstResizeHandleMount = true;

  // Resize handlers
  function handleLeftResize(deltaX: number) {
    const container = document.querySelector('[data-component="workbench"]');
    if (!container) return;

    const containerWidth = container.clientWidth;
    const currentLeft = leftWidth();
    const currentRight = rightWidth();
    const newLeft = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, currentLeft + deltaX));
    const newCenter = containerWidth - newLeft - currentRight;

    // Ensure center doesn't go below minimum
    if (newCenter < CENTER_MIN) return;

    setLeftWidth(newLeft);
    persistWidths(newLeft, currentRight);
  }

  function handleRightResize(deltaX: number) {
    const container = document.querySelector('[data-component="workbench"]');
    if (!container) return;

    const containerWidth = container.clientWidth;
    const currentLeft = leftWidth();
    const currentRight = rightWidth();
    // Right handle moves opposite direction: dragging right shrinks right panel
    const newRight = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, currentRight - deltaX));
    const newCenter = containerWidth - currentLeft - newRight;

    // Ensure center doesn't go below minimum
    if (newCenter < CENTER_MIN) return;

    setRightWidth(newRight);
    persistWidths(currentLeft, newRight);
  }

  function handleLeftReset() {
    setLeftWidth(DEFAULT_WIDTH);
    persistWidths(DEFAULT_WIDTH, rightWidth());
  }

  function handleRightReset() {
    // Skip reset on first mount - user hasn't had a chance to interact yet
    if (isFirstResizeHandleMount) {
      isFirstResizeHandleMount = false;
      return;
    }
    // Also skip if outline is closed (rightWidth = 0) - this is a reopen, not a user reset
    if (rightWidth() === 0) return;
    setRightWidth(DEFAULT_WIDTH);
    persistWidths(leftWidth(), DEFAULT_WIDTH);
  }

  const toggleOutline = () => {
    if (outlineOpen()) {
      setSavedRightWidth(rightWidth());
      setRightWidth(0);
      setOutlineOpen(false);
    } else {
      const isFirstOpen = !hasOutlineBeenOpened;
      setOutlineOpen(true);
      hasOutlineBeenOpened = true;
      if (!isFirstOpen) {
        // On subsequent opens (after first close), restore saved width
        setRightWidth(savedRightWidth() || DEFAULT_WIDTH);
      }
      // On first open, rightWidth is already set from localStorage via loadWidths()
    }
  };

  const handleSelectFile = (path: string) => {
    setActiveFilePath(path);
    // If outline is closed, open it when a file is selected
    if (!outlineOpen()) {
      hasOutlineBeenOpened = true;
      setRightWidth(savedRightWidth() || DEFAULT_WIDTH);
      setOutlineOpen(true);
    }
    // Call the parent's onSelectFile if provided
    if (props.onSelectFile) {
      props.onSelectFile(path);
    }
  };

  return (
    <div 
      data-component="workbench"
      class="flex flex-col h-screen bg-background text-on-surface overflow-hidden"
    >
      {/* Top navigation bar */}
      <WorkbenchTopNav
        title={props.currentSession?.title || "RCode"}
        sseStatus={props.sseStatus}
        currentModel={props.currentModel}
        onModelChange={props.onModelChange}
        activeSessionId={props.currentSession?.id}
        onTerminalToggle={props.onTerminalToggle}
        terminalOpen={props.terminalOpen}
        onSettingsClick={props.onSettingsClick}
        onOutlineToggle={toggleOutline}
        outlineOpen={outlineOpen()}
        activeProjectName={projectContext.activeProject()?.name}
      />

      {/* Main 3-column content area */}
      <div class="flex-1 flex overflow-hidden">
        {/* Far left project rail */}
        <ProjectRail />

        {/* Left rail - Sessions/Explorer */}
        <WorkbenchLeftRail
          sessions={props.sessions}
          currentSessionId={props.currentSession?.id}
          onSelect={props.onSelectSession}
          onNewSession={props.onNewSession}
          onSelectFile={handleSelectFile}
          activeFilePath={activeFilePath()}
          width={leftWidth()}
        />

        {/* Left resize handle */}
        <ResizeHandle
          side="left"
          currentWidth={leftWidth()}
          onResize={handleLeftResize}
          onReset={handleLeftReset}
          onDragStart={() => setIsDragging(true)}
          onDragEnd={() => setIsDragging(false)}
        />

        {/* Center - Transcript/Composer workspace */}
        <main
          data-component="workbench-center"
          class="flex-1 flex flex-col overflow-hidden bg-background"
          style={{ "min-width": `${CENTER_MIN}px` }}
        >
          {props.children}
        </main>

        {/* Right resize handle */}
        <Show when={outlineOpen()}>
          <ResizeHandle
            side="right"
            currentWidth={rightWidth()}
            onResize={handleRightResize}
            onReset={handleRightReset}
            onDragStart={() => setIsDragging(true)}
            onDragEnd={() => setIsDragging(false)}
          />
        </Show>

        {/* Right outline panel */}
        <WorkbenchOutline
          isOpen={outlineOpen()}
          session={props.currentSession}
          activeFilePath={activeFilePath()}
          width={rightWidth()}
          isDragging={isDragging()}
        />
      </div>
    </div>
  );
}
