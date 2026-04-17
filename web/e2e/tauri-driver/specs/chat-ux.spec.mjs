import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import {
  API_BASE,
  E2E_MODEL,
  waitForBackend,
  waitFor,
  fetchJson,
} from '../helpers/e2e-helpers.mjs';

// ─── Temp project helpers ──────────────────────────────────────────────────────

function setupTempGitProject(projectName) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rcode-chat-ux-'));
  fs.writeFileSync(path.join(root, 'README.md'), `# ${projectName}\n`, 'utf8');
  execFileSync('git', ['init', '-b', 'main'], { cwd: root, stdio: 'ignore' });
  execFileSync('git', ['config', 'user.email', 'e2e@example.com'], { cwd: root, stdio: 'ignore' });
  execFileSync('git', ['config', 'user.name', 'RCode E2E'], { cwd: root, stdio: 'ignore' });
  execFileSync('git', ['add', 'README.md'], { cwd: root, stdio: 'ignore' });
  execFileSync('git', ['commit', '-m', 'init'], { cwd: root, stdio: 'ignore' });
  return root;
}

async function createProject(projectPath, name) {
  return fetchJson(`${API_BASE}/projects`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path: projectPath, name }),
  });
}

async function deleteProject(projectId) {
  const response = await fetch(`${API_BASE}/projects/${encodeURIComponent(projectId)}`, { method: 'DELETE' });
  if (!response.ok && response.status !== 404) {
    throw new Error(`Failed to delete project ${projectId}: ${response.status}`);
  }
}

