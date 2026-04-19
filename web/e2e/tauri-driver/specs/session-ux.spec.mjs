import assert from 'node:assert/strict';
import {
  API_BASE,
  E2E_MODEL,
  waitForBackend,
  waitFor,
  fetchJson,
  setupTempGitProject,
  createProject,
  captureState,
  restoreState,
} from '../helpers/e2e-helpers.mjs';

/** Create a session for a specific project via API (uses project_id, not project_path). */
async function createSession(projectId) {
  return fetchJson(`${API_BASE}/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ project_id: projectId, agent_id: 'build', model_id: E2E_MODEL }),
  });
}

async function sleep(ms) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function listProjectSessions(projectId) {
  return fetchJson(`${API_BASE}/projects/${encodeURIComponent(projectId)}/sessions`);
}

async function deleteSession(sessionId) {
  const response = await fetch(`${API_BASE}/session/${sessionId}`, { method: 'DELETE' });
  if (!response.ok && response.status !== 404) {
    throw new Error(`Failed to delete session ${sessionId}: ${response.status}`);
  }
}

async function deleteProject(projectId) {
  const response = await fetch(`${API_BASE}/projects/${encodeURIComponent(projectId)}`, { method: 'DELETE' });
  if (!response.ok && response.status !== 404) {
    throw new Error(`Failed to delete project ${projectId}: ${response.status}`);
  }
}

async function ensureProjectVisible(projectName, timeoutMs = 30000) {
  const initial = projectName.charAt(0).toUpperCase();
  await waitFor(async () => {
    return browser.execute((targetName) => {
      return Array.from(document.querySelectorAll('[data-component="project-rail"] button'))
        .some((button) => {
          const text = button.textContent?.trim();
          const title = button.getAttribute('title');
          return text === targetName || text === targetName?.charAt(0)?.toUpperCase() || title === targetName;
        });
    }, initial);
  }, timeoutMs, 250);
}

async function selectProjectByName(projectName) {
  await ensureProjectVisible(projectName);
  const initial = projectName.charAt(0).toUpperCase();

  await browser.execute((targetName) => {
    const buttons = Array.from(document.querySelectorAll('[data-component="project-rail"] button'));
    const match = buttons.find((button) => {
      const title = button.getAttribute('title');
      const text = button.textContent?.trim();
      return title === targetName || text === targetName;
    });
    match?.click();
  }, initial);

  await waitFor(async () => {
    const headerText = await browser.execute(() => {
      const rail = document.querySelector('[data-component="workbench-left-rail"]');
      return rail?.textContent || '';
    });
    return headerText.includes(projectName);
  }, 15000, 250);
}

async function reloadAndSelectProject(projectName) {
  await browser.reloadSession();
  await waitForBackend();
  await ensureProjectVisible(projectName);
  await selectProjectByName(projectName);
}

async function clickNewSession() {
  const button = await $('[data-component="new-session-button"]');
  await button.waitForExist({ timeout: 30000 });
  await button.click();
}

async function getSessionTitlesFromDom() {
  return browser.execute(() => {
    return Array.from(document.querySelectorAll('[data-component="sessions-list"] [data-session-id]'))
      .map((node) => node.textContent?.trim() || '');
  });
}

async function waitForSessionTitleInDom(title, timeoutMs = 20000) {
  await waitFor(async () => {
    const titles = await getSessionTitlesFromDom();
    return titles.some((text) => text.includes(title));
  }, timeoutMs, 250);
}

async function renameSessionViaUi(oldTitle, newTitle) {
  await waitForSessionTitleInDom(oldTitle);

  const row = await browser.execute((targetTitle) => {
    const rows = Array.from(document.querySelectorAll('[data-component="sessions-list"] [data-session-id]'));
    const match = rows.find((node) => (node.textContent || '').includes(targetTitle));
    if (!match) return null;
    return match.getAttribute('data-session-id');
  }, oldTitle);

  if (!row) {
    throw new Error(`Could not find session row for title: ${oldTitle}`);
  }

  const opened = await browser.execute((sessionId) => {
    const rowEl = document.querySelector(`[data-session-id="${sessionId}"]`);
    if (!rowEl) return false;
    rowEl.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, detail: 2 }));
    return true;
  }, row);

  if (!opened) {
    throw new Error(`Could not trigger rename for session id: ${row}`);
  }

  await waitFor(async () => {
    return browser.execute((sessionId) => {
      return Boolean(document.querySelector(`[data-session-id="${sessionId}"] input[type="text"]`));
    }, row);
  }, 10000, 100);

  await browser.execute((sessionId, nextTitle) => {
    const input = document.querySelector(`[data-session-id="${sessionId}"] input[type="text"]`);
    if (!input) return false;
    input.focus();
    input.value = nextTitle;
    input.dispatchEvent(new InputEvent('input', { bubbles: true, data: nextTitle, inputType: 'insertText' }));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    return true;
  }, row, newTitle);

  await waitForSessionTitleInDom(newTitle, 15000);
}

async function refreshProjectSessionsView(projectName) {
  await reloadAndSelectProject(projectName);
}

async function setSessionFilter(value) {
  const input = await $('input[placeholder="Filter sessions..."]');
  await input.waitForExist({ timeout: 10000 });
  await input.setValue(value);
  await sleep(450);
}

async function getVisibleSessionRows() {
  return browser.execute(() => {
    return Array.from(document.querySelectorAll('[data-component="sessions-list"] [data-session-id]'))
      .map((node) => node.textContent?.trim() || '')
      .filter(Boolean);
  });
}

async function toggleCompactMode() {
  await browser.execute(() => {
    const buttons = Array.from(document.querySelectorAll('[data-component="workbench-left-rail"] button'));
    const compactButton = buttons.find((button) => {
      const title = button.getAttribute('title') || '';
      return title.includes('Compact mode') || title.includes('Normal mode');
    });
    compactButton?.click();
  });
}

async function getSessionRowPadding(title) {
  return browser.execute((targetTitle) => {
    const row = Array.from(document.querySelectorAll('[data-component="sessions-list"] [data-session-id]'))
      .find((node) => (node.textContent || '').includes(targetTitle));
    if (!row) return null;
    const style = window.getComputedStyle(row);
    return { paddingTop: style.paddingTop, paddingBottom: style.paddingBottom };
  }, title);
}

async function groupHeaderExists(label) {
  return browser.execute((targetLabel) => {
    return Array.from(document.querySelectorAll('[data-component="sessions-list"] button'))
      .some((button) => (button.textContent || '').includes(targetLabel));
  }, label);
}

async function toggleGroup(label) {
  await browser.execute((targetLabel) => {
    const button = Array.from(document.querySelectorAll('[data-component="sessions-list"] button'))
      .find((node) => (node.textContent || '').includes(targetLabel));
    button?.click();
  }, label);
}

async function countVisibleSessionRows() {
  return browser.execute(() => document.querySelectorAll('[data-component="sessions-list"] [data-session-id]').length);
}

async function ensureSessionReady() {
  const existingInputs = await $$('[data-component="textarea"]');
  if (existingInputs.length > 0) {
    return existingInputs[0];
  }
  await clickNewSession();
  const input = await $('[data-component="textarea"]');
  await input.waitForExist({ timeout: 30000 });
  return input;
}

async function submitPrompt(prompt) {
  const textarea = await ensureSessionReady();
  await textarea.click();
  await textarea.setValue(prompt);
  const sendButton = await $('[data-component="prompt-submit"]');
  await sendButton.click();
}

async function latestProjectSession(projectId) {
  const sessions = await listProjectSessions(projectId);
  return [...sessions].sort((a, b) => b.updated_at.localeCompare(a.updated_at))[0] ?? null;
}

describe('RCode Tauri session UX', () => {
  let initialState;
  const projectName = `Z Session UX ${Date.now()}`;
  let project;
  let projectPath;
  const createdSessionIds = [];
  const tempDirs = [];

  before(async () => {
    await waitForBackend();
    initialState = await captureState();
    projectPath = setupTempGitProject(projectName);
    tempDirs.push(projectPath);
    project = await createProject(projectPath, projectName);
  });

  beforeEach(async () => {
    await reloadAndSelectProject(projectName);
  });

  after(async () => {
    await restoreState(initialState, { deleteTempDirs: tempDirs });
  });

  it('shows active project name and git branch in the left rail header', async () => {
    const bootstrap = await fetchJson(`${API_BASE}/explorer/bootstrap?project_id=${encodeURIComponent(project.id)}`);
    await waitFor(async () => {
      const headerText = await browser.execute(() => {
        const rail = document.querySelector('[data-component="workbench-left-rail"]');
        return rail?.textContent || '';
      });
      return headerText.includes(projectName) && headerText.includes(bootstrap.git_branch || '');
    }, 15000, 250);
  });

  it('creates a new session under the active project and shows it in the list', async () => {
    const before = await listProjectSessions(project.id);
    await clickNewSession();
    await waitFor(async () => {
      const after = await listProjectSessions(project.id);
      return after.length === before.length + 1;
    }, 15000, 250);

    const latest = await latestProjectSession(project.id);
    assert.ok(latest, 'expected latest session to exist');
    createdSessionIds.push(latest.id);
    await waitFor(async () => (await countVisibleSessionRows()) >= 1, 15000, 250);
  });

  it('supports inline rename via double click and persists the new title', async () => {
    const session = await createSession(project.id, 'Rename Candidate');
    createdSessionIds.push(session.id);

    await reloadAndSelectProject(projectName);
    await waitForSessionTitleInDom('Rename Candidate');
    await renameSessionViaUi('Rename Candidate', 'Renamed Session UX');

    const sessions = await listProjectSessions(project.id);
    const renamed = sessions.find((item) => item.id === session.id);
    assert.equal(renamed?.title, 'Renamed Session UX');
  });

  it('filters sessions by title using the search input', async () => {
    const alpha = await createSession(project.id, 'Alpha Search Target');
    const beta = await createSession(project.id, 'Beta Search Other');
    createdSessionIds.push(alpha.id, beta.id);

    await reloadAndSelectProject(projectName);
    await waitForSessionTitleInDom('Alpha Search Target');
    await waitForSessionTitleInDom('Beta Search Other');

    await setSessionFilter('Alpha Search');
    await waitFor(async () => {
      const rows = await getVisibleSessionRows();
      return rows.some((text) => text.includes('Alpha Search Target')) && !rows.some((text) => text.includes('Beta Search Other'));
    }, 10000, 250);
  });

  it('toggles compact mode and changes session row density', async () => {
    const session = await createSession(project.id, 'Compact Density Session');
    createdSessionIds.push(session.id);

    await reloadAndSelectProject(projectName);
    await waitForSessionTitleInDom('Compact Density Session');

    const before = await getSessionRowPadding('Compact Density Session');
    await toggleCompactMode();
    await waitFor(async () => {
      const after = await getSessionRowPadding('Compact Density Session');
      return after && before && after.paddingTop !== before.paddingTop;
    }, 5000, 150);

    const after = await getSessionRowPadding('Compact Density Session');
    assert.notEqual(after?.paddingTop, before?.paddingTop);
  });

  it('shows date group headers and allows collapsing a group', async () => {
    const session = await createSession(project.id, 'Grouped Session Item');
    createdSessionIds.push(session.id);

    await reloadAndSelectProject(projectName);
    await waitForSessionTitleInDom('Grouped Session Item');
    assert.equal(await groupHeaderExists('Today'), true);

    const beforeCount = await countVisibleSessionRows();
    await toggleGroup('Today');
    await waitFor(async () => (await countVisibleSessionRows()) < beforeCount, 5000, 150);
    const afterCount = await countVisibleSessionRows();
    assert.ok(afterCount < beforeCount, `expected fewer rows after collapsing Today; before=${beforeCount}, after=${afterCount}`);
  });

  it('propagates auto-generated title after first exchange', async () => {
    const before = await listProjectSessions(project.id);
    await clickNewSession();
    await waitFor(async () => {
      const after = await listProjectSessions(project.id);
      return after.length === before.length + 1;
    }, 15000, 250);

    const session = await latestProjectSession(project.id);
    assert.ok(session, 'expected latest session for auto-title test');
    createdSessionIds.push(session.id);
    await submitPrompt('Give this conversation a short descriptive title after answering hi.');

    await waitFor(async () => {
      const latest = await fetchJson(`${API_BASE}/session/${session.id}`);
      return Boolean(latest.title && latest.title !== 'Untitled');
    }, 60000, 1000);

    const updated = await fetchJson(`${API_BASE}/session/${session.id}`);
    assert.ok(updated.title && updated.title !== 'Untitled', `expected auto-title, got ${updated.title}`);

    await refreshProjectSessionsView(projectName);
    await waitForSessionTitleInDom(updated.title, 15000);
  });
});
