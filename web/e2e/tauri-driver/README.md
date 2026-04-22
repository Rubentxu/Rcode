# Tauri desktop e2e with tauri-driver

This directory contains the native desktop smoke suite for the Tauri app using
[tauri-driver](https://github.com/tauri-apps/tauri/tree/dev/tooling/tauri-driver)
and WebdriverIO.

Use this suite when you need confidence in the real desktop application, not
just the web frontend.

## What this validates

The smoke path covers:

1. **Startup** — window title, embedded backend API reachability, providers/models
2. **Session management** — sidebar, new session creation, session selection, API listing
3. **Basic messaging** — textarea visibility/enabled state, prompt submission, user message rendering
4. **Tool calling** — bash tool execution, UI result display, structured persistence via API
5. **Streaming** — Processing... indicator during response
6. **Settings panel** — open/close, navigation, providers/models sections
7. **Abort** — Stop button appears during processing and is clickable
8. **Session UX** — project branch label, session rename, search/filter, compact mode, date grouping, auto-title propagation

## Prerequisites

- Rust toolchain installed
- Node.js ≥ 18 installed
- `cargo install tauri-driver --locked`
- On Linux headless/CI, `xvfb-run` should be available (used automatically)

## Install test dependencies

```bash
cd web/e2e/tauri-driver
npm install
```

## Run locally

```bash
cd web/e2e/tauri-driver
npm test
```

On Linux headless (or CI):

```bash
cd web/e2e/tauri-driver
xvfb-run -a npm test
```

## Run with a specific spec

```bash
cd web/e2e/tauri-driver
npm test -- --spec=./specs/tool-calling.spec.mjs
```

## Environment

The suite expects the Tauri app to resolve its provider configuration from the
environment or repo config, just like normal app usage.

For a tool-calling smoke test with MiniMax Anthropic-compatible, a typical
setup is:

```bash
export ANTHROPIC_BASE_URL=https://api.minimax.io/anthropic
export ANTHROPIC_AUTH_TOKEN=...
export ANTHROPIC_MODEL=MiniMax-M2.7-highspeed
```

## Architecture

- `wdio.conf.mjs` — WebdriverIO config; builds Tauri app in `onPrepare`, starts
  tauri-driver in `beforeSession`, shuts it down in `afterSession`
- `specs/tool-calling.spec.mjs` — core smoke: creates session, runs `bash pwd`,
  verifies result in UI and structured `tool_call`/`tool_result` parts via API
- `specs/comprehensive.spec.mjs` — broader coverage across all 7 areas above
- `specs/session-ux.spec.mjs` — session UX features: branch label, rename, search, compact mode, date groups, auto-title

## Known issues / limitations

- The Settings navigation button selectors (`button=General`, etc.) are fragile due to
  Solid.js inline-style rendering. The Settings open/close and section content tests
  pass reliably; nav item button detection may need refinement for CI reliability.
- Streaming is validated via presence of the "Processing..." indicator; no fine-grained
  SSE timing measurement.
- Abort test requires a prompt that takes long enough to show the Stop button; the
  current "count from 1 to 100" prompt generally works but timing can vary.
- Session UX tests use `browser.execute()` for DOM queries to avoid stale WebdriverIO
  element handles when Solid.js re-renders the session list during UI state changes.

## Current test status

| Test file | Passing | Total |
|-----------|---------|-------|
| `tool-calling.spec.mjs` | 1 | 1 |
| `comprehensive.spec.mjs` | 14 | 19 |
| `session-ux.spec.mjs` | pending local run | 6 |
| `orchestrator-features.spec.mjs` | pending | 15 |
| **Total** | **15 + others** | **41** |

The 5 failing tests in `comprehensive.spec.mjs` are in the Settings navigation
area (timing/selector issues) and the Abort test (prompt timing). These are
pre-existing issues unrelated to the session-ux change. Run the session-ux spec
directly to validate the new coverage in isolation.

## Orchestrator Features Spec

The `orchestrator-features.spec.mjs` validates ReflexiveOrchestrator features:

1. **Worker Agents** — verify explore, implement, test, verify, research agents are registered
2. **Orchestrator Initialization** — session creates with runtime and registry
3. **Delegation** — child sessions created via delegation mechanism
4. **Session Status** — correct status transitions (created → idle → running)
5. **Tool Calls** — tool calls recorded in session messages
6. **Multiple Sessions** — multiple concurrent sessions coexist
7. **Message Persistence** — session messages persisted correctly
8. **Project Switching** — sessions list updates when switching projects
9. **Session Deletion** — sessions can be deleted
10. **Agent Tools** — agent info includes supported tools
11. **Entropy Evaluation** — high tool diversity triggers delegation
12. **Error Handling** — tool errors recorded correctly
13. **GREEN Zone** — simple prompts execute without delegation
14. **Tool Tracking** — tool usage tracked for Thompson Sampling
15. **Contextual Responses** — agent provides contextually relevant responses
