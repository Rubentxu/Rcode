/**
 * RCode Tauri Desktop E2E — Session Lifecycle
 *
 * Validates the complete session lifecycle:
 * 1. Create session via API → appears in UI sessions list
 * 2. Click session → SessionView renders with textarea
 * 3. Session appears with correct metadata (model, status)
 * 4. Multiple sessions can coexist in the sessions list
 *
 * All tests use E2E_MODEL (minimax/MiniMax-M2.7-highspeed) for cost control.
 */

import assert from 'node:assert/strict';
import {
  API_BASE,
  E2E_MODEL,
  waitForBackend,
  waitFor,
  fetchJson,
  createSessionWithModel,
  createSessionDirect,
  captureState,
  restoreState,
  debugSnapshot,
  waitForDebugInspector,
  debugRefreshSessions,
  debugSwitchProject,
} from '../helpers/e2e-helpers.mjs';

// ─── Suite ────────────────────────────────────────────────────────────────────

describe('Session Lifecycle', () => {
  let initialState;

  before(async () => {
    await waitForBackend();
    initialState = await captureState();
  });

  after(async () => {
    await restoreState(initialState);
  });

  // ── Test 1: Session appears in UI after creation ────────────────────────────

  it('creates a session and it appears in the sessions list', async () => {
    await waitForDebugInspector();

    // Get first project
    const projects = await fetchJson(`${API_BASE}/projects`);
    assert.ok(projects.length > 0, 'At least one project must exist');

    const projectId = projects[0].id;
    const projectPath = projects[0].canonical_path || projects[0].path;

    // Activate project
    await debugSwitchProject(projectId);
    await new Promise((r) => setTimeout(r, 1500));

    // Count sessions before
    const snapBefore = await debugSnapshot();
    const countBefore = snapBefore?.workspace?.sessionCount || 0;

    // Create session via API
    const session = await createSessionDirect({ projectPath });

    // Refresh sessions in UI
    await debugRefreshSessions();
    await new Promise((r) => setTimeout(r, 500));

    // Verify session appears in UI
    const sessionItem = await $(`[data-session-id="${session.id}"]`);
    await sessionItem.waitForExist({ timeout: 15_000 });

    // Verify count increased
    const snapAfter = await debugSnapshot();
    assert.ok(
      (snapAfter?.workspace?.sessionCount || 0) > countBefore,
      `Session count should have increased (was ${countBefore}, now ${snapAfter?.workspace?.sessionCount})`,
    );

    // Cleanup
    await fetch(`${API_BASE}/session/${session.id}`, { method: 'DELETE' }).catch(() => {});
  });

  // ── Test 2: Navigate to session renders SessionView ─────────────────────────

  it('navigating to a session shows the textarea (SessionView)', async () => {
    const { sessionId, textarea } = await createSessionWithModel();

    // Verify textarea is present and interactable
    assert.ok(textarea, 'Textarea should exist');
    await textarea.waitForDisplayed({ timeout: 10_000 });

    // Verify via API that the session exists
    const session = await fetchJson(`${API_BASE}/session/${sessionId}`);
    assert.equal(session.id, sessionId, 'Session should exist via API');
  });

  // ── Test 3: Session metadata is correct ─────────────────────────────────────

  it('session is created with E2E_MODEL and correct project', async () => {
    const session = await createSessionDirect();

    assert.ok(session.id, 'Session should have an ID');
    assert.equal(session.model_id, E2E_MODEL, `Session model should be ${E2E_MODEL}`);
    assert.ok(session.project_id, 'Session should have a project_id');
    assert.ok(session.status, 'Session should have a status');

    // Cleanup
    await fetch(`${API_BASE}/session/${session.id}`, { method: 'DELETE' }).catch(() => {});
  });

  // ── Test 4: Multiple sessions coexist ────────────────────────────────────────

  it('multiple sessions appear in the sessions list', async () => {
    await waitForDebugInspector();

    const projects = await fetchJson(`${API_BASE}/projects`);
    const projectPath = projects[0].canonical_path || projects[0].path;
    const projectId = projects[0].id;

    await debugSwitchProject(projectId);
    await new Promise((r) => setTimeout(r, 1500));

    // Create two sessions
    const s1 = await createSessionDirect({ projectPath });
    const s2 = await createSessionDirect({ projectPath });

    // Refresh
    await debugRefreshSessions();
    await new Promise((r) => setTimeout(r, 500));

    // Both should appear in DOM
    const el1 = await $(`[data-session-id="${s1.id}"]`);
    const el2 = await $(`[data-session-id="${s2.id}"]`);
    await el1.waitForExist({ timeout: 15_000 });
    await el2.waitForExist({ timeout: 15_000 });

    // Cleanup
    await fetch(`${API_BASE}/session/${s1.id}`, { method: 'DELETE' }).catch(() => {});
    await fetch(`${API_BASE}/session/${s2.id}`, { method: 'DELETE' }).catch(() => {});
  });

  // ── Test 5: New session button creates a session ────────────────────────────

  it('clicking new-session-button creates a new session', async () => {
    await waitForDebugInspector();

    const projects = await fetchJson(`${API_BASE}/projects`);
    const projectId = projects[0].id;

    await debugSwitchProject(projectId);
    await new Promise((r) => setTimeout(r, 1500));
    await debugRefreshSessions();
    await new Promise((r) => setTimeout(r, 500));

    // Click sessions tab
    const sessionsTab = await $('[data-tab="sessions"]');
    await sessionsTab.waitForExist({ timeout: 10_000 });
    await sessionsTab.click();
    await new Promise((r) => setTimeout(r, 500));

    // Click new session button
    const newBtn = await $('[data-component="new-session-button"]');
    await newBtn.waitForExist({ timeout: 10_000 });
    await newBtn.waitForClickable({ timeout: 10_000 });
    await newBtn.click();

    // Wait for textarea to appear (indicates session was created)
    const textarea = await $('[data-component="textarea"]');
    await textarea.waitForExist({ timeout: 30_000 });
    assert.ok(await textarea.isDisplayed(), 'Textarea should be visible after creating session');
  });

  // ── Test 6: Sessions list has search functionality ───────────────────────────

  it('sessions list has a search input', async () => {
    // The search input appears when sessions tab is active
    const sessionsTab = await $('[data-tab="sessions"]');
    await sessionsTab.waitForExist({ timeout: 10_000 });
    await sessionsTab.click();
    await new Promise((r) => setTimeout(r, 500));

    const searchInput = await $('[aria-label="Filter sessions"]');
    await searchInput.waitForExist({ timeout: 10_000 });
    assert.ok(await searchInput.isDisplayed(), 'Search input should be visible');
  });
});
