# Skill Registry

**Delegator use only.** Any agent that launches sub-agents reads this registry to resolve compact rules, then injects them directly into sub-agent prompts. Sub-agents do NOT read this registry or individual SKILL.md files.

## User Skills

| Trigger | Skill | Path |
|---------|-------|------|
| /call-graph, call hierarchy, who calls, what calls, 调用图, 调用关系, 谁调用了, 调用了谁 | rust-call-graph | /home/rubentxu/.agents/skills/rust-call-graph/SKILL.md |
| /refactor, rename symbol, move function, extract, 重构, 重命名, 提取函数, 安全重构 | rust-refactor-helper | /home/rubentxu/.agents/skills/rust-refactor-helper/SKILL.md |
| /symbols, project structure, list structs, list traits, list functions, 符号分析, 项目结构, 列出所有, 有哪些struct | rust-symbol-analyzer | /home/rubentxu/.agents/skills/rust-symbol-analyzer/SKILL.md |
| Rust testing, cargo test, mockall, proptest, tokio test, test organization | rust-testing | /home/rubentxu/.agents/skills/rust-testing/SKILL.md |
| braintrust tracing, hook architecture, sub-agent correlation | braintrust-tracing | /home/rubentxu/.agents/skills/braintrust-tracing/SKILL.md |
| UI/UX design, plan, build, create, design, implement, review, fix, improve, optimize, enhance, refactor, check UI/UX code | ui-ux-pro-max | /home/rubentxu/.agents/skills/ui-ux-pro-max/SKILL.md |
| browser, open website, fill form, click button, take screenshot, scrape data, test web app, automate browser | agent-browser | /home/rubentxu/.config/opencode/skills/agent-browser/SKILL.md |
| pull request, PR, open PR, create PR, prepare changes for review | branch-pr | /home/rubentxu/.config/opencode/skills/branch-pr/SKILL.md |
| chrome devtools, performance, network traffic, web scraping, Puppeteer | chrome-devtools | /home/rubentxu/.config/opencode/skills/chrome-devtools/SKILL.md |
| debug, debugging, bug, investigate, root cause, profiling, performance | debugging-strategies | /home/rubentxu/.config/opencode/skills/debugging-strategies/SKILL.md |
| documentation, docs, /docs, .md files, write docs, review docs | docs-writer | /home/rubentxu/.config/opencode/skills/docs-writer/SKILL.md |
| find skill, search skill, how do I, is there a skill, capability | find-skills | /home/rubentxu/.config/opencode/skills/find-skills/SKILL.md |
| GitHub issue, create issue, report bug, feature request, bug report | issue-creation | /home/rubentxu/.config/opencode/skills/issue-creation/SKILL.md |
| judgment day, adversarial review, dual review, doble review, juzgar, que lo juzguen | judgment-day | /home/rubentxu/.config/opencode/skills/judgment-day/SKILL.md |
| playwright, web testing, form filling, screenshots, data extraction | playwright-cli | /home/rubentxu/.config/opencode/skills/playwright-cli/SKILL.md |
| create skill, new skill, add agent instructions, document patterns | skill-creator | /home/rubentxu/.config/opencode/skills/skill-creator/SKILL.md |

## Compact Rules

Pre-digested rules per skill. Delegators copy matching blocks into sub-agent prompts as `## Project Standards (auto-resolved)`.

### rust-testing
- Use `cargo test -p <crate>` for focused crate-level validation — never `cargo test --lib`
- Run `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` for shared infrastructure changes
- Design for testability: use traits, avoid mocking owned types, prefer mockall automock for complex mocking
- Use `tokio::test` for async functions, `assert_cmd` for CLI testing, and proptest for property-based testing
- Tests touching environment variables MUST sanitize and restore them
- Add regression tests for every bug that escaped once
- Never rely on developer shell environment in tests unless explicitly testing environment resolution
- Use `cargo-nextest` for faster CI test execution

### rust-refactor-helper
- Always use `--dry-run` first to preview changes before applying
- Use LSP `findReferences` to find ALL references before renaming symbols
- Categorize changes by file and check for name conflicts, visibility changes, and macro-generated code
- For extract function: identify inputs, outputs, and side effects; generate new signature before replacing
- Verify reference completeness, name conflicts, visibility changes, macro-generated code, documentation, and test coverage
- Check for circular dependencies before moving symbols
- Provide impact analysis showing definition, all references, and potential issues

### rust-symbol-analyzer
- Use LSP `documentSymbol` for single-file analysis and `workspaceSymbol` for entire project
- Categorize symbols by type: structs, traits, functions, enums, modules
- Generate project structure visualization with module hierarchy
- Provide complexity metrics: function count, lines, complexity rating per file
- Show dependency analysis: what a file imports and what imports from it

