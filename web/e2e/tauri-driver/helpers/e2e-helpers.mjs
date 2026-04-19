/**
 * Shared E2E test helpers for RCode Tauri driver specs.
 *
 * All e2e specs MUST use E2E_MODEL and createSessionWithModel() to guarantee
 * a predictable, cost-effective model is used for every test run.
 */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

// ─── Constants ────────────────────────────────────────────────────────────────

export const API_BASE = 'http://127.0.0.1:4098';

/**
 * The model used for all e2e tests.
 * Using MiniMax-M2.7-highspeed to avoid expensive model costs during testing.
 */
export const E2E_MODEL = 'minimax/MiniMax-M2.7-highspeed';

// ─── Backend helpers ───────────────────────────────────────────────────────────

/**
 * Wait until the embedded backend is reachable.
 */
export async function waitForBackend(timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${API_BASE}/session`);
      if (res.ok) return;
    } catch (_) {}
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error('Embedded backend did not become ready');
}

/**
 * Generic polling helper.
 */
export async function waitFor(condition, timeoutMs = 40_000, intervalMs = 300) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await condition()) return;
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new Error(`waitFor timed out after ${timeoutMs}ms`);
}

/**
 * Fetch JSON from the backend API.
 */
export async function fetchJson(url, options = {}) {
  const res = await fetch(url, options);
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  return res.json();
}

// ─── Temp project helpers ─────────────────────────────────────────────────────

/**
 * Create a temporary git project directory.
 * Returns the path. Caller is responsible for cleanup via restoreState({ deleteTempDirs: [...] }).
 */
export function setupTempGitProject(prefix = 'rcode-e2e') {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), `${prefix}-`));
  fs.writeFileSync(path.join(root, 'README.md'), '# E2E Test Project\n', 'utf8');
  execFileSync('git', ['init', '-b', 'main'], { cwd: root, stdio: 'ignore' });
  execFileSync('git', ['config', 'user.email', 'e2e@example.com'], { cwd: root, stdio: 'ignore' });
  execFileSync('git', ['config', 'user.name', 'RCode E2E'], { cwd: root, stdio: 'ignore' });
  execFileSync('git', ['add', 'README.md'], { cwd: root, stdio: 'ignore' });
  execFileSync('git', ['commit', '-m', 'init'], { cwd: root, stdio: 'ignore' });
  return root;
}

export async function createProject(projectPath, name) {
  return fetchJson(`${API_BASE}/projects`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path: projectPath, name }),
  });
}

export async function deleteProject(projectId) {
  const response = await fetch(`${API_BASE}/projects/${encodeURIComponent(projectId)}`, { method: 'DELETE' });
  if (!response.ok && response.status !== 404) {
    throw new Error(`Failed to delete project ${projectId}: ${response.status}`);
  }
}

export async function deleteSession(sessionId) {
  const response = await fetch(`${API_BASE}/session/${sessionId}`, { method: 'DELETE' });
  if (!response.ok && response.status !== 404) {
    throw new Error(`Failed to delete session ${sessionId}: ${response.status}`);
  }
}

export async function deleteAllSessions() {
  const sessions = await fetchJson(`${API_BASE}/session`).catch(() => []);
  for (const s of sessions) {
    await fetch(`${API_BASE}/session/${s.id}`, { method: 'DELETE' }).catch(() => {});
  }
}

// ─── Session helpers ───────────────────────────────────────────────────────────

/**
 * Create a new session via the backend API using E2E_MODEL, then navigate to it
 * in the UI by clicking its entry in the sessions list.
 *
 * The key insight: setting localStorage alone does NOT update the SolidJS reactive
 * state. The UI uses globalStore.activeProjectId() (reactive) to determine which
 * project's sessions to load. We must use debugSwitchProject() to trigger the
 * full reactive chain:
 *   debugSwitchProject → rcode:debug-switch-project event →
 *   projectContext.setActiveProject() → globalStore.setActiveProject() →
 *   createEffect → workspace.switchProject() [populates cache] + loadSessions()
 *
 * Returns: { sessionId, textarea }
 *
 * Usage in before():
 *   let sessionId, textarea;
 *   before(async () => {
 *     await waitForBackend();
 *     ({ sessionId, textarea } = await createSessionWithModel());
 *   });
 */
export async function createSessionWithModel(projectPath = '/tmp') {
  // 1. Resolve a valid project from the backend (or use first available project)
  let resolvedProjectPath = projectPath;
  let resolvedProjectId = null;
  try {
    const projects = await fetchJson(`${API_BASE}/projects`);
    if (Array.isArray(projects) && projects.length > 0) {
      resolvedProjectPath = projects[0].canonical_path || projects[0].path || projectPath;
      resolvedProjectId = projects[0].id;
    }
  } catch (_) {
    // fallback to provided projectPath
  }

  if (!resolvedProjectId) {
    throw new Error('No projects available — cannot create session');
  }

  // 2. Wait for the debug inspector to be available (loaded via dynamic import).
  //    This is critical — the inspector may not be ready immediately after page load.
  const inspectorReady = await waitForDebugInspector();

  // 3. Activate the project in the REACTIVE state (not just localStorage).
  //    debugSwitchProject sets localStorage AND dispatches rcode:debug-switch-project
  //    → projectContext.setActiveProject() → globalStore.setActiveProject()
  //    → createEffect → workspace.switchProject() [populates cache] + loadSessions()
  if (inspectorReady) {
    await debugSwitchProject(resolvedProjectId);
    // Wait for the reactive chain to settle: createEffect → switchProject + loadSessions
    await new Promise((r) => setTimeout(r, 1500));
  } else {
    // Fallback: Debug inspector not available (race condition with dynamic import).
    // Set localStorage and do a full page reload, then wait for app initialization.
    console.warn('[e2e] Debug inspector not available — using browser.refresh() fallback');
    await browser.execute((id) => localStorage.setItem('rcode:active-project', id), resolvedProjectId);
    await browser.refresh();
    await waitForBackend();
    await new Promise((r) => setTimeout(r, 3000));
  }

  // 3. Create session with the target model
  const session = await fetchJson(`${API_BASE}/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      project_path: resolvedProjectPath,
      model_id: E2E_MODEL,
    }),
  });

  const sessionId = session.id;

  // 4. Reload sessions to pick up the newly created one
  if (inspectorReady) {
    await debugRefreshSessions();
    await new Promise((r) => setTimeout(r, 500));
  } else {
    // Fallback: refresh page and wait for sessions to load
    await browser.refresh();
    await waitForBackend();
    await new Promise((r) => setTimeout(r, 3000));
  }

  // 5. Navigate to the session in the UI
  await navigateToSession(sessionId);

  // 6. Wait for the textarea to be ready
  const textarea = await $('[data-component="textarea"]');
  await textarea.waitForExist({ timeout: 30_000 });

  return { sessionId, textarea };
}

