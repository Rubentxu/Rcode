import assert from 'node:assert/strict';
import {
  waitForBackend,
  waitFor,
  submitPrompt,
  createSessionWithModel,
  captureState,
  restoreState,
} from '../helpers/e2e-helpers.mjs';

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
  let initialState;
  let textarea;

  before(async () => {
    await waitForBackend();
    initialState = await captureState();
  });

  after(async () => {
    await restoreState(initialState);
  });

  beforeEach(async () => {
    await browser.reloadSession();
    await waitForBackend();

    // Ensure the textarea is visible by using createSessionWithModel
    // which properly navigates to the session in the UI
    ({ textarea } = await createSessionWithModel());
  });

  // ── Scenario A: User message occupies ~2/3 width ─────────────────────────

  it('Scenario A — user message occupies roughly 2/3 of transcript width', async () => {
    // Submit a long prompt via UI to ensure we have a user message
    const longPrompt = 'This is a very long user prompt that should wrap to multiple lines and test the width constraint of the user message bubble layout rendering in the chat interface';
    await submitPrompt(textarea, longPrompt);

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
    await submitPrompt(textarea, 'Say hello in one word');
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
    await submitPrompt(textarea, 'Count to 3');
    await waitForInputEnabled(60_000);

    await submitPrompt(textarea, 'Count to 5');
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