### rust-call-graph
- Use LSP `prepareCallHierarchy` first, then `incomingCalls` (callers) or `outgoingCalls` (callees)
- Support `--depth N` for traversal depth and `--direction in|out|both` for call direction
- Generate ASCII tree visualization for call hierarchies
- Provide analysis insights: entry points, leaf functions, hot paths, and potential issues
- Export to Mermaid format for documentation when requested

### debugging-strategies
- Apply scientific method: observe → hypothesize → experiment → analyze → repeat
- Reproduce the problem consistently before attempting fixes
- Isolate the problem: remove complexity until minimal reproduction case
- Use binary search debugging: comment out half the code, narrow down section
- Check recent changes first: most bugs are recent
- Never make multiple changes at once — change one thing at a time
- Use proper debugging tools (debugger, logging) instead of just print statements
- Document findings to help future debugging efforts

### docs-writer
- Use active voice and present tense; address developers as "you"
- Follow BLUF (Bottom Line Up Front) — start with domain purpose before technical details
- Wrap text at 80 characters except for Rust code blocks
- Use meaningful domain names in examples (e.g., `Order`, `CustomerId`, `PaymentAmount`)
- Verify Rust code examples compile and type signatures match actual code
- Document all possible errors and performance considerations
- Check bounded context scope and aggregate responsibilities align with ubiquitous language

### branch-pr
- Every PR MUST link an approved issue — no exceptions
- Every PR MUST have exactly one `type:*` label
- Branch names must match: `^(feat|fix|chore|docs|style|refactor|perf|test|build|ci|revert)/[a-z0-9._-]+$`
- Commit messages must follow conventional commits: `^(build|chore|ci|docs|feat|fix|perf|refactor|revert|style|test)(\([a-z0-9\._-]+\))?!?: .+`
- Run `shellcheck` on modified shell scripts before pushing
- Automated checks must pass before merge is possible

### issue-creation
- Blank issues are disabled — MUST use a template (bug report or feature request)
- Every issue gets `status:needs-review` automatically on creation
- A maintainer MUST add `status:approved` before any PR can be opened
- Search existing issues for duplicates before creating new ones
- Questions go to Discussions, not issues

### agent-browser / playwright-cli
- Use for web testing, form filling, screenshots, data extraction, and browser automation
- Support navigating pages, clicking buttons, filling forms, taking screenshots
- Good for: create session, send prompt, SSE/rendering flow, toast/error handling, tool-calling smoke tests

## Project Conventions

| File | Path | Notes |
|------|------|-------|
| AGENTS.md | /home/rubentxu/Proyectos/rust/rust-code/AGENTS.md | RCode agent contract with validation matrix |

## RCode-Specific Validation Rules (from AGENTS.md)

### Product Invariants (Never Break)
- Tool-calling must work end to end for supported providers
- Streaming and non-streaming paths must remain semantically distinct
- Session messages must preserve structured parts: `text`, `reasoning`, `tool_call`, `tool_result`
- Web client and Tauri desktop flow must work against the same backend API contract
- Provider-specific behavior must NOT leak into unrelated providers

### Architecture Invariants
- `LlmProvider` is the stable application port
- Provider implementations are adapters, not the domain contract
- Shared OpenAI-compatible behavior lives in `crates/providers/src/openai_compat/`
- Concrete providers (OpenAI, MiniMax, OpenRouter, ZAI) must prefer composition over inheritance-like delegation
- History serialization must preserve provider wire-format expectations

### Validation Matrix (Required Proof)
1. **Unit/crate-level**: `cargo test -p <crate>` for focused changes
2. **Workspace regression**: `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings`
3. **Web e2e**: `cd web && npx playwright test` for UI/API changes
4. **Tauri desktop e2e**: Required when touching embedded backend, `get_backend_url`, Tauri commands, or desktop-only behavior

### Production-Readiness Checklist
- crate-level tests are green
- workspace tests are green if shared code changed
- clippy is green with warnings denied
- web e2e smoke tests pass
- desktop/Tauri smoke path passes when desktop wiring is touched
- bugfixes have regression tests
- no hidden dependence on local shell env in tests

### What NOT to Do
- Do NOT flatten structured tool history into plain text
- Do NOT mark a refactor complete just because compile passes
- Do NOT rely only on mocked tests for streaming or tool-calling changes
- Do NOT leave environment-sensitive tests unsanitized
- Do NOT archive or declare production-ready if relevant gates are still red