/**
 * Click on a session item in the UI sessions list by its ID.
 *
 * IMPORTANT: The caller (createSessionWithModel) is responsible for ensuring
 * the correct project is active and sessions have been loaded before calling
 * this function. This function only handles the UI interaction of finding
 * and clicking the session element.
 */
export async function navigateToSession(sessionId) {
  // Click sessions tab
  const sessionsTab = await $('[data-tab="sessions"]');
  await sessionsTab.waitForExist({ timeout: 15_000 });
  await sessionsTab.click();
  await new Promise((r) => setTimeout(r, 500));

  // Wait for session to appear and click it
  const sessionItem = await $(`[data-session-id="${sessionId}"]`);
  await sessionItem.waitForExist({ timeout: 15_000 });
  // Scroll into view to avoid click interception from overlays
  await sessionItem.scrollIntoView({ block: 'center' });
  await new Promise((r) => setTimeout(r, 200));
  await sessionItem.click();
}

// ─── State snapshot / restore helpers ─────────────────────────────────────────

/**
 * Capture the full backend + browser state before a test suite that mutates
 * projects or localStorage.  Call in before().
 *
 * Returns a snapshot object to pass to restoreState().
 */
export async function captureState() {
  const projects = await fetchJson(`${API_BASE}/projects`).catch(() => []);
  const sessions = await fetchJson(`${API_BASE}/session`).catch(() => []);
  const mruValue = await browser.execute(() => localStorage.getItem('rcode:active-project'));
  return {
    projects: Array.isArray(projects) ? projects : [],
    sessions: Array.isArray(sessions) ? sessions : [],
    mruValue,
  };
}

