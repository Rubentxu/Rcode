import assert from 'node:assert/strict';
import {
  API_BASE,
  E2E_MODEL,
  waitFor,
  fetchJson,
  createSessionWithModel,
} from '../helpers/e2e-helpers.mjs';

describe('RCode Tauri desktop smoke', () => {
  it('creates a session, executes a tool, and persists structured messages', async () => {
    // Create session with E2E_MODEL to ensure cost-effective model usage.
    const { sessionId, textarea } = await createSessionWithModel();

    const prompt = 'Use bash tool: pwd. Do not answer from memory.';
    await textarea.setValue(prompt);

    const sendButton = await $('[data-component="prompt-submit"]');
    await sendButton.click();

    await waitFor(async () => {
      const body = await $('body');
      const text = await body.getText();
      return (
        text.includes('/home/rubentxu/Proyectos/rust/rust-code') &&
        /current working directory|working directory is/i.test(text)
      );
    }, 45000);

    const messagePayload = await fetchJson(
      `${API_BASE}/session/${sessionId}/messages?offset=0&limit=20`
    );

    const messages = messagePayload.messages ?? [];
    const allParts = messages.flatMap((m) => m.parts ?? []);

    assert.ok(allParts.some((p) => p.type === 'tool_call' && p.name === 'bash'));
    assert.ok(allParts.some((p) => p.type === 'tool_result'));
    assert.ok(
      allParts.some(
        (p) =>
          p.type === 'text' &&
          /current working directory|working directory is/i.test(p.content ?? '')
      )
    );
  });
});
