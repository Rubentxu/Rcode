import { createEffect, createSignal, on, onMount, Show } from "solid-js";
import Workbench from "./components/Workbench";
import SessionView from "./components/SessionView";
import WelcomeScreen from "./components/WelcomeScreen";
import RecentProjectsView from "./components/RecentProjectsView";
import Terminal from "./components/Terminal";
import { Settings } from "./components/Settings";
import { ToastContainer, showToast } from "./components/Toast";
import { getApiBase } from "./api/config";
import type { Message, MessagePart } from "./api/types";
import { useProjectContext } from "./context/ProjectContext";
import { useWorkspace } from "./context/WorkspaceContext";
import type { Session, SSEStatus } from "./stores";

interface PromptResponse {
  message_id: string;
  request_id: string;
  status: string;
}

interface ModelCatalogEntry {
  id: string;
  enabled: boolean;
}

const MOCK_MODE = import.meta.env.VITE_MOCK_MODE === "true";

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

function pickPreferredModel(models: ModelCatalogEntry[]): string | null {
  if (models.length === 0) {
    return null;
  }
  return (
    models.find((model) => model.enabled)?.id
    ?? models[0]?.id
    ?? null
  );
}

export default function App() {
  const projectContext = useProjectContext();
  const workspace = useWorkspace();
  const { globalStore } = workspace;

  // Track whether we're doing an initial session load (full replace) vs an incremental reload
  let isSessionLoad = false;

  const handleSelectProject = (projectId: string) => {
    projectContext.setActiveProject(projectId);
  };

  const handleAddProject = () => {
    // ProjectRail handles the actual add-project flow via its own dialog
    // This is called from WelcomeScreen/RecentProjectsView CTAs which don't have direct access
    // to the ProjectRail's dialog state. The actual trigger happens through the ProjectRail's
    // handleAddProject. For now, we dispatch a custom event that ProjectRail listens to.
    const event = new CustomEvent("rcode:open-add-project");
    window.dispatchEvent(event);
  };

  // Local SSE status (per App-level SSE connection if needed)
  const [sseStatus, setSseStatus] = createSignal<"connected" | "connecting" | "disconnected">("disconnected");

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
        globalStore.setModel(preferredModel);
      }
    } catch (error) {
      console.warn("Failed to resolve preferred model", error);
    }
  };

  // Load messages for a session with merge logic
  const loadMessages = async (sessionId: string) => {
    if (MOCK_MODE) {
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
          isSessionLoad = false;
          workspace.setMessages(sessionId, structuredMessages);
        } else {
          workspace.setMessages(sessionId, structuredMessages);
        }
      } else {
        console.error("Failed to load messages", { sessionId, status: response.status, statusText: response.statusText });
      }
    } catch (e) {
      console.error("Failed to load messages:", e);
    }
  };

  const selectSession = (session: Session) => {
    workspace.switchSession(session.id);
    if (session.model_id) {
      globalStore.setModel(session.model_id);
    }
    
    // Cache-first: only fetch if messages not already cached
    const cachedMessages = workspace.workspace.getMessages(session.id);
    if (cachedMessages.length === 0) {
      isSessionLoad = true;
      void loadMessages(session.id);
    } else {
      isSessionLoad = false;
    }
    setSseStatus("connected");
  };

  const createSession = async () => {
    const activeProject = projectContext.activeProject();
    if (!activeProject) {
      showToast({
        type: "warning",
        message: `No project selected. Please open a project folder first.`,
        duration: 5000,
      });
      return null;
    }

    try {
      const model = globalStore.currentModel();
      const payload = {
        project_id: activeProject.id,
        agent_id: "build",
        model_id: model,
      };

      const res = await fetch(`${await getApiBase()}/session`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      if (!res.ok) throw new Error("Failed to create session");
      const session: Session = await res.json();
      workspace.addSession(session);
      workspace.switchSession(session.id);
      workspace.setMessages(session.id, []);
      setSseStatus("connected");
      return session;
    } catch (err) {
      console.error("Failed to create session:", err);
      // Fallback: create client-side only
      const newSession: Session = {
        id: crypto.randomUUID(),
        title: `Session ${workspace.sessions().length + 1}`,
        status: "idle",
        updated_at: new Date().toISOString(),
        model_id: globalStore.currentModel(),
      };
      workspace.addSession(newSession);
      workspace.switchSession(newSession.id);
      workspace.setMessages(newSession.id, []);
      setSseStatus("connected");
      return newSession;
    }
  };

  const submitPrompt = async (prompt: string) => {
    const sessionId = workspace.activeSessionId();
    if (!sessionId || !prompt.trim()) return;

    const session = workspace.sessions().find(s => s.id === sessionId);
    if (!session) return;

    workspace.setLoading(sessionId, true);
    
    // Add user message immediately
    const userMsg: Message = {
      id: crypto.randomUUID(),
      role: "user",
      content: prompt,
      created_at: new Date().toISOString(),
    };
    
    const currentMessages = workspace.workspace.getMessages(sessionId);
    workspace.setMessages(sessionId, [...currentMessages, userMsg]);

    if (MOCK_MODE) {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      
      const assistantMsg: Message = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: getMockResponse(prompt),
        created_at: new Date().toISOString(),
      };
      const msgs = workspace.workspace.getMessages(sessionId);
      workspace.setMessages(sessionId, [...msgs, assistantMsg]);
      workspace.setLoading(sessionId, false);
      return;
    }

    try {
      const response = await fetch(`${await getApiBase()}/session/${sessionId}/prompt`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt }),
      });
      
      if (response.ok) {
        const promptResponse = (await response.json()) as PromptResponse;
        console.info("Prompt accepted", {
          sessionId,
          requestId: promptResponse.request_id,
          status: promptResponse.status,
        });
      } else {
        const msgs = workspace.workspace.getMessages(sessionId);
        workspace.setMessages(sessionId, msgs.slice(0, -1));
        
        let errorMsg = `Request failed: ${response.status}`;
        let errorCode = "";
        try {
          const errorData = await response.json();
          errorMsg = errorData.message || errorMsg;
          errorCode = errorData.code || "";
        } catch {
          errorMsg = `${response.status} ${response.statusText}`;
        }
        
        console.error(`Prompt failed: ${errorMsg}`);
        
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
        
        await workspace.loadSessions();
        await loadMessages(sessionId);
        workspace.setLoading(sessionId, false);
      }
    } catch (e) {
      console.error("Failed to submit prompt:", e);
      const msgs = workspace.workspace.getMessages(sessionId);
      workspace.setMessages(sessionId, msgs.slice(0, -1));
      showToast({
        type: "error",
        message: `Network error: Could not connect to server. Make sure the backend is running.`,
        duration: 6000,
      });
      workspace.setLoading(sessionId, false);
    }
  };

  const abortSession = async () => {
    const sessionId = workspace.activeSessionId();
    if (!sessionId) return;

    try {
      await fetch(`${await getApiBase()}/session/${sessionId}/abort`, { method: "POST" });
      await workspace.loadSessions();
    } catch (e) {
      console.error("Failed to abort session:", e);
    }
  };

  const handleCommandResult = (result: { success: boolean; message: string; data?: unknown }) => {
    if (!result.success) {
      console.error("Command failed:", result.message);
    } else {
      console.log("Command succeeded:", result.message);
      if (result.data) {
        const action = (result.data as { action?: string }).action;
        switch (action) {
          case "new_session":
            void createSession();
            break;
          case "clear":
            {
              const sessionId = workspace.activeSessionId();
              if (sessionId) {
                workspace.setMessages(sessionId, []);
              }
            }
            break;
          case "set_model":
            {
              const modelData = (result.data as { model?: string });
              if (modelData.model) {
                globalStore.setModel(modelData.model);
              }
            }
            break;
          case "list_sessions":
            break;
          case "open_settings":
            globalStore.toggleSettings(true);
            break;
        }
      }
    }
  };

  const handleRetry = async (assistantMessageId: string, userPrompt: string) => {
    const sessionId = workspace.activeSessionId();
    if (!sessionId) return;

    const msgs = workspace.workspace.getMessages(sessionId);
    const assistantIndex = msgs.findIndex((m) => m.id === assistantMessageId);
    if (assistantIndex >= 0) {
      workspace.setMessages(sessionId, msgs.slice(0, assistantIndex));
    }

    workspace.setLoading(sessionId, true);

    if (MOCK_MODE) {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      const assistantMsg: Message = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: getMockResponse(userPrompt),
        created_at: new Date().toISOString(),
      };
      const currentMsgs = workspace.workspace.getMessages(sessionId);
      workspace.setMessages(sessionId, [...currentMsgs, assistantMsg]);
      workspace.setLoading(sessionId, false);
      return;
    }

    try {
      const response = await fetch(`${await getApiBase()}/session/${sessionId}/prompt`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt: userPrompt }),
      });
      if (response.ok) {
        const promptResponse = (await response.json()) as PromptResponse;
        console.info("Retry prompt accepted", {
          sessionId,
          requestId: promptResponse.request_id,
          status: promptResponse.status,
        });
        await loadMessages(sessionId);
      } else {
        workspace.setLoading(sessionId, false);
        let errorMsg = `Request failed: ${response.status}`;
        try {
          const errorData = await response.json();
          errorMsg = errorData.message || errorMsg;
        } catch { /* use status text */ }
        console.error(`Retry prompt failed: ${errorMsg}`);
        showToast({ type: "error", message: errorMsg, duration: 5000 });
        await loadMessages(sessionId);
      }
    } catch (e) {
      console.error("Retry failed:", e);
      workspace.setLoading(sessionId, false);
      showToast({
        type: "error",
        message: `Network error: Could not connect to server.`,
        duration: 6000,
      });
    }
  };

  onMount(() => {
    void loadPreferredModel();
    // Load sessions for the current project on mount
    void workspace.loadSessions();
  });

  // Only reset session when the active project changes
  createEffect(() => {
    const projectId = globalStore.activeProjectId();
    if (projectId) {
      // workspaceStore.switchProject preserves old workspace in LRU cache
      // and creates/restores the new one
      workspace.switchProject(projectId);
      // Load sessions for the new project
      void workspace.loadSessions();
    }
  });

  // Get current session from workspace
  const currentSession = () => {
    const sessionId = workspace.activeSessionId();
    if (!sessionId) return null;
    return workspace.sessions().find(s => s.id === sessionId) || null;
  };

  // 3-way routing: onboarding → project list → session
  const renderMainContent = () => {
    if (projectContext.projects().length === 0) {
      return <WelcomeScreen onAddProject={handleAddProject} />;
    }
    if (!currentSession()) {
      return (
        <RecentProjectsView
          projects={projectContext.projects()}
          activeProject={projectContext.activeProject()}
          onSelectProject={handleSelectProject}
          onAddProject={handleAddProject}
        />
      );
    }
    return (
      <SessionView
        session={currentSession()!}
        onSubmit={submitPrompt}
        onAbort={abortSession}
        onCommandResult={handleCommandResult}
        onComplete={() => {
          const sessionId = workspace.activeSessionId();
          if (sessionId) {
            workspace.setLoading(sessionId, false);
          }
        }}
        onReloadMessages={async () => {
          const sessionId = workspace.activeSessionId();
          if (sessionId) {
            await loadMessages(sessionId);
          }
        }}
        onError={(errorMsg) => {
          showToast({
            type: "error",
            message: `Agent error: ${errorMsg}`,
            duration: 6000,
          });
        }}
        onRetry={handleRetry}
        currentModel={globalStore.currentModel()}
      />
    );
  };

  return (
    <>
      <Workbench
        sessions={workspace.sessions()}
        currentSession={currentSession()}
        currentModel={globalStore.currentModel()}
        sseStatus={sseStatus()}
        terminalOpen={globalStore.terminalOpen()}
        showSettings={globalStore.showSettings()}
        onSelectSession={selectSession}
        onNewSession={createSession}
        onModelChange={globalStore.setModel}
        onTerminalToggle={() => globalStore.toggleTerminal()}
        onSettingsClick={() => globalStore.toggleSettings(true)}
      >
        {renderMainContent()}
      </Workbench>
      <Terminal isOpen={globalStore.terminalOpen()} onClose={() => globalStore.toggleTerminal(false)} />
      <Show when={globalStore.showSettings()}>
        <Settings onClose={() => globalStore.toggleSettings(false)} />
      </Show>
      <ToastContainer />
    </>
  );
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