/**
 * Restore the backend + browser state to what captureState() recorded.
 *
 * - Deletes any project whose ID was NOT present in the snapshot.
 * - Does NOT recreate projects that were deleted during the test (backend
 *   cannot recreate arbitrary projects — use temp dirs and the API instead).
 * - Deletes any session whose ID was NOT present in the snapshot.
 * - Restores localStorage['rcode:active-project'] to the original value.
 *
 * Call in after().
 */
export async function restoreState(snapshot, { deleteTempDirs = [] } = {}) {
  // 1. Delete projects created during the test (not in the original snapshot)
  const snapshotIds = new Set(snapshot.projects.map((p) => p.id));
  const currentProjects = await fetchJson(`${API_BASE}/projects`).catch(() => []);
  for (const p of currentProjects) {
    if (!snapshotIds.has(p.id)) {
      await fetch(`${API_BASE}/projects/${encodeURIComponent(p.id)}`, { method: 'DELETE' });
    }
  }

  // 2. Restore original projects that were deleted during the test
  for (const p of snapshot.projects) {
    const stillExists = currentProjects.some((cp) => cp.id === p.id);
    if (!stillExists) {
      // Re-register by path if possible
      const body = { path: p.path || p.canonical_path, name: p.name };
      await fetch(`${API_BASE}/projects`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      }).catch(() => {}); // best-effort — path may no longer exist
    }
  }

  // 3. Delete sessions that weren't in the snapshot
  const snapshotSessionIds = new Set((snapshot.sessions || []).map((s) => s.id));
  const currentSessions = await fetchJson(`${API_BASE}/session`).catch(() => []);
  for (const s of currentSessions) {
    if (!snapshotSessionIds.has(s.id)) {
      await fetch(`${API_BASE}/session/${s.id}`, { method: 'DELETE' }).catch(() => {});
    }
  }

  // 4. Restore localStorage
  if (snapshot.mruValue === null) {
    await browser.execute(() => localStorage.removeItem('rcode:active-project'));
  } else {
    await browser.execute((v) => localStorage.setItem('rcode:active-project', v), snapshot.mruValue);
  }

  // 5. Clean up temp dirs
  for (const dir of deleteTempDirs) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// ─── UI interaction helpers ────────────────────────────────────────────────────

/**
 * Type text into the prompt textarea and click Send.
 * Clears any existing content first.
 */
export async function submitPrompt(textarea, text) {
  await textarea.click();
  await textarea.scrollIntoView({ block: 'center' });
  await new Promise((r) => setTimeout(r, 100));
  await browser.keys(['Control', 'a']);
  await browser.keys(['Delete']);
  await textarea.setValue(text);
  await new Promise((r) => setTimeout(r, 150));

  const sendBtn = await $('[data-component="prompt-submit"]');
  await sendBtn.waitForExist({ timeout: 10_000 });
  await sendBtn.scrollIntoView({ block: 'center' });
  await new Promise((r) => setTimeout(r, 100));
  // Use JS click as fallback when WebDriver click is intercepted by overlays
  try {
    await sendBtn.click();
  } catch {
    await browser.execute(() => {
      const btn = document.querySelector('[data-component="prompt-submit"]');
      if (btn) btn.click();
    });
  }
}

/**
 * Wait for the textarea to be re-enabled (not disabled) after an LLM response.
 */
export async function waitForInputEnabled(timeoutMs = 60_000) {
  await waitFor(
    () =>
      browser.execute(() => {
        const el = document.querySelector('[data-component="textarea"]');
        if (!el) return false;
        return !el.disabled && el.getAttribute('aria-disabled') !== 'true';
      }),
    timeoutMs,
  );
}

// ─── Debug Inspector helpers ────────────────────────────────────────────────