async function createSession(projectId) {
  const session = await fetchJson(`${API_BASE}/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ project_id: projectId, agent_id: 'build', model_id: E2E_MODEL }),
  });
  return session;
}

async function submitPrompt(sessionId, promptText) {
  const res = await fetch(`${API_BASE}/session/${sessionId}/prompt`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ prompt: promptText }),
  });
  if (!res.ok) throw new Error(`Prompt submission failed: ${res.status}`);
  return res.json();
}

async function deleteSession(sessionId) {
  const response = await fetch(`${API_BASE}/session/${sessionId}`, { method: 'DELETE' });
  if (!response.ok && response.status !== 404) {
    throw new Error(`Failed to delete session ${sessionId}: ${response.status}`);
  }
}

async function waitForMessages(sessionId, count, timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const data = await fetchJson(`${API_BASE}/session/${sessionId}/messages?offset=0&limit=100`);
    if (Array.isArray(data.messages) && data.messages.length >= count) {
      return data.messages;
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`waitForMessages timed out after ${timeoutMs}ms waiting for ${count} messages`);
}

async function waitForInputEnabled(timeoutMs = 60_000) {
  await waitFor(
    () =>
      browser.execute(() => {
        const el = document.querySelector('[data-component="textarea"]');
        if (!el) return false;
        return !el.disabled && el.getAttribute('aria-disabled') !== 'true';
      }),
    timeoutMs,
    500,
  );
}

// ─── Chat UX spec ─────────────────────────────────────────────────────────────

describe('RCode Tauri chat UX', () => {
  let project;
  let projectPath;
  let session;
  const createdSessionIds = [];

  before(async () => {
    await waitForBackend();
    const projectName = `Z Chat UX ${Date.now()}`;
    projectPath = setupTempGitProject(projectName);
    project = await createProject(projectPath, projectName);
    session = await createSession(project.id);
    createdSessionIds.push(session.id);
  });

  after(async () => {
    for (const id of createdSessionIds) {
      await deleteSession(id);
    }
    if (project?.id) {
      await deleteProject(project.id);
    }
    if (projectPath) {
      fs.rmSync(projectPath, { recursive: true, force: true });
    }
  });

  // Navigate to the session before each test
  beforeEach(async () => {
    await browser.reloadSession();
    await waitForBackend();

    // Click the project in the rail to select it
    await waitFor(
      () =>
        browser.execute(() => {
          const buttons = Array.from(document.querySelectorAll('[data-component="project-rail"] button'));
          return buttons.length > 0;
        }),
      30_000,
      500,
    );

    await browser.execute(() => {
      const buttons = Array.from(document.querySelectorAll('[data-component="project-rail"] button'));
      if (buttons.length > 0) buttons[0].click();
    });

    // Wait for textarea
    const textarea = await $('[data-component="textarea"]');
    await textarea.waitForExist({ timeout: 30_000 });
  });

  // ── Scenario A: User message occupies ~2/3 width ─────────────────────────

  it('Scenario A — user message occupies roughly 2/3 of transcript width', async () => {
    // Submit a long prompt via API to ensure we have a user message
    const longPrompt = 'This is a very long user prompt that should wrap to multiple lines and test the width constraint of the user message bubble layout rendering in the chat interface';
    await submitPrompt(session.id, longPrompt);

    // Wait for the message to appear in the DOM
    await waitFor(
      () =>
        browser.execute(() => {
          return document.querySelectorAll('[data-component="user-bubble-message"]').length > 0;
        }),
      60_000,
      500,
    );

    // Wait for input to be re-enabled (response complete)
    await waitForInputEnabled(60_000);

    // Measure user message width vs parent transcript width
    const dims = await browser.execute(() => {
      const userBubble = document.querySelector('[data-component="user-bubble-message"]');
      const transcript = document.querySelector('[data-component="transcript"]');
      if (!userBubble || !transcript) return null;

      const msgRect = userBubble.getBoundingClientRect();
      const parentRect = transcript.getBoundingClientRect();
      return {
        msgWidth: msgRect.width,
        parentWidth: parentRect.width,
      };
    });

    assert.ok(dims, 'Could not measure user message dimensions');
    assert.ok(dims.parentWidth > 0, `Parent width should be positive: ${dims.parentWidth}`);
    assert.ok(dims.msgWidth > 0, `Message width should be positive: ${dims.msgWidth}`);

    const ratio = dims.msgWidth / dims.parentWidth;
    assert.ok(
      ratio >= 0.55 && ratio <= 0.75,
      `User message width ratio should be ~0.66 (55%%-75%%), got ${ratio.toFixed(3)} (msg=${dims.msgWidth}px, parent=${dims.parentWidth}px)`,
    );
  });

  // ── Scenario B: QuickActions (copy + retry) on hover, no branch ─────────

  it('Scenario B — QuickActions (copy + retry) appear on assistant hover, branch absent', async () => {
    // Submit a prompt to get an assistant response
    await submitPrompt(session.id, 'Say hello in one word');
    await waitForInputEnabled(60_000);

    // Wait for assistant message to appear
    await waitFor(
      () =>
        browser.execute(() => {
          return document.querySelectorAll('[data-component="assistant-document-message"]').length > 0;
        }),
      60_000,
      500,
    );

    // Hover over the assistant message to reveal QuickActions
    await browser.execute(() => {
      const assistantMsg = document.querySelector('[data-component="assistant-document-message"]');
      if (assistantMsg) {
        assistantMsg.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true, cancelable: true }));
      }
    });

    // Give hover styles time to apply
    await new Promise((r) => setTimeout(r, 300));

    // Verify copy button is visible
    const copyBtn = await $('[data-action="copy"]');
    await waitFor(
      () => copyBtn.isExisting(),
      10_000,
      200,
    );
    assert.ok(await copyBtn.isExisting(), 'Copy button should exist in QuickActions');
    assert.ok(await copyBtn.isDisplayed(), 'Copy button should be visible on hover');

    // Verify retry button is visible
    const retryBtn = await $('[data-action="retry"]');
    assert.ok(await retryBtn.isExisting(), 'Retry button should exist in QuickActions');
    assert.ok(await retryBtn.isDisplayed(), 'Retry button should be visible on hover');

    // Verify NO branch button
    const branchBtns = await $$('[data-action="branch"]');
    assert.equal(branchBtns.length, 0, 'Branch button should NOT exist in QuickActions');
  });

  // ── Scenario C: Compact chat spacing ────────────────────────────────────

  it('Scenario C — chat has compact turn spacing (gap < 20px between messages)', async () => {
    // Submit two prompts to get at least two user+assistant turns
    await submitPrompt(session.id, 'Count to 3');
    await waitForInputEnabled(60_000);

    await submitPrompt(session.id, 'Count to 5');
    await waitForInputEnabled(60_000);

    // Wait for at least 2 messages
    await waitFor(
      () =>
        browser.execute(() => {
          return document.querySelectorAll('[data-component="user-bubble-message"]').length >= 2;
        }),
      60_000,
      500,
    );

    // Measure vertical gap between consecutive messages
    const gaps = await browser.execute(() => {
      const messages = Array.from(document.querySelectorAll('[data-component="user-bubble-message"], [data-component="assistant-document-message"]'));
      const result = [];
      for (let i = 0; i < messages.length - 1; i++) {
        const curr = messages[i];
        const next = messages[i + 1];
        const currRect = curr.getBoundingClientRect();
        const nextRect = next.getBoundingClientRect();
        const gap = nextRect.top - (currRect.top + currRect.height);
        result.push({ gap, currTop: currRect.top, nextTop: nextRect.top, currH: currRect.height });
      }
      return result;
    });

    assert.ok(gaps.length > 0, 'Should have measured at least one gap between messages');

    // All gaps should be less than 20px for compact layout
    for (const { gap } of gaps) {
      assert.ok(
        gap < 20,
        `Turn gap should be < 20px for compact layout, got ${gap.toFixed(1)}px`,
      );
    }
  });
});
