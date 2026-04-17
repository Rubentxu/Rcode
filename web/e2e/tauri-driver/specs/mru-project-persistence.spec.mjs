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

// ─── Constants ────────────────────────────────────────────────────────────────

const MRU_KEY = 'rcode:active-project';

// ─── Temp project helpers ──────────────────────────────────────────────────────

function setupTempGitProject(projectName) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rcode-mru-'));
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

async function listProjects() {
  return fetchJson(`${API_BASE}/projects`);
}

async function deleteAllProjects() {
  const projects = await listProjects();
  for (const p of projects) {
    await deleteProject(p.id);
  }
}

// ─── MRU project persistence spec ──────────────────────────────────────────────

describe('RCode Tauri MRU project persistence', () => {
  const createdProjects = [];
  const tempDirs = [];

  before(async () => {
    await waitForBackend();
  });

  after(async () => {
    for (const project of createdProjects) {
      await deleteProject(project.id);
    }
    for (const dir of tempDirs) {
      fs.rmSync(dir, { recursive: true, force: true });
    }
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

    // Navigate to the app and wait for project rail
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
      const match = buttons.find((btn) => {
        const title = btn.getAttribute('title');
        return title === projectName;
      });
      if (match) match.click();
    }, projectName1);

    // Wait a moment for localStorage write
    await new Promise((r) => setTimeout(r, 500));

    // Read localStorage for the MRU key
    const mruValue = await browser.execute(() => {
      return localStorage.getItem('rcode:active-project');
    });

    assert.ok(mruValue !== null, `localStorage['${MRU_KEY}'] should not be null after selecting a project`);
    assert.equal(mruValue, project1.id, `localStorage should contain the first project's ID, got "${mruValue}" instead of "${project1.id}"`);
  });

  // ── Scenario B: After reload, MRU project is auto-selected ──────────────

  it('Scenario B — after reload, MRU project is auto-selected in the UI', async () => {
    // Create one project
    const projectName = `Z MRU B ${Date.now()}`;
    const projectPath = setupTempGitProject(projectName);
    tempDirs.push(projectPath);
    const project = await createProject(projectPath, projectName);
    createdProjects.push(project);

    // Select the project in the UI
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

    await browser.execute((projName) => {
      const buttons = Array.from(document.querySelectorAll('[data-component="project-rail"] button'));
      const match = buttons.find((btn) => {
        const title = btn.getAttribute('title');
        return title === projName;
      });
      if (match) match.click();
    }, projectName);

    await new Promise((r) => setTimeout(r, 500));

    // Verify it's written to localStorage
    const mruBefore = await browser.execute(() => localStorage.getItem('rcode:active-project'));
    assert.equal(mruBefore, project.id, 'Project should be written to localStorage before reload');

    // Reload the session
    await browser.reloadSession();
    await waitForBackend();

    // The project should be auto-selected (active project name visible in left rail header)
    const headerText = await waitFor(
      () =>
        browser.execute(() => {
          const rail = document.querySelector('[data-component="workbench-left-rail"]');
          return rail?.textContent || '';
        }),
      30_000,
      500,
    );

    assert.ok(
      headerText.includes(projectName),
      `Left rail header should contain the project name "${projectName}" after auto-selection, got: "${headerText}"`,
    );
  });

  // ── Scenario C: Single project is auto-selected on load ─────────────────

  it('Scenario C — with a single project, it is auto-selected on load without user interaction', async () => {
    // Ensure only ONE project exists — delete all others first
    await deleteAllProjects();
    createdProjects.length = 0;

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
    const headerText = await waitFor(
      () =>
        browser.execute(() => {
          const rail = document.querySelector('[data-component="workbench-left-rail"]');
          return rail?.textContent || '';
        }),
      30_000,
      500,
    );

    assert.ok(
      headerText.includes(projectName),
      `With single project, it should auto-select on load. Left rail header should contain "${projectName}", got: "${headerText}"`,
    );

    // localStorage should also have the MRU set
    const mruValue = await browser.execute(() => localStorage.getItem('rcode:active-project'));
    assert.equal(mruValue, project.id, `localStorage should contain the single project's ID: "${project.id}"`);
  });
});
