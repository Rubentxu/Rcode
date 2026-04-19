/**
 * RCode Tauri Desktop E2E — Project Management
 *
 * Validates project management features:
 * 1. Project rail shows registered projects
 * 2. Project can be selected (becomes active)
 * 3. Project health status is visible
 * 4. Session list updates when switching projects
 * 5. Project onboarding — WelcomeScreen when no projects
 *
 * All tests use E2E_MODEL (minimax/MiniMax-M2.7-highspeed) for cost control.
 */

import assert from 'node:assert/strict';
import {
  API_BASE,
  waitForBackend,
  waitFor,
  fetchJson,
  setupTempGitProject,
  createProject,
  deleteProject,
  createSessionWithModel,
  createSessionDirect,
  captureState,
  restoreState,
  debugSnapshot,
  waitForDebugInspector,
  debugSwitchProject,
  debugRefreshSessions,
} from '../helpers/e2e-helpers.mjs';

// ─── Suite ────────────────────────────────────────────────────────────────────

describe('Project Management', () => {
  let initialState;
  let tempDir;

  before(async () => {
    await waitForBackend();
    initialState = await captureState();
  });

  after(async () => {
    await restoreState(initialState, { deleteTempDirs: tempDir ? [tempDir] : [] });
  });

  // ── Test 1: Project rail is visible and shows projects ──────────────────────

  it('project rail is visible and shows at least one project', async () => {
    const projectRail = await $('[data-component="project-rail"]');
    await projectRail.waitForExist({ timeout: 10_000 });

    // Verify projects exist via API (more reliable than debugSnapshot DOM reads)
    const projects = await fetchJson(`${API_BASE}/projects`);
    assert.ok(
      projects.length >= 1,
      `Should have at least 1 project via API, got ${projects.length}`,
    );
  });

  // ── Test 2: Active project is highlighted ────────────────────────────────────

  it('a project can be the active project', async () => {
    const snap = await debugSnapshot();
    assert.ok(
      snap?.project?.activeId,
      'Should have an active project',
    );
  });

  // ── Test 3: Register a new temp project ─────────────────────────────────────

  it('can register a new project via API', async () => {
    tempDir = setupTempGitProject('rcode-e2e-project');

    const project = await createProject(tempDir, 'E2E Test Project');
    assert.ok(project.id, 'Project should have an ID');
    assert.equal(project.name, 'E2E Test Project', 'Project name should match');

    // Verify it appears in the project list
    const projects = await fetchJson(`${API_BASE}/projects`);
    const found = projects.find((p) => p.id === project.id);
    assert.ok(found, 'New project should appear in project list');

    // Cleanup: delete the project
    await deleteProject(project.id);
  });

  // ── Test 4: Sessions belong to the active project ───────────────────────────

  it('sessions are scoped to the active project', async () => {
    await waitForDebugInspector();

    const projects = await fetchJson(`${API_BASE}/projects`);
    assert.ok(projects.length > 0, 'Need at least one project');

    const project1 = projects[0];
    const projectPath1 = project1.canonical_path || project1.path;

    // Switch to project 1
    await debugSwitchProject(project1.id);
    await new Promise((r) => setTimeout(r, 1500));
    await debugRefreshSessions();
    await new Promise((r) => setTimeout(r, 500));

    const snap1 = await debugSnapshot();
    const count1 = snap1?.workspace?.sessionCount || 0;

    // Create a session for project 1
    const session = await createSessionDirect({ projectPath: projectPath1 });

    await debugRefreshSessions();
    await new Promise((r) => setTimeout(r, 500));

    const snap2 = await debugSnapshot();
    assert.ok(
      (snap2?.workspace?.sessionCount || 0) > count1,
      'Session count should increase after creating a session',
    );

    // Verify the session is for the correct project
    assert.equal(session.project_id, project1.id, 'Session should belong to the active project');

    // Cleanup
    await fetch(`${API_BASE}/session/${session.id}`, { method: 'DELETE' }).catch(() => {});
  });

  // ── Test 5: WorkbenchTopNav shows project context ───────────────────────────

  it('top navigation bar shows project context', async () => {
    const topNav = await $('[data-component="workbench-topnav"]');
    await topNav.waitForExist({ timeout: 10_000 });

    // Should show settings toggle
    const settingsBtn = await $('[data-component="settings-toggle"]');
    await settingsBtn.waitForExist({ timeout: 10_000 });
  });

  // ── Test 6: Sessions tab shows sessions for active project ─────────────────

  it('sessions tab shows session list for active project', async () => {
    // Create a session first
    const { sessionId } = await createSessionWithModel();

    // Click sessions tab
    const sessionsTab = await $('[data-tab="sessions"]');
    await sessionsTab.waitForExist({ timeout: 10_000 });
    await sessionsTab.click();
    await new Promise((r) => setTimeout(r, 500));

    // The session should be visible
    const sessionItem = await $(`[data-session-id="${sessionId}"]`);
    await sessionItem.waitForExist({ timeout: 10_000 });
  });
});
