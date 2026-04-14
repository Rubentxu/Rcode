# Skill Registry

## Overview
Central registry of all available agent skills for the rust-code project.

## Skill Index

### SDD Workflow (gentleman-programming)

| Skill | Source | Purpose | Trigger Phrases |
|-------|--------|---------|-----------------|
| sdd-init | ~/.config/opencode/skills/ | Initialize SDD project context | sdd init, iniciar sdd, openspec init |
| sdd-explore | ~/.config/opencode/skills/ | Explore and investigate ideas before a change | investigate, think through, clarify requirements |
| sdd-propose | ~/.config/opencode/skills/ | Create change proposal with intent/scope/approach | create proposal, update proposal |
| sdd-spec | ~/.config/opencode/skills/ | Write specifications with requirements and scenarios | write specs, update specs, delta specs |
| sdd-design | ~/.config/opencode/skills/ | Create technical design with architecture decisions | write design, update design |
| sdd-tasks | ~/.config/opencode/skills/ | Break down change into implementation tasks | create tasks, update tasks, task breakdown |
| sdd-apply | ~/.config/opencode/skills/ | Implement tasks writing actual code | implement, apply tasks, write code |
| sdd-verify | ~/.config/opencode/skills/ | Validate implementation matches specs/design/tasks | verify, validate implementation |
| sdd-archive | ~/.config/opencode/skills/ | Archive completed change | archive, close change |
| sdd-onboard | ~/.config/opencode/skills/ | Guided walkthrough of SDD workflow | onboard, walkthrough |

### Agent Teams Lite (gentleman-programming)

| Skill | Source | Purpose | Trigger Phrases |
|-------|--------|---------|-----------------|
| branch-pr | ~/.config/opencode/skills/ | Create pull requests | create PR, branch-pr, open PR |
| issue-creation | ~/.config/opencode/skills/ | Create GitHub issues | create issue, report bug, request feature |
| judgment-day | ~/.config/opencode/skills/ | Parallel adversarial review | judgment day, doble review, juzgar |

### Rust Development

| Skill | Source | Purpose | Trigger Phrases |
|-------|--------|---------|-----------------|
| rust-testing | ~/.agents/skills/ | Rust testing patterns (unit, integration, async) | cargo test, mockall, proptest, tokio test |
| rust-call-graph | ~/.agents/skills/ | Visualize function call graphs via LSP | /call-graph, call hierarchy, who calls |
| rust-refactor-helper | ~/.agents/skills/ | Safe refactoring with LSP analysis | /refactor, rename symbol, extract function |
| rust-symbol-analyzer | ~/.agents/skills/ | Analyze project structure via LSP symbols | /symbols, project structure, list structs |

### Browser Automation

| Skill | Source | Purpose | Trigger Phrases |
|-------|--------|---------|-----------------|
| agent-browser | ~/.config/opencode/skills/ | Browser automation CLI for AI agents | open website, fill form, click button, screenshot |
| chrome-devtools | ~/.agents/skills/ | Puppeteer-based browser automation and debugging | automate browser, take screenshot, performance |
| playwright-cli | ~/.agents/skills/ | Playwright browser automation for testing | navigate website, fill forms, test web app |

### General Tools

| Skill | Source | Purpose | Trigger Phrases |
|-------|--------|---------|-----------------|
| skill-creator | ~/.config/opencode/skills/ | Create new agent skills | create skill, add agent instructions |
| find-skills | ~/.config/opencode/skills/ | Discover and install agent skills | find skill, is there a skill for |
| docs-writer | ~/.agents/skills/ | Write/edit documentation | /docs, .md files, documentation |
| debugging-strategies | ~/.agents/skills/ | Systematic debugging techniques | investigate bug, performance issue, unexpected behavior |
| braintrust-tracing | ~/.agents/skills/ | Tracing for Claude Code sessions | (system/internal) |
| go-testing | ~/.config/opencode/skills/ | Go testing patterns for Bubbletea TUI | go test, teatest, test coverage |

### UI/UX

| Skill | Source | Purpose | Trigger Phrases |
|-------|--------|---------|-----------------|
| ui-ux-pro-max | ~/.agents/skills/ | UI/UX design and implementation patterns | UI, UX, design, layout |

## Statistics
- Total Skills: 26
- SDD Skills: 10
- Agent Teams Skills: 3
- Rust Skills: 4
- Browser Skills: 3
- General Tools: 6
- Last Updated: 2026-04-14

## Project Conventions
- AGENTS.md: Engram persistent memory protocol + RCode agent contract
- Language: User speaks Spanish, code in English
- Testing: `cargo test -p <crate>` (never bare `cargo test --lib`)
- SDD methodology with Agent Teams orchestration
- SDD persistence mode: engram
- Strict TDD Mode: enabled
- .gitignore: `*` (ignores everything by default) — use `git add -f` for new files

## Scan Directories

| Directory | Skills Found |
|-----------|-------------|
| ~/.config/opencode/skills/ | 16 (sdd-init, sdd-explore, sdd-propose, sdd-spec, sdd-design, sdd-tasks, sdd-apply, sdd-verify, sdd-archive, sdd-onboard, branch-pr, issue-creation, judgment-day, skill-creator, skill-registry, go-testing) |
| ~/.agents/skills/ | 12 (rust-testing, rust-call-graph, rust-refactor-helper, rust-symbol-analyzer, agent-browser, chrome-devtools, playwright-cli, ui-ux-pro-max, braintrust-tracing, debugging-strategies, docs-writer, find-skills) |
| ~/.claude/skills/ | 4 (judgment-day, sdd-onboard, skill-creator, go-testing) |
| ~/.opencode/skills/ | 0 |
| .claude/skills/ (project) | Not found |
| .agent/skills/ (project) | Not found |

## Notes
Auto-generated registry. Duplicates (same skill name across directories) are resolved by project-level > user-level priority. No project-level skill directories found.
