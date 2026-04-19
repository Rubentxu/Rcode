/**
 * llm-diagnostic.spec.mjs
 *
 * Minimal smoke test to diagnose why the LLM is not responding.
 * Sends a simple prompt and waits for any assistant text in the UI.
 * Dumps session messages and API state for inspection.
 */
import assert from 'node:assert/strict';
import {
  API_BASE,
  E2E_MODEL,
  waitForBackend,
  waitFor,
  fetchJson,
  createSessionWithModel,
} from '../helpers/e2e-helpers.mjs';

const POLL_INTERVAL = 500;

describe('LLM diagnostic smoke', () => {
  it('sends a prompt and verifies the LLM produces a response', async () => {
    // 1. Wait for UI ready + create session with E2E_MODEL
    await waitForBackend();
    const { sessionId, textarea } = await createSessionWithModel();
    console.log('[diag] Session created with model:', E2E_MODEL, '| id:', sessionId);

    // 2. Send a trivial prompt (no tools needed)
    const prompt = 'Reply with exactly one word: hello';
    await textarea.setValue(prompt);
    const sendBtn = await $('[data-component="prompt-submit"]');
    await sendBtn.click();
    console.log('[diag] Prompt sent:', prompt);

    // 5. Poll session status until Idle or Aborted (not Running)
    // Use a short timeout so we can dump state even on failure
    let finalStatus = null;
    const WAIT_MS = 30000;
    const pollStart = Date.now();
    while (Date.now() - pollStart < WAIT_MS) {
      try {
        const sessions = await fetchJson(`${API_BASE}/session`);
        const s = sessions.find((s) => s.id === sessionId);
        if (s) {
          console.log('[diag] Session status:', s.status, '| model_id:', s.model_id);
          finalStatus = s.status;
          if (s.status !== 'Running') break;
        }
      } catch (e) {
        console.log('[diag] poll error:', e.message);
      }
      await new Promise((r) => setTimeout(r, POLL_INTERVAL));
    }
    console.log('[diag] Final session status after poll:', finalStatus);

    // 6. Dump all messages regardless of status
    let messages = [];
    try {
      const payload = await fetchJson(`${API_BASE}/session/${sessionId}/messages?offset=0&limit=50`);
      messages = payload.messages ?? [];
    } catch (e) {
      console.log('[diag] failed to fetch messages:', e.message);
    }
    console.log('[diag] Message count:', messages.length);
    for (const msg of messages) {
      console.log(`[diag]   role=${msg.role} parts=${JSON.stringify(msg.parts?.map(p => ({ type: p.type, len: (p.content ?? p.text ?? '').length })))}`);
    }

    // 6b. Also dump full text of any parts for debugging
    for (const msg of messages) {
      for (const part of (msg.parts ?? [])) {
        if (part.type === 'text' && part.content) {
          console.log(`[diag]   TEXT[${msg.role}]: ${part.content.slice(0, 200)}`);
        }
      }
    }

    const allParts = messages.flatMap((m) => m.parts ?? []);
    const assistantText = allParts.find((p) => p.type === 'text' && (p.content ?? '').length > 0);

    // 7. Also check body text in UI
    let bodyText = '';
    try {
      bodyText = await (await $('body')).getText();
    } catch (_) {}
    const hasHelloInUI = /hello/i.test(bodyText);
    console.log('[diag] "hello" in UI:', hasHelloInUI);
    console.log('[diag] assistant text part:', assistantText?.content?.slice(0, 100));

    assert.ok(
      assistantText,
      `Expected at least one assistant text part. Session status: ${finalStatus}. Parts found: ${JSON.stringify(allParts.map(p => p.type))}`
    );
  });
});
