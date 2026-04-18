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
  captureState,
  restoreState,
} from '../helpers/e2e-helpers.mjs';

// ─── Temp project helpers ──────────────────────────────────────────────────────

function setupTempGitProject(projectName) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rcode-onboarding-'));
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
  return fetchJson(`${API_BASE}/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ project_id: projectId, agent_id: 'build', model_id: E2E_MODEL }),
  });
}

async function listSessions() {
  return fetchJson(`${API_BASE}/session`);
}

async function deleteSession(sessionId) {
  const response = await fetch(`${API_BASE}/session/${sessionId}`, { method: 'DELETE' });
  if (!response.ok && response.status !== 404) {
    throw new Error(`Failed to delete session ${sessionId}: ${response.status}`);
  }
}

// ─── Project onboarding spec ───────────────────────────────────────────────────

describe('RCode Tauri project onboarding', () => {
  // State captured before the suite runs — restored in after()
  let initialState;

  // Resources created within this suite (cleaned up via restoreState)
  const createdProjects = [];
  const createdSessions = [];
  const tempDirs = [];

  before(async () => {
    await waitForBackend();
    // Snapshot the full state before any mutation
    initialState = await captureState();
  });

  after(async () => {
    // Delete sessions created in this suite first
    for (const sessionId of createdSessions) {
      await deleteSession(sessionId);
    }
    // Restore projects and localStorage to pre-suite state
    await restoreState(initialState, { deleteTempDirs: tempDirs });
  });

  // ── Scenario A: WelcomeScreen shown when no projects ──────────────────────
  //
  // Temporarily removes all projects to trigger the WelcomeScreen.
  // Restores them in the after() via restoreState().

  it('Scenario A — WelcomeScreen shown when no projects exist', async () => {
    // Delete all existing projects (restoreState will put them back)
    const allProjects = await fetchJson(`${API_BASE}/projects`).catch(() => []);
    for (const p of allProjects) {
      await deleteProject(p.id);
    }

    // Reload to get fresh UI state
    await browser.reloadSession();
    await waitForBackend();

    // WelcomeScreen must be visible
    const welcome = await $('[data-component="welcome-screen"]');
    await welcome.waitForExist({ timeout: 30_000 });
    assert.equal(await welcome.isDisplayed(), true, 'WelcomeScreen should be visible when no projects exist');

    // Textarea must NOT be present (can't create session without project)
    const textarea = await $$('[data-component="textarea"]');
    assert.equal(textarea.length, 0, 'Textarea should not exist without an active project');

    // "Open Folder" / "Add Project" button must be present
    const addBtn = await $('[data-component="button"][data-variant="primary"]');
    assert.equal(await addBtn.isExisting(), true, 'Primary add-project button should be present');
    const btnText = await addBtn.getText();
    assert.ok(btnText.includes('Open') || btnText.includes('Folder'), `Button text should contain "Open Folder": got "${btnText}"`);
  });

  // ── Scenario B: RecentProjectsView shown when projects exist ──────────────

  it('Scenario B — RecentProjectsView shown when projects exist but no session active', async () => {
    // Create a test project
    const projectName = `Z Onboarding B ${Date.now()}`;
    const projectPath = setupTempGitProject(projectName);
    tempDirs.push(projectPath);
    const project = await createProject(projectPath, projectName);
    createdProjects.push(project);

    // Reload to pick up the new project
    await browser.reloadSession();
    await waitForBackend();

    // RecentProjectsView must be visible (not WelcomeScreen, not SessionView)
    const recentView = await $('[data-component="recent-projects-view"]');
    await recentView.waitForExist({ timeout: 30_000 });
    assert.equal(await recentView.isDisplayed(), true, 'RecentProjectsView should be visible when projects exist');

    // Project name should appear in the list
    const projectNameVisible = await waitFor(async () => {
      const text = await recentView.getText();
      return text.includes(projectName);
    }, 15_000, 500);
    assert.ok(projectNameVisible, `Project name "${projectName}" should appear in RecentProjectsView`);

    // "Open" button should be present per project row
    const openButtons = await $$('[data-component="recent-projects-view"] button');
    const openTexts = await Promise.all(openButtons.map((btn) => btn.getText()));
    assert.ok(openTexts.some((t) => t.trim() === 'Open'), 'An "Open" button should be present in RecentProjectsView');
  });

  // ── Scenario C: Selecting project navigates to session view ──────────────

  it('Scenario C — selecting a project from RecentProjectsView navigates to session view', async () => {
    // Create a project and session
    const projectName = `Z Onboarding C ${Date.now()}`;
    const projectPath = setupTempGitProject(projectName);
    tempDirs.push(projectPath);
    const project = await createProject(projectPath, projectName);
    createdProjects.push(project);

    const session = await createSession(project.id);
    createdSessions.push(session.id);

    // Reload and wait for RecentProjectsView
    await browser.reloadSession();
    await waitForBackend();

    const recentView = await $('[data-component="recent-projects-view"]');
    await recentView.waitForExist({ timeout: 30_000 });

    // Find and click the "Open" button for our project
    const openButtons = await $$('[data-component="recent-projects-view"] button');
    for (const btn of openButtons) {
      const text = await btn.getText();
      if (text.trim() === 'Open') {
        await btn.click();
        break;
      }
    }

    // Wait for textarea (SessionView rendered)
    const textarea = await $('[data-component="textarea"]');
    await textarea.waitForExist({ timeout: 30_000 });
    assert.equal(await textarea.isDisplayed(), true, 'Textarea should be visible after selecting a project');
  });

  // ── Scenario D: createSession guard — no session without active project ───
  //
  // Temporarily removes all projects to trigger the WelcomeScreen.
  // Restores them in the after() via restoreState().

  it('Scenario D — clicking new-session-button without active project does not create a session', async () => {
    // Delete all projects (restoreState will put them back)
    const allProjects = await fetchJson(`${API_BASE}/projects`).catch(() => []);
    for (const p of allProjects) {
      await deleteProject(p.id);
    }
    // Clear our created-projects list since they're gone now too
    createdProjects.length = 0;

    // Reload
    await browser.reloadSession();
    await waitForBackend();

    // Verify WelcomeScreen is showing
    const welcome = await $('[data-component="welcome-screen"]');
    await welcome.waitForExist({ timeout: 30_000 });
    assert.equal(await welcome.isDisplayed(), true, 'WelcomeScreen should be visible');

    // Record current session count
    const sessionsBefore = await listSessions();
    const countBefore = Array.isArray(sessionsBefore) ? sessionsBefore.length : 0;

    // Try to click new-session-button (it should exist but do nothing)
    const newSessionBtn = await $('[data-component="new-session-button"]');
    if (await newSessionBtn.isExisting()) {
      await newSessionBtn.click();
      await new Promise((r) => setTimeout(r, 500));
    }

    // Verify session count hasn't changed
    const sessionsAfter = await listSessions();
    const countAfter = Array.isArray(sessionsAfter) ? sessionsAfter.length : 0;
    assert.equal(countAfter, countBefore, 'Session count should not increase when no project is active');

    // Verify WelcomeScreen is still showing
    assert.equal(await welcome.isDisplayed(), true, 'WelcomeScreen should still be visible after clicking new-session-button');
  });
});
