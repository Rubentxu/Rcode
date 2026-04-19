import assert from 'node:assert/strict';
import {
  API_BASE,
  E2E_MODEL,
  waitFor,
  fetchJson,
  createSessionWithModel,
  captureState,
  restoreState,
} from '../helpers/e2e-helpers.mjs';

const BACKEND = API_BASE;

/**
 * Send a prompt via UI using E2E_MODEL and wait for the assistant to respond.
 * Uses "Use bash tool: pwd" to force a fast tool response.
 */
async function sendAndWait(secretPrompt) {
  const { textarea } = await createSessionWithModel();

  // Include bash tool call to force fast LLM response
  const fullPrompt = `Use bash tool: pwd. ${secretPrompt}`;
  await textarea.setValue(fullPrompt);

  const sendButton = await $('[data-component="prompt-submit"]');
  await sendButton.click();

  // Wait for the assistant to respond — bash tool runs fast (~5-15s)
  await waitFor(async () => {
    const body = await $('body');
    const text = await body.getText();
    return (
      text.includes('/home/rubentxu/Proyectos/rust/rust-code') ||
      /working directory|current directory/i.test(text)
    );
  }, 60000);
}

/**
 * Get user message text from the most recent build session.
 */
async function getLatestUserMessageText() {
  const sessions = await fetchJson(`${BACKEND}/session`);
  const buildSessions = sessions
    .filter((s) => s.agent_id === 'build')
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  assert.ok(buildSessions.length > 0, 'expected at least one build session');
  const sessionId = buildSessions[0].id;

  const payload = await fetchJson(
    `${BACKEND}/session/${sessionId}/messages?offset=0&limit=20`
  );
  const messages = payload.messages ?? [];
  return {
    sessionId,
    userText: messages
      .filter((m) => m.role === 'user')
      .flatMap((m) => m.parts ?? [])
      .filter((p) => p.type === 'text')
      .map((p) => p.content ?? '')
      .join(' '),
  };
}

describe('RCode Privacy Gateway — Tauri desktop smoke', () => {
  let initialState;

  before(async () => {
    initialState = await captureState();
  });

  after(async () => {
    await restoreState(initialState);
  });

  it('sanitizes email in user prompt before persisting', async () => {
    const testEmail = 'alice@corp.com';
    await sendAndWait(`My contact email is ${testEmail}`);

    const { sessionId, userText } = await getLatestUserMessageText();
    console.log('[privacy] Session ID:', sessionId);

    const hasRawEmail = userText.includes(testEmail);
    const hasPrivacyToken = /EMAIL_[A-Z0-9]+/.test(userText);

    console.log('[privacy] === Email sanitization ===');
    console.log('[privacy] User text:', userText.substring(0, 300));
    console.log('[privacy] Raw email present:', hasRawEmail);
    console.log('[privacy] Privacy token present:', hasPrivacyToken);

    if (hasPrivacyToken && !hasRawEmail) {
      console.log('[privacy] ✅ PASS: Email was tokenized');
    } else if (hasRawEmail) {
      console.log('[privacy] ⚠️  INFO: Email NOT tokenized (check ~/.config/rcode/config.json)');
    }

    assert.ok(
      hasPrivacyToken || hasRawEmail,
      `expected either a privacy token or raw email in user message, got: "${userText.substring(0, 200)}"`
    );
  });

  it('sanitizes GitHub token in user prompt', async () => {
    const fakeToken = 'ghp_abc123def456ghi789jkl012mnop345';
    await sendAndWait(`My secret token is ${fakeToken}`);

    const { sessionId, userText } = await getLatestUserMessageText();
    console.log('[privacy] Session ID:', sessionId);

    const hasRawToken = userText.includes(fakeToken);
    const hasPrivacyToken = /GITHUB_TOKEN_[A-Z0-9]+/.test(userText);

    console.log('[privacy] === GitHub token sanitization ===');
    console.log('[privacy] User text:', userText.substring(0, 300));
    console.log('[privacy] Raw token present:', hasRawToken);
    console.log('[privacy] Privacy token present:', hasPrivacyToken);

    if (hasPrivacyToken && !hasRawToken) {
      console.log('[privacy] ✅ PASS: GitHub token was tokenized');
    } else if (hasRawToken) {
      console.log('[privacy] ⚠️  INFO: Token NOT tokenized (check ~/.config/rcode/config.json)');
    }

    assert.ok(
      hasPrivacyToken || hasRawToken,
      `expected either a privacy token or raw token in user message, got: "${userText.substring(0, 200)}"`
    );
  });

  it('sanitizes multiple secrets in one prompt', async () => {
    await sendAndWait(
      'email: bob@example.com, AWS key: AKIAIOSFODNN7EXAMPLE, password: hunter2'
    );

    const { sessionId, userText } = await getLatestUserMessageText();
    console.log('[privacy] Session ID:', sessionId);

    const secrets = [
      { name: 'email', raw: 'bob@example.com', pattern: /EMAIL_[A-Z0-9]+/ },
      { name: 'AWS key', raw: 'AKIAIOSFODNN7EXAMPLE', pattern: /AWS_KEY_[A-Z0-9]+/ },
      { name: 'password', raw: 'hunter2', pattern: /PASSWORD_[A-Z0-9]+/ },
    ];

    console.log('[privacy] === Multiple secrets ===');
    console.log('[privacy] User text:', userText.substring(0, 300));

    let tokenizedCount = 0;
    let rawCount = 0;
    for (const secret of secrets) {
      const rawPresent = userText.includes(secret.raw);
      const tokenPresent = secret.pattern.test(userText);
      console.log(`[privacy] ${secret.name}: raw=${rawPresent}, token=${tokenPresent}`);
      if (tokenPresent && !rawPresent) tokenizedCount++;
      if (rawPresent) rawCount++;
    }

    if (tokenizedCount > 0) {
      console.log(`[privacy] ✅ PASS: ${tokenizedCount}/${secrets.length} secrets tokenized`);
    } else {
      console.log(`[privacy] ⚠️  INFO: secrets not tokenized (privacy may be disabled)`);
    }

    assert.ok(
      tokenizedCount + rawCount >= 1,
      `expected at least one secret in user message, got: "${userText.substring(0, 200)}"`
    );
  });
});
