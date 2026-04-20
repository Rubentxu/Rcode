# RCode

A Rust-first AI coding agent platform with native performance, designed for tool execution, session persistence, and multi-provider interoperability.

## Overview

RCode is an AI coding agent built entirely in Rust, targeting high performance and native execution across multiple interfaces: HTTP API, interactive TUI, web client, and native desktop via Tauri.

The agent excels at code tasks by combining a powerful tool system with streaming responses and multi-provider support. It can read, write, and edit files, execute shell commands, search codebases, delegate to sub-agents, and integrate with external services via MCP.

## Key Features

- **Native Performance** — Built in Rust with Tokio for async, delivering efficient tool execution and low-latency streaming
- **Multi-Provider** — Works with Anthropic, OpenAI, Google, MiniMax, ZAI, OpenRouter, and other OpenAI-compatible providers
- **Tool System** — 17+ built-in tools: bash, read, write, edit, glob, grep, task, skill, webfetch, websearch, MCP, and more
- **Streaming SSE** — Real-time event streaming for text, reasoning, and tool execution
- **Multi-Client** — HTTP API server, interactive TUI, web UI, and Tauri desktop app
- **Session Persistence** — SQLite-backed session storage with message history
- **Skills & Commands** — Load specialized skill instructions and slash commands at runtime
- **MCP Integration** — Connect to Model Context Protocol servers for extended capabilities
- **Privacy Gateway** — Data sanitization hooks and security monitoring (via `crates/privacy`)
- **CogniCode** — Code intelligence snapshot injection for enhanced context (via `crates/cognicode`)

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    Client Layer                               │
│         (CLI / TUI / Web / Tauri Desktop)                    │
└────────────────────────────┬─────────────────────────────────┘
                             │ HTTP + SSE
┌────────────────────────────▼─────────────────────────────────┐
│                    rcode-server (Axum)                        │
│  submit_prompt() ──► AgentExecutor Loop                      │
│                             │                                 │
│         ┌───────────────────┼───────────────────┐              │
│         ▼                   ▼                   ▼              │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ LlmProvider │  │ToolRegistry  │  │  EventBus    │       │
│  │ (streaming) │  │  Service     │  │  (SSE pub)  │       │
│  └─────────────┘  └──────────────┘  └──────────────┘       │
│                             │                                 │
│         ┌───────────────────┼───────────────────┐              │
│         ▼                   ▼                   ▼              │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ Core Tools  │  │  MCP Tools   │  │  Skills/     │       │
│  │ (bash,etc.) │  │              │  │  Commands    │       │
│  └─────────────┘  └──────────────┘  └──────────────┘       │
└──────────────────────────────────────────────────────────────┘
```

## Repository Structure

```
rcode/
├── crates/                    # Rust workspace members
│   ├── core/                  # Domain types, traits, Message/Part models
│   ├── agent/                # AgentExecutor, DefaultAgent, subagent management
│   ├── session/              # Session service with compaction
│   ├── tools/                # Tool implementations + registry
│   ├── providers/            # LLM provider adapters (OpenAI, Anthropic, etc.)
│   ├── server/               # HTTP server with Axum, REST API, SSE
│   ├── storage/              # SQLite persistence layer
│   ├── event/                # EventBus for SSE streaming
│   ├── cli/                  # CLI application (run, serve, tui commands)
│   ├── tui/                  # Interactive terminal UI (Ratatui)
│   ├── config/               # Configuration loading and management
│   ├── mcp/                  # MCP client and server registry
│   ├── lsp/                  # LSP client for code intelligence
│   ├── engram/              # Persistent memory system
│   ├── acp/                  # Agent Communication Protocol
│   ├── privacy/              # Privacy gateway service
│   ├── cognicode/            # Code intelligence injection
│   ├── observability/        # Tracing and metrics
│   ├── plugins/              # Plugin loader and manager
│   └── gen-types/           # Type generation for frontend
├── web/                      # Web client (SolidJS + Vite + Tailwind)
│   ├── src/                  # SolidJS components and API client
│   ├── e2e/                  # Playwright E2E tests
│   └── src-tauri/            # Tauri desktop configuration
└── docs/                     # Architecture and design docs
```

## Getting Started

### Prerequisites

- **Rust 1.85+** (edition 2024)
- **SQLite** (for session persistence)
- **API key** for your chosen provider (Anthropic, OpenAI, etc.)

### Build from Source

```bash
# Clone the repository
git clone <your-fork-url>
cd rust-code

# Build the entire workspace
cargo build --workspace

# Or build just the CLI
cargo build -p rcode-cli
```

### Quick Start

```bash
# Set your API key
export ANTHROPIC_API_KEY=sk-ant-...

# Run with a direct prompt
cargo run -p rcode-cli -- run --message "Explain Rust's ownership model"

