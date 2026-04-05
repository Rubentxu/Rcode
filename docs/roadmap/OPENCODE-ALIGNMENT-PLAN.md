# RCode ← OpenCode Alignment Plan

**Date:** 2026-04-05
**Reference:** [opencode-ai/opencode](https://github.com/opencode-ai/opencode) (Go rewrite, main branch)

## Executive Summary

OpenCode was rewritten from TypeScript to Go with a clean, pragmatic architecture. This document maps every architectural gap between OpenCode v2 (Go) and our RCode clone (Rust), ordered by dependency and priority. Each gap becomes an SDD change for Agent Teams implementation.

---

## Architecture Comparison Matrix

| Capability | OpenCode (Go) | RCode (Rust) | Status |
|---|---|---|---|
| Provider trait | 3 methods: `SendMessages`, `StreamResponse`, `Model` | `complete()` + `stream()` + `model_info()` + `abort()` | ✅ Parity (RCode has more) |
| Provider factory | `NewProvider(name, opts...)` with switch | `ProviderFactory::build()` with match | ✅ Parity |
| Provider count | 6 (Anthropic, OpenAI, Gemini, Bedrock, Azure, Copilot) | 7 (Anthropic, OpenAI, Google, OpenRouter, MiniMax, ZAI, Mock) | ✅ Parity |
| Config cascade | JSON/JSONC, global → project → env | JSON/JSONC, managed → global → project → env | ✅ Parity |
| Credential storage | `APIKey` in config per provider | `api_key` in config + `*_API_KEY`/`*_AUTH_TOKEN` env | ✅ Parity |
| **Agent loop** | `processGeneration()`: stream → tool calls → loop | `Executor::run()` exists but **not wired to server** | 🔴 Gap #1 |
| **Subagent delegation** | `agentTool` creates child session, runs agent, returns | `TaskTool` is a **placeholder** | 🔴 Gap #2 |
| **Session parent/child** | `CreateTaskSession`, `CreateTitleSession` with `ParentSessionID` | No parent-child in schema | 🔴 Gap #3 |
| **Title generation** | Async background, separate session, `SendMessages` | `TitleGenerator` exists but **not wired** | 🔴 Gap #4 |
| **Compaction** | Truncates at `SummaryMessageID`, `Summarize()` method | `Summarizer` trait + `CompactionService` exist but **not wired** | 🔴 Gap #5 |
| **Permission system** | Blocking `Request()` → `Grant()`/`Deny()` via pubsub | Permission rules exist, only enforced in TaskTool | 🟡 Gap #6 |
| **Event bus** | pubsub per entity (Session, AgentEvent, PermissionRequest) | `EventBus` in core + separate `event` crate | 🟡 Gap #7 |
| **MCP tools** | Dynamic MCP tool loading with session context | `McpTool` exists but session context not wired | 🟡 Gap #8 |
| **Model configuration per agent** | `Agents map[AgentName]Agent{Model, MaxTokens, ReasoningEffort}` | `AgentDefinition` has `model` field, **ignored by executor** | 🔴 Gap #9 |
| **Auto-compact** | `AutoCompact` config flag | Not implemented | 🟡 Gap #10 |
| **LSP integration** | Config-driven per-language LSP clients | `lsp` crate exists, not wired to agent tools | 🟡 Gap #11 |

---

## Dependency Graph

```
GAP #1: Wire Agent Loop ─────────────────────────────────────┐
    │                                                          │
    ├── GAP #3: Session Parent/Child                          │
    │       │                                                  │
    │       ├── GAP #2: Subagent Delegation (TaskTool)        │
    │       │                                                  │
    │       └── GAP #4: Title Generation                      │
    │                                                          │
    ├── GAP #9: Agent Model Overrides                         │
    │                                                          │
    └── GAP #5: Compaction/Summarize                          │
            │                                                  │
            └── GAP #10: Auto-compact flag                    │
                                                             │
GAP #6: Permission System ──────────────────────────────────│ (parallel)
    │                                                          │
    └── GAP #2 (also needs permissions)                      │
                                                             │
GAP #7: Event Bus Unification ──────────────────────────────│ (parallel)
    │                                                          │
    └── All SSE streaming depends on this                    │
                                                             │
GAP #8: MCP Tools with Session Context ─────────────────────│ (parallel)
                                                             │
GAP #11: LSP Integration ───────────────────────────────────│ (parallel)
                                                             │
COMMIT ─────────────────────────────────────────────────────┘
```

---

## GAP #1: Wire Agent Loop to Server (CRITICAL)

### Current State
- `Executor::run()` in `crates/agent/src/executor.rs` — fully implemented with streaming, tool loop, cancellation
- `crates/server/src/routes/mod.rs` — `/prompt` endpoint receives user input but does NOT call the executor
- The executor is only called from TUI (`crates/tui/src/views/chat.rs`)

### OpenCode Pattern (Go)
```go
func (a *agent) Run(ctx, sessionID, content, attachments) (<-chan AgentEvent, error) {
    // 1. Check session busy
    // 2. Store cancel in sync.Map
    // 3. goroutine {
    //    processGeneration(ctx, sessionID, content, attachments)
    //  }
    // 4. Return event channel
}

func (a *agent) processGeneration(ctx, sessionID, content, attachments) AgentEvent {
    // 1. Load messages from session
    // 2. If first message → async generateTitle
    // 3. If compacted → truncate at SummaryMessageID
    // 4. Create user message, append to history
    // 5. LOOP: streamAndHandleEvents → if tool_use: continue; if done: return
}
```

### Target State
- `/prompt` calls `Executor::run()` (or a new `AgentService::run()`)
- Returns SSE stream of `AgentEvent` (text chunks, tool calls, tool results, finish)
- Tracks per-session busy state (`sync.Map` or `DashMap`)
- Supports cancellation via `DELETE /prompt/:session_id`
- Loads existing messages from session storage before first prompt

### Files to Change
- `crates/server/src/routes/mod.rs` — wire `/prompt` to executor
- `crates/server/src/state.rs` — add `Executor` or `AgentService` to `AppState`
- `crates/server/src/lib.rs` — initialize executor on server start
- `crates/agent/src/executor.rs` — possibly add `AgentService` wrapper

### SDD Change Name: `wire-agent-loop`

---

## GAP #2: Subagent Delegation (TaskTool)

### Current State
- `crates/tools/src/task.rs` — `TaskTool` exists as a placeholder
- `crates/agent/src/subagent.rs` — `Subagent` struct exists but not fully wired

### OpenCode Pattern (Go)
```go
func (b *agentTool) Run(ctx, call ToolCall) (ToolResponse, error) {
    // 1. Parse prompt from params
    // 2. Get current sessionID, messageID from context
    // 3. Create new Agent(config.AgentTask, sessions, messages, TaskAgentTools)
    // 4. CreateTaskSession(toolCallID, parentSessionID, title)
    // 5. agent.Run(ctx, session.ID, prompt) → wait for done
    // 6. Return result text
}
```
- Task agents get a **restricted tool set**: GlobTool, GrepTool, LS, View (NO Bash, Edit, Write)
- Result is NOT visible to user — parent agent must summarize

### Target State
- `TaskTool` creates child session with `parent_session_id`
- Runs executor with restricted tool set (read-only tools only)
- Returns tool result to parent agent
- Child session is navigable in UI

### Files to Change
- `crates/tools/src/task.rs` — implement real delegation
- `crates/agent/src/subagent.rs` — wire to executor
- `crates/server/src/routes/mod.rs` — expose child sessions in API
- `web/src/components/` — session navigation UI

### SDD Change Name: `subagent-delegation`
### Depends On: `wire-agent-loop`, `session-parent-child`

---

## GAP #3: Session Parent/Child Hierarchy

### Current State
- `crates/storage/src/schema.rs` — `sessions` table has no `parent_session_id` column
- `crates/session/src/service.rs` — `SessionService` has `create()`, no child session support

### OpenCode Pattern (Go)
```go
type Session struct {
    ID, ParentSessionID, Title string
    MessageCount, PromptTokens, CompletionTokens int64
    SummaryMessageID string
    Cost float64
    CreatedAt, UpdatedAt int64
}

// Three creation modes:
Create(ctx, title)                              // Top-level session
CreateTaskSession(ctx, toolCallID, parentID, title)  // Subagent session
CreateTitleSession(ctx, parentID)               // Title generation session
```

### Target State
- Add `parent_session_id TEXT NULL` to sessions table
- Add `CreateTaskSession` and `CreateTitleSession` methods
- Add `ListChildren(session_id)` query
- Session list endpoint returns tree structure

### Files to Change
- `crates/storage/src/schema.rs` — add column + migration
- `crates/session/src/service.rs` — add creation methods
- `crates/core/src/session.rs` — update Session struct
- `crates/server/src/routes/mod.rs` — expose hierarchy in API

### SDD Change Name: `session-parent-child`

---

## GAP #4: Wire Title Generation

### Current State
- `crates/session/src/summarizer.rs` — has `TitleGenerator` (not wired)
- `crates/session/src/compaction_service.rs` — has `CompactionService` (not wired)

### OpenCode Pattern (Go)
```go
// In processGeneration:
if len(msgs) == 0 {
    go func() {
        titleErr := a.generateTitle(context.Background(), sessionID, content)
    }()
}

func (a *agent) generateTitle(ctx, sessionID, content) error {
    // 1. CreateTitleSession(parentSessionID)
    // 2. SendMessages (NOT stream) with prompt: "Generate a short title for: {content}"
    // 3. session.Title = response.Content
    // 4. session.Save()
}
```

### Target State
- On first message in a session, spawn async title generation
- Uses a smaller/cheaper model (configurable per agent)
- Updates session title in storage
- Publishes session update event for UI refresh

### Files to Change
- `crates/agent/src/executor.rs` — trigger title gen on first message
- `crates/session/src/summarizer.rs` — wire `TitleGenerator`
- `crates/server/src/routes/mod.rs` — publish session events via SSE

### SDD Change Name: `wire-title-generation`
### Depends On: `wire-agent-loop`, `session-parent-child`

---

## GAP #5: Wire Compaction/Summarization

### Current State
- `crates/session/src/compaction.rs` — `CompactionService` with `should_compact()` logic
- `crates/session/src/compaction_service.rs` — service wrapper
- `crates/session/src/summarizer.rs` — `Summarizer` trait
- Neither is called from the executor

### OpenCode Pattern (Go)
```go
// In processGeneration:
if session.SummaryMessageID != "" {
    // Find summary message in history
    // Truncate history to start from summary
    msgs[0].Role = message.User  // Treat summary as user context
}

// Summarize is a separate agent method:
Summarize(ctx, sessionID) error
```

### Target State
- Before sending messages to LLM, check token count
- If exceeds threshold → call `Summarizer` to create summary
- Store summary as a special message, truncate history
- Configurable via `AutoCompact` flag

### Files to Change
- `crates/agent/src/executor.rs` — check token count, trigger compaction
- `crates/session/src/compaction_service.rs` — wire to executor
- `crates/session/src/summarizer.rs` — implement real summarization (LLM call)

### SDD Change Name: `wire-compaction`
### Depends On: `wire-agent-loop`

---

## GAP #6: Permission System (Full)

### Current State
- `crates/core/src/permission.rs` — `PermissionRule` with allow/deny patterns
- `crates/agent/src/permissions.rs` — `PermissionChecker`
- Only enforced in `TaskTool`, not globally

### OpenCode Pattern (Go)
```go
type Service interface {
    GrantPersistant(permission)   // Remember for session
    Grant(permission)             // One-time allow
    Deny(permission)              // One-time deny
    Request(opts) bool            // Blocks until user responds
    AutoApproveSession(sessionID) // For non-interactive
}

// Every tool call goes through:
if !permissionService.Request(opts) {
    return // Tool call denied
}
```
- Path-based checking with directory resolution
- TUI shows permission prompt, blocks agent until response
- Web shows permission dialog via SSE event

### Target State
- Permission check before EVERY tool execution (not just TaskTool)
- Blocking pattern for TUI, event-based for web/SSE
- Per-session persistent grants
- Path/command pattern matching
- Auto-approve mode for non-interactive (server/CI)

### Files to Change
- `crates/agent/src/permissions.rs` — full permission service
- `crates/agent/src/executor.rs` — check permissions before each tool call
- `crates/server/src/routes/mod.rs` — SSE permission request events
- `web/src/components/` — permission dialog UI

### SDD Change Name: `permission-system`
### Depends On: `wire-agent-loop`

---

## GAP #7: Event Bus Unification

### Current State
- `crates/core/src/event.rs` — `EventBus` with typed events
- `crates/event/src/bus.rs` — separate event crate
- Two event systems coexist, causing confusion

### OpenCode Pattern (Go)
```go
// pubsub pattern per entity type:
type Broker[T any] struct { subscribers map[string]chan T }
type Suscriber[T any] interface {
    Subscribe(id string) <-chan T
    Unsubscribe(id string)
    Publish(eventType string, data T)
}
```
- Each entity (Session, AgentEvent, PermissionRequest) has its own typed pubsub
- Clean separation, no cross-contamination

### Target State
- Single `EventBus` in core, typed channels per entity
- Remove duplicate `event` crate or merge into core
- SSE server subscribes to relevant events and forwards to clients

### Files to Change
- `crates/core/src/event.rs` — make generic typed pubsub
- `crates/event/` — deprecate or merge
- `crates/server/src/routes/mod.rs` — subscribe to event bus for SSE

### SDD Change Name: `event-bus-unification`
### Depends On: None (foundational)

---

## GAP #8: MCP Tools with Session Context

### Current State
- `crates/tools/src/mcp_tool.rs` — `McpTool` exists
- MCP servers configured in `RcodeConfig.mcp_servers`
- Session context not passed to MCP tool calls

### OpenCode Pattern (Go)
```go
// MCP tools are dynamically loaded per server config
// Session context (sessionID, messageID) available in tool context
// Tools can read/write session-scoped data
```

### Target State
- MCP tools receive session context
- Tool results stored with message association
- MCP server lifecycle managed (start/stop per config)

### Files to Change
- `crates/tools/src/mcp_tool.rs` — pass session context
- `crates/server/src/routes/mod.rs` — MCP server management endpoints

### SDD Change Name: `mcp-session-context`
### Depends On: `wire-agent-loop`

---

## GAP #9: Agent Model Overrides

### Current State
- `crates/core/src/agent.rs` — `AgentDefinition` has `model: Option<String>`
- `crates/agent/src/executor.rs` — ignores agent model, always uses configured provider
- OpenCode's agents (coder, summarizer, task, title) each have their own model config

### OpenCode Pattern (Go)
```go
type Agent struct {
    Model           models.ModelID
    MaxTokens       int64
    ReasoningEffort string
}

type Config struct {
    Agents map[AgentName]Agent  // Per-agent model override
}

// Agent service uses agent-specific model:
func NewAgent(agentName, sessions, messages, tools) (Service, error) {
    cfg := config.Get()
    agentCfg := cfg.Agents[agentName]
    provider := provider.NewProvider(agentCfg.Model, ...)
}
```

### Target State
- Executor reads agent's model override from config
- Falls back to global configured model if agent has no override
- Per-agent `max_tokens` and `reasoning_effort` support

### Files to Change
- `crates/agent/src/executor.rs` — use agent model override
- `crates/core/src/config.rs` — add per-agent config section
- `crates/server/src/state.rs` — build provider per agent config

### SDD Change Name: `agent-model-overrides`
### Depends On: `wire-agent-loop`

---

## GAP #10: Auto-Compact Flag

### Current State
- Not implemented

### OpenCode Pattern (Go)
```go
type Config struct {
    AutoCompact bool `json:"autoCompact,omitempty"`
}
```
- When enabled, automatically summarizes when context gets too long

### Target State
- Add `auto_compact: bool` to config
- Wire to compaction check in executor

### Files to Change
- `crates/core/src/config.rs` — add field
- `crates/agent/src/executor.rs` — check flag before compaction

### SDD Change Name: `auto-compact`
### Depends On: `wire-compaction`

---

## GAP #11: LSP Integration

### Current State
- `crates/lsp/src/lib.rs` — LSP client wrapper exists
- Not wired to agent tools or config

### OpenCode Pattern (Go)
```go
type LSPConfig struct {
    Disabled bool
    Command  string
    Args     []string
    Options  any
}

type Config struct {
    LSP map[string]LSPConfig  // Per-language LSP config
}
```
- LSP provides go-to-definition, hover, diagnostics to agents

### Target State
- Config-driven LSP server startup per language
- Expose as agent tools (definition, hover, references)

### Files to Change
- `crates/lsp/` — config-driven initialization
- `crates/tools/` — LSP tools for agent use
- `crates/core/src/config.rs` — LSP config section

### SDD Change Name: `lsp-integration`
### Depends On: `wire-agent-loop`

---

## Implementation Order (Recommended)

### Wave 1 — Foundation (must be done first)
| Order | SDD Change | Gap | Rationale |
|-------|-----------|-----|-----------|
| 1 | `event-bus-unification` | #7 | Foundational — SSE depends on clean events |
| 2 | `session-parent-child` | #3 | Schema change needed before subagents |
| 3 | `wire-agent-loop` | #1 | **Critical path** — everything depends on this |

### Wave 2 — Core Features
| Order | SDD Change | Gap | Rationale |
|-------|-----------|-----|-----------|
| 4 | `agent-model-overrides` | #9 | Quick win after loop is wired |
| 5 | `wire-title-generation` | #4 | UX improvement, depends on loop + sessions |
| 6 | `permission-system` | #6 | Safety requirement, depends on loop |
| 7 | `wire-compaction` | #5 | Reliability, depends on loop |

### Wave 3 — Advanced Features
| Order | SDD Change | Gap | Rationale |
|-------|-----------|-----|-----------|
| 8 | `subagent-delegation` | #2 | Most complex, depends on everything above |
| 9 | `auto-compact` | #10 | Small, depends on compaction |
| 10 | `mcp-session-context` | #8 | Enhancement, depends on loop |
| 11 | `lsp-integration` | #11 | Nice-to-have, depends on loop |

---

## Estimated Complexity

| SDD Change | Complexity | Est. Tasks | Risk |
|---|---|---|---|
| `event-bus-unification` | Medium | 8-10 | Low — refactor, no new behavior |
| `session-parent-child` | Low | 4-6 | Low — schema + methods |
| `wire-agent-loop` | **High** | 12-15 | **High** — core behavior change |
| `agent-model-overrides` | Low | 3-5 | Low — config + executor tweak |
| `wire-title-generation` | Medium | 5-7 | Medium — async + model call |
| `permission-system` | **High** | 10-12 | Medium — TUI + web blocking |
| `wire-compaction` | Medium | 6-8 | Medium — token counting + LLM call |
| `subagent-delegation` | **High** | 12-15 | **High** — child sessions + restricted tools |
| `auto-compact` | Low | 2-3 | Low — config flag |
| `mcp-session-context` | Medium | 5-7 | Low — context passing |
| `lsp-integration` | Medium | 6-8 | Medium — external process management |

**Total estimated: ~73-96 tasks across 11 SDD changes**

---

## Notes

- All SDD changes use **Strict TDD Mode** (project convention)
- Test command: `cargo test --lib -p <crate>` (never bare `cargo test --lib`)
- `.gitignore` has `*` — all new files need `git add -f`
- Config paths (`~/.config/opencode/`) preserved for backward compatibility
- The project is called **RCode** — all branding uses this name
