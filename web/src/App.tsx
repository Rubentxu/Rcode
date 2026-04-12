import { createEffect, createSignal, on, onMount, Show } from "solid-js";
import Workbench from "./components/Workbench";
import SessionView from "./components/SessionView";
import EmptySessionView from "./components/EmptySessionView";
import Terminal from "./components/Terminal";
import { Settings } from "./components/Settings";
import { ToastContainer, showToast } from "./components/Toast";
import { getApiBase } from "./api/config";
import type { Message, MessagePart } from "./api/types";
import { fetchProjectSessions } from "./api/projects";
import { useProjectContext } from "./context/ProjectContext";

export interface Session {
  id: string;
  title: string | null;
  status: "idle" | "running" | "completed";
  updated_at: string;
  model_id?: string;
}

interface PromptResponse {
  message_id: string;
  request_id: string;
  status: string;
}

interface ModelCatalogEntry {
  id: string;
  enabled: boolean;
}

const MOCK_MODE = import.meta.env.VITE_MOCK_MODE === "true"; // Set via env var for testing

// Mock data for testing without backend
const mockSessions: Session[] = [
  { id: "1", title: "Welcome Session", status: "completed", updated_at: new Date().toISOString() },
];

// Mock responses - selected deterministically based on prompt for reproducible test behavior
const mockResponses: Record<string, string> = {
  default: "I can help you with that! Let me analyze the code and provide suggestions.",
  hello: "hello hi hey",
  bash: "[Tool call: bash]\nThe current working directory is /home/rubentxu/Proyectos/rust/rust-code",
  tool: "Tool executed successfully with results.",
};

function getMockResponse(prompt: string): string {
  const lower = prompt.toLowerCase();
  if (lower.includes("hello")) return mockResponses.hello;
  if (lower.includes("bash") || lower.includes("pwd")) return mockResponses.bash;
  if (lower.includes("tool")) return mockResponses.tool;
  return mockResponses.default;
}

function normalizeMessageId(id: unknown): string {
  if (typeof id === "string") {
    return id;
  }

  if (id && typeof id === "object" && "0" in (id as Record<string, unknown>)) {
    const tupleValue = (id as Record<string, unknown>)["0"];
    if (typeof tupleValue === "string") {
      return tupleValue;
    }
  }

  return "";
}

function pickPreferredModel(models: ModelCatalogEntry[]): string | null {
  if (models.length === 0) {
    return null;
  }

  // Use the first enabled model from the catalog; fallback to first model available
  return (
    models.find((model) => model.enabled)?.id
    ?? models[0]?.id
    ?? null
  );
}