# Start the HTTP API server
cargo run -p rcode-cli -- serve

# Launch the interactive TUI
cargo run -p rcode-cli -- tui
```

The server starts on `http://127.0.0.1:4096` by default.

## Configuration

RCode searches for configuration files in this order:

1. Path passed via `--config <path>`
2. `./opencode.json` (current directory)
3. `~/.config/opencode/opencode.json` (Unix)
4. Project `.opencode/` directory configs
5. `OPENCODE_CONFIG_CONTENT` env var (inline JSON)
6. `~/.config/rcode/config.json` (RCode overlay — highest precedence for RCode-specific fields)

### Example Configuration

```json
{
  "model": "anthropic/claude-sonnet-4-5",
  "providers": {
    "anthropic": {
      "api_key": "${ANTHROPIC_API_KEY}"
    },
    "openai": {
      "api_key": "${OPENAI_API_KEY}"
    }
  },
  "server": {
    "port": 4096
  }
}
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `OPENAI_API_KEY` | OpenAI API key |
| `OPENROUTER_API_KEY` | OpenRouter API key |
| `MINIMAX_API_KEY` | MiniMax API key |
| `OPENCODE_CONFIG_CONTENT` | Inline config JSON (highest precedence after `--config` flag) |

## CLI Commands

### `run` — Direct Execution

```bash
rcode run [OPTIONS]

Options:
  -m, --message <MESSAGE>    Direct message input
  -f, --file <FILE>          Read prompt from file
      --stdin                Read from stdin
      --json                 Output as JSON
      --silent               Suppress stdout
      --save-session <BOOL>  Persist session (default: true)
  -s, --model <MODEL>        Model to use
```

### `serve` — HTTP Server Mode

```bash
rcode serve [OPTIONS]

Options:
  -p, --port <PORT>     Port to listen on (default: 4096)
  -h, --hostname <HOST>  Hostname to bind to (default: 127.0.0.1)
```

### `tui` — Interactive Terminal UI

```bash
rcode tui
```

## HTTP API

### Session Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check |
| GET | `/session` | List all sessions |
| POST | `/session` | Create new session |
| GET | `/session/:id` | Get session by ID |
| DELETE | `/session/:id` | Delete session |
| GET | `/session/:id/messages` | Get session messages (paginated) |
| POST | `/session/:id/prompt` | Submit prompt to session |
| POST | `/session/:id/abort` | Abort running session |
| GET | `/session/:id/events` | SSE stream for session events |
| GET | `/event` | SSE stream for all events |

### Example Usage

```bash
# Create session
curl -X POST http://localhost:4096/session \
  -H "Content-Type: application/json" \
  -d '{"project_path": "/path/to/project"}'

# Submit prompt
curl -X POST http://localhost:4096/session/<session_id>/prompt \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Hello, world!"}'

# Stream events
curl http://localhost:4096/session/<session_id>/events
```

## Tool System

RCode's default tool registry includes approximately 20 built-in tools:

| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands |
| `read` | Read files from the filesystem |
| `write` | Write content to files |
| `edit` | Make targeted edits using oldString/newString |
| `multiedit` | Apply multiple edits in a single operation |
| `glob` | Find files matching patterns |
| `grep` | Search file contents |
| `codesearch` | Search codebases with structured results |
| `task` | Delegate work to a sub-agent |
| `delegate` / `delegation_read` | Create and read delegation records |
| `skill` | Load specialized skill instructions |
| `slash_command` | Execute discovered slash commands |
| `plan` / `plan_exit` | Display and modify execution plans |
| `todowrite` | Manage task checklists |
| `question` | Ask the user clarifying questions |
| `webfetch` | Fetch content from URLs |
| `websearch` | Search the web |
| `applypatch` | Apply patches to files |
| `session_navigation` | Navigate and query session history |

Additional tools are available via integration:
- **MCP** — Access tools from Model Context Protocol servers

## Testing

```bash
# Run all workspace tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p rcode-cli
cargo test -p rcode-agent

# Run with output visible
cargo test --workspace -- --nocapture

# Run Clippy linting
cargo clippy --workspace --all-targets -- -D warnings
```

### Frontend Testing

```bash
cd web

# Run vitest unit tests
npm test

# Run Playwright E2E tests
npm run e2e:web
```

## Development

### Key Files

- `AGENTS.md` — Agent contract and validation policy for contributors
- `docs/architecture/mvp-architecture.md` — System architecture diagrams
- `docs/architecture-agent-system.md` — Deep dive into the agent executor loop
- `docs/analysis/opencode-vs-rcode.md` — Comparative analysis with OpenCode

### Code Generation

Generate TypeScript types from Rust types for the frontend:

```bash
cd web
npm run types:generate
```

## License

MIT
