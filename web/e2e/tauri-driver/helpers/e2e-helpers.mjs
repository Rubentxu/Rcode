/**
 * Shared E2E test helpers for RCode Tauri driver specs.
 *
 * All e2e specs MUST use E2E_MODEL and createSessionWithModel() to guarantee
 * a predictable, cost-effective model is used for every test run.
 */

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

// ─── Session helpers ───────────────────────────────────────────────────────────

/**
 * Create a new session via the backend API using E2E_MODEL, then navigate to it
 * in the UI by clicking its entry in the sessions list.
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
  try {
    const projects = await fetchJson(`${API_BASE}/projects`);
    if (Array.isArray(projects) && projects.length > 0) {
      resolvedProjectPath = projects[0].canonical_path || projects[0].path || projectPath;
    }
  } catch (_) {
    // fallback to provided projectPath
  }

  // 2. Create session with the target model
  const session = await fetchJson(`${API_BASE}/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      project_path: resolvedProjectPath,
      model_id: E2E_MODEL,
    }),
  });

  const sessionId = session.id;

  // 3. Navigate to the session in the UI
  await navigateToSession(sessionId);

  // 4. Wait for the textarea to be ready
  const textarea = await $('[data-component="textarea"]');
  await textarea.waitForExist({ timeout: 30_000 });

  return { sessionId, textarea };
}

/**
 * Click on a session item in the UI sessions list by its ID.
 */
export async function navigateToSession(sessionId) {
  const sessionItem = await $(`[data-session-id="${sessionId}"]`);
  await sessionItem.waitForExist({ timeout: 30_000 });
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
  const mruValue = await browser.execute(() => localStorage.getItem('rcode:active-project'));
  return { projects: Array.isArray(projects) ? projects : [], mruValue };
}

/**
 * Restore the backend + browser state to what captureState() recorded.
 *
 * - Deletes any project whose ID was NOT present in the snapshot.
 * - Does NOT recreate projects that were deleted during the test (backend
 *   cannot recreate arbitrary projects — use temp dirs and the API instead).
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

  // 3. Restore localStorage
  if (snapshot.mruValue === null) {
    await browser.execute(() => localStorage.removeItem('rcode:active-project'));
  } else {
    await browser.execute((v) => localStorage.setItem('rcode:active-project', v), snapshot.mruValue);
  }

  // 4. Clean up temp dirs
  const { rmSync } = await import('node:fs');
  for (const dir of deleteTempDirs) {
    rmSync(dir, { recursive: true, force: true });
  }
}

// ─── UI interaction helpers ────────────────────────────────────────────────────

/**
 * Type text into the prompt textarea and click Send.
 * Clears any existing content first.
 */
export async function submitPrompt(textarea, text) {
  await textarea.click();
  await browser.keys(['Control', 'a']);
  await browser.keys(['Delete']);
  await textarea.setValue(text);
  await new Promise((r) => setTimeout(r, 150));

  const sendBtn = await $('[data-component="prompt-submit"]');
  await sendBtn.waitForExist({ timeout: 10_000 });
  await sendBtn.click();
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