export default function App() {
  const projectContext = useProjectContext();
  const [sessions, setSessions] = createSignal<Session[]>(MOCK_MODE ? mockSessions : []);
  const [currentSession, setCurrentSession] = createSignal<Session | null>(null);
  const [currentModel, setCurrentModel] = createSignal<string>("claude-sonnet-4-5");
  const [messages, setMessages] = createSignal<Message[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);
  const [sseStatus, setSseStatus] = createSignal<"connected" | "connecting" | "disconnected">("disconnected");
  const [terminalOpen, setTerminalOpen] = createSignal(false);
  const [showSettings, setShowSettings] = createSignal(false);

  const loadSessions = async () => {
    if (MOCK_MODE) {
      setSessions(mockSessions);
      return;
    }
    try {
      const response = await fetch(`${await getApiBase()}/session`);
      if (response.ok) {
        const data = await response.json();
        setSessions(data);
      }
    } catch (e) {
      console.error("Failed to load sessions:", e);
    }
  };

  const loadProjectSessions = async (projectId: string) => {
    if (MOCK_MODE) {
      setSessions(mockSessions);
      return;
    }

    try {
      const data = await fetchProjectSessions(projectId);
      setSessions(data.map((session) => ({
        id: session.id,
        title: session.title,
        status: session.status as Session["status"],
        updated_at: session.updated_at,
        model_id: session.model_id,
      })));
    } catch (error) {
      console.error("Failed to load project sessions:", error);
    }
  };

  const loadPreferredModel = async () => {
    try {
      const response = await fetch(`${await getApiBase()}/models`);
      if (!response.ok) {
        return;
      }

      const data = await response.json();
      const preferredModel = pickPreferredModel((data.models || []) as ModelCatalogEntry[]);
      if (preferredModel) {
        console.info("Resolved preferred model", { preferredModel });
        setCurrentModel(preferredModel);
      }
    } catch (error) {
      console.warn("Failed to resolve preferred model", error);
    }
  };

  // Track whether we're doing an initial session load (full replace) vs an incremental reload
  let isSessionLoad = false;

  const loadMessages = async (sessionId: string) => {
    if (MOCK_MODE) {
      // Messages are managed locally in mock mode
      return;
    }
    try {
      const url = `${await getApiBase()}/session/${sessionId}/messages?offset=0&limit=100`;
      console.info("Loading messages", { sessionId, url });
      const response = await fetch(url);
      console.info("Load messages response", { sessionId, ok: response.ok, status: response.status });
      if (response.ok) {
        const data = await response.json();
        console.debug("Load messages payload", {
          sessionId,
          total: data.total,
          count: Array.isArray(data.messages) ? data.messages.length : 0,
        });
        // Preserve structured parts from backend; keep content for backward compat
        const structuredMessages: Message[] = (data.messages || []).map((m: any) => {
          console.debug("Loaded message", {
            sessionId,
            messageId: normalizeMessageId(m.id),
            role: m.role,
            partTypes: Array.isArray(m.parts) ? m.parts.map((p: any) => p?.type) : [],
            hasParts: Array.isArray(m.parts) && m.parts.length > 0,
          });
          
          return {
            id: normalizeMessageId(m.id),
            role: typeof m.role === 'string' ? m.role.toLowerCase() : 'user',
            content: m.content || "",
            created_at: m.created_at,
            parts: m.parts as MessagePart[] | undefined,
          };
        });

        if (isSessionLoad) {
          // Full replace only on initial session selection
          isSessionLoad = false;
          setMessages(structuredMessages);
        } else {
          // Incremental reload (from SSE onDone/onAssistantCommitted/onReloadMessages):
          // Always merge — never remove messages that exist locally.
          // The backend may not have all messages persisted yet (race condition),
          // so we use a length-based heuristic: if backend has >= local count,
          // do a full replace (backend is authoritative). Otherwise, merge only
          // messages that exist in both.
          setMessages((prev) => {
            if (structuredMessages.length >= prev.length) {
              // Backend is caught up — full replace is safe
              return structuredMessages;
            }
            // Backend is behind — merge existing messages but keep local-only ones
            const backendMap = new Map(structuredMessages.map((m) => [m.id, m]));
            return prev.map((local) => {
              const fromBackend = backendMap.get(local.id);
              return fromBackend ?? local;
            });
          });
        }
      } else {
        console.error("Failed to load messages", { sessionId, status: response.status, statusText: response.statusText });
      }
    } catch (e) {
      console.error("Failed to load messages:", e);
    }
  };

  const selectSession = (session: Session) => {
    setCurrentSession(session);
    if (session.model_id) {
      setCurrentModel(session.model_id);
    }
    isSessionLoad = true;
    loadMessages(session.id);
    setSseStatus("connected");
  };

  const createSession = async () => {
    try {
      const activeProject = projectContext.activeProject();
      const payload = activeProject
        ? {
            project_id: activeProject.id,
            agent_id: "build",
            model_id: currentModel(),
          }
        : {
            project_path: ".",
            agent_id: "build",
            model_id: currentModel(),
          };

      const res = await fetch(`${await getApiBase()}/session`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      if (!res.ok) throw new Error("Failed to create session");
      const session: Session = await res.json();
      setSessions((prev) => [session, ...prev]);
      setCurrentSession(session);
      setMessages([]);
      setSseStatus("connected");
      return session;
    } catch (err) {
      console.error("Failed to create session:", err);
      // Fallback: create client-side only
      const newSession: Session = {
        id: crypto.randomUUID(),
        title: `Session ${sessions().length + 1}`,
        status: "idle",
        updated_at: new Date().toISOString(),
        model_id: currentModel(),
      };
      setSessions((prev) => [newSession, ...prev]);
      setCurrentSession(newSession);
      setMessages([]);
      setSseStatus("connected");
      return newSession;
    }
  };

  const submitPrompt = async (prompt: string) => {
    const session = currentSession();
    if (!session || !prompt.trim()) return;

    setIsLoading(true);
    
    // Add user message immediately
    const userMsg: Message = {
      id: crypto.randomUUID(),
      role: "user",
      content: prompt,
      created_at: new Date().toISOString(),
    };
    setMessages((prev) => [...prev, userMsg]);

    if (MOCK_MODE) {
      // Simulate AI response delay
      await new Promise((resolve) => setTimeout(resolve, 1000));
      
      const assistantMsg: Message = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: getMockResponse(prompt),
        created_at: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, assistantMsg]);
      setIsLoading(false);
      return;
    }

    try {
      const response = await fetch(`${await getApiBase()}/session/${session.id}/prompt`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt }),
      });
      
      if (response.ok) {
        const promptResponse = (await response.json()) as PromptResponse;
        console.info("Prompt accepted", {
          sessionId: session.id,
          requestId: promptResponse.request_id,
          status: promptResponse.status,
        });
        // Do NOT call loadMessages here — the user message is already in local state,
        // and the backend hasn't processed the prompt yet. loadMessages would replace
        // local messages with stale backend data, causing the user message to disappear.
        // Messages will be refreshed when SSE onDone/onAssistantCommitted fires.
      } else {
        // Remove the user message we just added since the prompt failed
        setMessages((prev) => prev.slice(0, -1));
        
        // Try to parse error response from backend
        let errorMsg = `Request failed: ${response.status}`;
        let errorCode = "";
        try {
          const errorData = await response.json();
          errorMsg = errorData.message || errorMsg;
          errorCode = errorData.code || "";
        } catch {
          // Use status text if JSON parsing fails
          errorMsg = `${response.status} ${response.statusText}`;
        }
        
        console.error(`Prompt failed: ${errorMsg}`);
        
        // Show toast notification with helpful error message
        if (errorMsg.includes("API key") || errorMsg.includes("No API key")) {
          showToast({
            type: "error",
            message: `API key not configured for this model. Go to Settings to configure your API key.`,
            duration: 8000,
          });
        } else if (errorMsg.includes("Unknown provider")) {
          showToast({
            type: "error",
            message: `Unknown model provider. Check Settings to configure a valid model.`,
            duration: 6000,
          });
        } else if (errorCode === "SESSION_ALREADY_RUNNING" || response.status === 409) {
          showToast({
            type: "warning",
            message: `Session is already processing. Please wait or abort the current run.`,
            duration: 5000,
          });
        } else if (response.status === 500) {
          showToast({
            type: "error",
            message: `Server error: ${errorMsg}`,
            duration: 6000,
          });
        } else {
          showToast({
            type: "error",
            message: errorMsg,
            duration: 5000,
          });
        }
        
        // Reload sessions to get accurate status
        loadSessions();
        
        // Also reload messages in case session state changed
        loadMessages(session.id).catch(() => {});
        
        // HTTP errors - SSE won't provide completion, so reset loading state
        setIsLoading(false);
      }
    } catch (e) {
      console.error("Failed to submit prompt:", e);
      // Remove the user message since the request failed completely
      setMessages((prev) => prev.slice(0, -1));
      showToast({
        type: "error",
        message: `Network error: Could not connect to server. Make sure the backend is running.`,
        duration: 6000,
      });
      // Network errors - SSE won't provide completion, so reset loading state
      setIsLoading(false);
    }
  };

  const abortSession = async () => {
    const session = currentSession();
    if (!session) return;

    try {
      await fetch(`${await getApiBase()}/session/${session.id}/abort`, { method: "POST" });
      loadSessions();
    } catch (e) {
      console.error("Failed to abort session:", e);
    }
  };

  const handleCommandResult = (result: { success: boolean; message: string; data?: unknown }) => {
    // Handle command results - for now just log them
    // In the future, these could show toast notifications or update UI
    if (!result.success) {
      console.error("Command failed:", result.message);
    } else {
      console.log("Command succeeded:", result.message);
      // Handle specific command actions
      if (result.data) {
        const action = (result.data as { action?: string }).action;
        switch (action) {
          case "new_session":
            createSession();
            break;
          case "clear":
            setMessages([]);
            break;
          case "set_model":
            // Model change is handled via setCurrentModel in Header
            const modelData = (result.data as { model?: string });
            if (modelData.model) {
              setCurrentModel(modelData.model);
            }
            break;
          case "list_sessions":
            // Sessions are already available in sidebar
            break;
          case "open_settings":
            setShowSettings(true);
            break;
        }
      }
    }
  };

  /**
   * MQA-2: Retry handler - replaces assistant response and re-submits user prompt.
   * Truncates messages from the assistant turn onward, then calls the API directly
   * WITHOUT adding a duplicate user message to local state.
   */
  const handleRetry = async (assistantMessageId: string, userPrompt: string) => {
    const session = currentSession();
    if (!session) return;

    // Truncate messages from the assistant turn onward
    setMessages((prev) => {
      const assistantIndex = prev.findIndex((m) => m.id === assistantMessageId);
      if (assistantIndex < 0) return prev;
      return prev.slice(0, assistantIndex);
    });

    setIsLoading(true);

    if (MOCK_MODE) {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      const assistantMsg: Message = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: getMockResponse(userPrompt),
        created_at: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, assistantMsg]);
      setIsLoading(false);
      return;
    }

    try {
      const response = await fetch(`${await getApiBase()}/session/${session.id}/prompt`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt: userPrompt }),
      });
      if (response.ok) {
        const promptResponse = (await response.json()) as PromptResponse;
        console.info("Retry prompt accepted", {
          sessionId: session.id,
          requestId: promptResponse.request_id,
          status: promptResponse.status,
        });
        await loadMessages(session.id);
      } else {
        setIsLoading(false);
        let errorMsg = `Request failed: ${response.status}`;
        try {
          const errorData = await response.json();
          errorMsg = errorData.message || errorMsg;
        } catch { /* use status text */ }
        console.error(`Retry prompt failed: ${errorMsg}`);
        showToast({ type: "error", message: errorMsg, duration: 5000 });
        loadMessages(session.id).catch(() => {});
      }
    } catch (e) {
      console.error("Retry failed:", e);
      setIsLoading(false);
      showToast({
        type: "error",
        message: `Network error: Could not connect to server.`,
        duration: 6000,
      });
    }
  };

  onMount(() => {
    loadPreferredModel();
  });

  // Only reset session/messages when the active project *changes* to a different value.
  // Using on() with defer:false ensures the initial load runs immediately but we
  // compare prev vs. next to avoid clearing an already-valid session on first hydration
  // (e.g. Tauri auto-selects the first project on mount which would otherwise wipe
  // a session that was just created or loaded before the effect fires).
  createEffect(
    on(
      () => projectContext.activeProject()?.id,
      (nextId, prevId) => {
        if (nextId !== prevId) {
          setCurrentSession(null);
          setMessages([]);
        }
        const activeProject = projectContext.activeProject();
        void (activeProject ? loadProjectSessions(activeProject.id) : loadSessions());
      },
    ),
  );

  return (
    <>
      <Workbench
        sessions={sessions()}
        currentSession={currentSession()}
        currentModel={currentModel()}
        sseStatus={sseStatus()}
        terminalOpen={terminalOpen()}
        showSettings={showSettings()}
        onSelectSession={selectSession}
        onNewSession={createSession}
        onModelChange={setCurrentModel}
        onTerminalToggle={() => setTerminalOpen(!terminalOpen())}
        onSettingsClick={() => setShowSettings(true)}
      >
        <Show
          when={currentSession()}
          fallback={<EmptySessionView onCreateSession={createSession} />}
        >
          <SessionView
            session={currentSession()!}
            messages={messages()}
            isLoading={isLoading}
            sseStatus={sseStatus()}
            onSubmit={submitPrompt}
            onAbort={abortSession}
            onSSEStatusChange={setSseStatus}
            sessions={sessions()}
            onCommandResult={handleCommandResult}
            onComplete={() => setIsLoading(false)}
            onReloadMessages={async () => {
              const session = currentSession();
              if (session) {
                await loadMessages(session.id);
              }
            }}
            onError={(errorMsg) => {
              setIsLoading(false);
              showToast({
                type: "error",
                message: `Agent error: ${errorMsg}`,
                duration: 6000,
              });
            }}
            onRetry={handleRetry}
            currentModel={currentModel()}
          />
        </Show>
      </Workbench>
      <Terminal isOpen={terminalOpen()} onClose={() => setTerminalOpen(false)} />
      <Show when={showSettings()}>
        <Settings onClose={() => setShowSettings(false)} />
      </Show>
      <ToastContainer />
    </>
  );
}
