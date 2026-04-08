import { createSignal, onMount, Show, For } from "solid-js";
import Header from "./components/Header";
import Sidebar from "./components/Sidebar";
import SessionView from "./components/SessionView";
import EmptySessionView from "./components/EmptySessionView";
import Terminal from "./components/Terminal";
import { Settings } from "./components/Settings";
import { ToastContainer, showToast } from "./components/Toast";
import { getApiBase } from "./api/config";
import type { Message, MessagePart } from "./api/types";

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

const MOCK_MODE = false; // Set to false when backend is available

// Mock data for testing without backend
const mockSessions: Session[] = [
  { id: "1", title: "Welcome Session", status: "completed", updated_at: new Date().toISOString() },
];

const mockResponses = [
  "I can help you with that! Let me analyze the code and provide suggestions.",
  "Based on my analysis, here are the key points to consider:\n\n1. The code structure looks good\n2. There might be a potential issue with error handling\n3. Consider adding unit tests",
  "Here's a code suggestion:\n```rust\nfn process_data(input: &str) -> Result<String, Error> {\n    // Implementation here\n    Ok(input.to_string())\n}\n```",
  "I've identified the issue. The problem is in the data flow. Let me explain...",
];

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

  return (
    models.find((model) => model.enabled && model.id === "anthropic/MiniMax-M2.7-highspeed")?.id
    ?? models.find((model) => model.enabled)?.id
    ?? models[0]?.id
    ?? null
  );
}

export default function App() {
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
        setMessages(structuredMessages);
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
    loadMessages(session.id);
    setSseStatus("connected");
  };

  const createSession = async () => {
    try {
      const res = await fetch(`${await getApiBase()}/session`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          project_path: ".", // Default to current directory
          agent_id: "build",
          model_id: currentModel(),
        }),
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
        content: mockResponses[Math.floor(Math.random() * mockResponses.length)],
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
        await loadMessages(session.id);
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

  onMount(() => {
    loadPreferredModel();
    loadSessions();
  });

  return (
    <div style="display: flex; flex-direction: column; height: 100vh; background: var(--bg-primary); color: var(--text-primary);">
      <Header 
        title="RCode" 
        sseStatus={sseStatus()} 
        terminalOpen={terminalOpen()}
        currentModel={currentModel()}
        onModelChange={setCurrentModel}
        activeSessionId={currentSession()?.id}
        onTerminalToggle={() => setTerminalOpen(!terminalOpen())}
        onSettingsClick={() => setShowSettings(true)}
      />
      <main style="flex: 1; display: flex; overflow: hidden;">
        <Sidebar
          sessions={sessions()}
          currentSessionId={currentSession()?.id}
          onSelect={selectSession}
          onNewSession={createSession}
        />
        <div style="flex: 1; display: flex; flex-direction: column; overflow: hidden;">
          <Show
            when={currentSession()}
            fallback={<EmptySessionView onCreateSession={createSession} />}
          >
            <SessionView
              session={currentSession()!}
              messages={messages()}
              isLoading={isLoading()}
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
            />
          </Show>
        </div>
      </main>
      <Terminal isOpen={terminalOpen()} onClose={() => setTerminalOpen(false)} />
      <Show when={showSettings()}>
        <Settings onClose={() => setShowSettings(false)} />
      </Show>
      <ToastContainer />
    </div>
  );
}