/**
 * Wait for the Debug Inspector to become available.
 *
 * The inspector is loaded via dynamic import in App.tsx and may not be
 * immediately available after page load. Call this before using any
 * debug inspector functions.
 *
 * Returns true if the inspector became available, false if it timed out.
 */
export async function waitForDebugInspector(timeoutMs = 15_000) {
  try {
    await waitFor(
      () => browser.execute(() => !!window.__RCODE_DEBUG__),
      timeoutMs,
      300,
    );
    return true;
  } catch {
    return false;
  }
}

/**
 * Get a full app snapshot from the debug inspector.
 * Returns null if the inspector is not available.
 */
export async function debugSnapshot() {
  return browser.execute(() => {
    if (!window.__RCODE_DEBUG__) return null;
    return window.__RCODE_DEBUG__.getSnapshot();
  });
}

/**
 * Get info about a specific component by selector.
 */
export async function debugComponent(selector) {
  return browser.execute(
    (sel) => {
      if (!window.__RCODE_DEBUG__) return null;
      return window.__RCODE_DEBUG__.getComponent(sel);
    },
    selector,
  );
}

/**
 * Get all elements matching a data attribute query.
 * Query format: "data-component=explorer-tree" or "data-tab=sessions"
 */
export async function debugFindElements(query) {
  return browser.execute(
    (q) => {
      if (!window.__RCODE_DEBUG__) return [];
      return window.__RCODE_DEBUG__.findElements(q);
    },
    query,
  );
}

/**
 * Trigger a session reload from the debug inspector.
 * This dispatches an event that the app can listen to.
 */
export async function debugRefreshSessions() {
  return browser.execute(() => {
    if (!window.__RCODE_DEBUG__) return false;
    window.__RCODE_DEBUG__.triggerRefreshSessions();
    return true;
  });
}

/**
 * Trigger a project switch from the debug inspector.
 */
export async function debugSwitchProject(projectId) {
  return browser.execute(
    (id) => {
      if (!window.__RCODE_DEBUG__) return false;
      window.__RCODE_DEBUG__.triggerProjectSwitch(id);
      return true;
    },
    projectId,
  );
}

// ─── Message helpers ──────────────────────────────────────────────────────────

/**
 * Fetch messages for a session from the backend API.
 * Returns the full paginated response with structured parts.
 */
export async function getMessages(sessionId, { offset = 0, limit = 50 } = {}) {
  return fetchJson(`${API_BASE}/session/${sessionId}/messages?offset=${offset}&limit=${limit}`);
}

/**
 * Extract all parts from messages, flattened.
 * Returns: Array<{ type, name?, content?, ... }>
 */
export function extractParts(messages) {
  const msgs = messages?.messages || messages || [];
  return msgs.flatMap((m) => m.parts || []);
}

/**
 * Assert that a message payload contains specific part types.
 * Throws with a descriptive message if any expected part is missing.
 */
export function assertParts(messages, expectedParts) {
  const parts = extractParts(messages);
  for (const expected of expectedParts) {
    if (expected.type === 'tool_call') {
      const found = parts.some((p) => p.type === 'tool_call' && (!expected.name || p.name === expected.name));
      if (!found) {
        const toolCalls = parts.filter((p) => p.type === 'tool_call').map((p) => p.name);
        throw new Error(`Expected tool_call part${expected.name ? ` with name "${expected.name}"` : ''} but found: ${JSON.stringify(toolCalls)}`);
      }
    } else {
      const found = parts.some((p) => p.type === expected.type);
      if (!found) {
        const types = parts.map((p) => p.type);
        throw new Error(`Expected part type "${expected.type}" but found: ${JSON.stringify(types)}`);
      }
    }
  }
}

/**
 * Wait for a session to have at least N messages via the API.
 * Polls GET /session/:id/messages until the count is met.
 */
export async function waitForMessages(sessionId, minCount = 2, timeoutMs = 60_000) {
  await waitFor(async () => {
    const data = await getMessages(sessionId);
    const msgs = data?.messages || [];
    return msgs.length >= minCount;
  }, timeoutMs, 1000);
  return getMessages(sessionId);
}

