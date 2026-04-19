import assert from 'node:assert/strict';
import {
  API_BASE,
  waitForBackend,
  waitFor,
  fetchJson,
  setupTempGitProject,
  createProject,
  deleteProject,
  captureState,
  restoreState,
} from '../helpers/e2e-helpers.mjs';

// ─── Constants ────────────────────────────────────────────────────────────────

const MRU_KEY = 'rcode:active-project';

// ─── MRU project persistence spec ──────────────────────────────────────────────

describe('RCode Tauri MRU project persistence', () => {
  // State captured before the suite runs — restored in after()
  let initialState;

  const createdProjects = [];
  const tempDirs = [];

  before(async () => {
    await waitForBackend();
    // Snapshot the full state before any mutation
    initialState = await captureState();
  });

  after(async () => {
    // Restore projects and localStorage to pre-suite state
    await restoreState(initialState, { deleteTempDirs: tempDirs });
  });

  // ── Scenario A: Active project is persisted to localStorage ──────────────

  it('Scenario A — selecting a project persists its ID to localStorage', async () => {
    // Create two projects
    const projectName1 = `Z MRU A1 ${Date.now()}`;
    const projectPath1 = setupTempGitProject(projectName1);
    tempDirs.push(projectPath1);
    const project1 = await createProject(projectPath1, projectName1);
    createdProjects.push(project1);

    const projectName2 = `Z MRU A2 ${Date.now()}`;
    const projectPath2 = setupTempGitProject(projectName2);
    tempDirs.push(projectPath2);
    const project2 = await createProject(projectPath2, projectName2);
    createdProjects.push(project2);

    // Reload and wait for the project rail to show both buttons
    await browser.reloadSession();
    await waitForBackend();

    await waitFor(
      () =>
        browser.execute(() => {
          return document.querySelectorAll('[data-component="project-rail"] button').length >= 2;
        }),
      30_000,
      500,
    );

    // Select the first project by clicking its avatar in the project rail
    await browser.execute((projectName) => {
      const buttons = Array.from(document.querySelectorAll('[data-component="project-rail"] button'));
      const match = buttons.find((btn) => btn.getAttribute('title') === projectName);
      if (match) match.click();
    }, projectName1);

    // Wait a moment for localStorage write
    await new Promise((r) => setTimeout(r, 500));

    // Read localStorage for the MRU key
    const mruValue = await browser.execute(() => localStorage.getItem('rcode:active-project'));

    assert.ok(mruValue !== null, `localStorage['${MRU_KEY}'] should not be null after selecting a project`);
    assert.equal(
      mruValue,
      project1.id,
      `localStorage should contain the first project's ID, got "${mruValue}" instead of "${project1.id}"`,
    );
  });

  // ── Scenario B: After reload, MRU project is auto-selected ──────────────

  it('Scenario B — after reload, MRU project is auto-selected in the UI', async () => {
    // Create one project
    const projectName = `Z MRU B ${Date.now()}`;
    const projectPath = setupTempGitProject(projectName);
    tempDirs.push(projectPath);
    const project = await createProject(projectPath, projectName);
    createdProjects.push(project);

    // Reload and wait for the project rail
    await browser.reloadSession();
    await waitForBackend();

    await waitFor(
      () =>
        browser.execute(() => {
          return document.querySelectorAll('[data-component="project-rail"] button').length >= 1;
        }),
      30_000,
      500,
    );

    // Select the project
    await browser.execute((projName) => {
      const buttons = Array.from(document.querySelectorAll('[data-component="project-rail"] button'));
      const match = buttons.find((btn) => btn.getAttribute('title') === projName);
      if (match) match.click();
    }, projectName);

    await new Promise((r) => setTimeout(r, 500));

    // Verify it's written to localStorage before reload
    const mruBefore = await browser.execute(() => localStorage.getItem('rcode:active-project'));
    assert.equal(mruBefore, project.id, 'Project should be written to localStorage before reload');

    // Reload and verify auto-selection
    await browser.reloadSession();
    await waitForBackend();

    await waitFor(
      () =>
        browser.execute(() => {
          const rail = document.querySelector('[data-component="workbench-left-rail"]');
          const text = rail?.textContent || '';
          return text.includes(arguments[0]);
        }, projectName),
      30_000,
      500,
    );

    // Verify the text now that we know it's present
    const headerText = await browser.execute(() => {
      const rail = document.querySelector('[data-component="workbench-left-rail"]');
      return rail?.textContent || '';
    });
    assert.ok(
      headerText.includes(projectName),
      `Left rail header should contain the project name "${projectName}" after auto-selection, got: "${headerText}"`,
    );
  });

  // ── Scenario C: Single project is auto-selected on load ─────────────────
  //
  // Temporarily removes all pre-existing projects and creates one.
  // restoreState() in after() will delete the test project and put
  // back any pre-existing ones.

  it('Scenario C — with a single project, it is auto-selected on load without user interaction', async () => {
    // Delete all current projects (restoreState will put them back)
    const allProjects = await fetchJson(`${API_BASE}/projects`).catch(() => []);
    for (const p of allProjects) {
      await deleteProject(p.id);
    }
    // Clear our tracking list — those are gone now too
    createdProjects.length = 0;

    // Create exactly one project
    const projectName = `Z MRU C ${Date.now()}`;
    const projectPath = setupTempGitProject(projectName);
    tempDirs.push(projectPath);
    const project = await createProject(projectPath, projectName);
    createdProjects.push(project);

    // Clear localStorage to ensure a fresh state
    await browser.execute(() => localStorage.removeItem('rcode:active-project'));

    // Reload — single project should be auto-selected
    await browser.reloadSession();
    await waitForBackend();

    // The project name should appear in the left rail header without any clicks
    await waitFor(
      () =>
        browser.execute(() => {
          const rail = document.querySelector('[data-component="workbench-left-rail"]');
          const text = rail?.textContent || '';
          return text.includes(arguments[0]);
        }, projectName),
      30_000,
      500,
    );

    // Verify the text now that we know it's present
    const headerText = await browser.execute(() => {
      const rail = document.querySelector('[data-component="workbench-left-rail"]');
      return rail?.textContent || '';
    });
    assert.ok(
      headerText.includes(projectName),
      `With single project, it should auto-select on load. Left rail header should contain "${projectName}", got: "${headerText}"`,
    );

    // localStorage should also have the MRU set
    const mruValue = await browser.execute(() => localStorage.getItem('rcode:active-project'));
    assert.equal(mruValue, project.id, `localStorage should contain the single project's ID: "${project.id}"`);
  });
});