/**
 * Wait for a tool_call part to appear in session messages via the API.
 */
export async function waitForToolCall(sessionId, toolName, timeoutMs = 60_000) {
  await waitFor(async () => {
    const data = await getMessages(sessionId);
    const parts = extractParts(data);
    return parts.some((p) => p.type === 'tool_call' && (!toolName || p.name === toolName));
  }, timeoutMs, 1000);
  return getMessages(sessionId);
}

/**
 * Wait for a tool_result part to appear in session messages via the API.
 */
export async function waitForToolResult(sessionId, timeoutMs = 60_000) {
  await waitFor(async () => {
    const data = await getMessages(sessionId);
    const parts = extractParts(data);
    return parts.some((p) => p.type === 'tool_result');
  }, timeoutMs, 1000);
  return getMessages(sessionId);
}

// ─── Session creation (API-only, no UI navigation) ─────────────────────────────

/**
 * Create a session via API only, without navigating to it in the UI.
 * Use this for test setup when you don't need the UI to show the session.
 *
 * Returns the full session object from the API.
 */
export async function createSessionDirect({ projectPath, modelId } = {}) {
  let resolvedPath = projectPath;
  if (!resolvedPath) {
    const projects = await fetchJson(`${API_BASE}/projects`);
    if (Array.isArray(projects) && projects.length > 0) {
      resolvedPath = projects[0].canonical_path || projects[0].path;
    }
  }
  if (!resolvedPath) {
    throw new Error('No project path available — cannot create session');
  }
  return fetchJson(`${API_BASE}/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      project_path: resolvedPath,
      model_id: modelId || E2E_MODEL,
    }),
  });
}

// ─── Prompt + wait helpers ───────────────────────────────────────────────────

/**
 * Submit a prompt and wait for the full response to complete.
 * Combines submitPrompt + waitForInputEnabled + returns messages.
 *
 * Returns the messages after response is complete.
 */
export async function submitPromptAndWait(textarea, text, { timeoutMs = 60_000 } = {}) {
  await submitPrompt(textarea, text);
  await waitForInputEnabled(timeoutMs);
}

// ─── Extended Debug Inspector helpers ────────────────────────────────────────

/**
 * Get toast notifications from the debug inspector.
 */
export async function debugGetToasts() {
  return browser.execute(() => {
    if (!window.__RCODE_DEBUG__) return [];
    return window.__RCODE_DEBUG__.getToasts();
  });
}

/**
 * Get rendered messages from the DOM via debug inspector.
 */
export async function debugGetMessages() {
  return browser.execute(() => {
    if (!window.__RCODE_DEBUG__) return [];
    return window.__RCODE_DEBUG__.getMessages();
  });
}

/**
 * Get streaming state from the DOM via debug inspector.
 */
export async function debugGetStreamingState() {
  return browser.execute(() => {
    if (!window.__RCODE_DEBUG__) return null;
    return window.__RCODE_DEBUG__.getStreamingState();
  });
}

/**
 * Toggle settings panel via debug inspector.
 */
export async function debugToggleSettings(open) {
  return browser.execute((o) => {
    if (!window.__RCODE_DEBUG__) return false;
    window.__RCODE_DEBUG__.triggerToggleSettings(o);
    return true;
  }, open);
}

/**
 * Toggle terminal panel via debug inspector.
 */
export async function debugToggleTerminal(open) {
  return browser.execute((o) => {
    if (!window.__RCODE_DEBUG__) return false;
    window.__RCODE_DEBUG__.triggerToggleTerminal(o);
    return true;
  }, open);
}

/**
 * Switch to sessions or explorer tab via debug inspector.
 */
export async function debugSwitchTab(tab) {
  return browser.execute((t) => {
    if (!window.__RCODE_DEBUG__) return false;
    window.__RCODE_DEBUG__.triggerSwitchTab(t);
    return true;
  }, tab);
}

/**
 * Abort current streaming response via debug inspector.
 */
export async function debugAbort() {
  return browser.execute(() => {
    if (!window.__RCODE_DEBUG__) return false;
    window.__RCODE_DEBUG__.triggerAbort();
    return true;
  });
}
