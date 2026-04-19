/**
 * RCode Tauri Desktop E2E — Chat Scroll UX Spec
 *
 * Validates the auto-scroll behavior introduced in the chat-scroll-ux change:
 *   1. Input is unlocked (not disabled) after the LLM finishes responding.
 *      Regression coverage for: onComplete was a no-op → setLoading never called.
 *   2. Chat auto-scrolls to the bottom during and after streaming.
 *   3. When the user scrolls up manually, auto-scroll pauses and the
 *      "jump to bottom" button appears.
 *   4. Clicking the jump-to-bottom button resumes auto-scroll and hides the button.
 *
 * Run:
 *   cd web/e2e/tauri-driver && npm test -- --spec specs/chat-scroll-ux.spec.mjs
 */

import assert from 'node:assert/strict';
import {
  waitForBackend,
  waitFor,
  submitPrompt,
  createSessionWithModel,
  E2E_MODEL,
} from '../helpers/e2e-helpers.mjs';

// ─── Constants ────────────────────────────────────────────────────────────────

// Short prompt that forces a real LLM round-trip using bash tool.
const SHORT_PROMPT = 'Use bash tool: echo "scroll ux test". Do not answer from memory.';

// Long prompt that produces multi-line output so the container has overflow.
const LONG_PROMPT =
  'Use bash tool: for i in $(seq 1 60); do echo "scroll line $i"; done. Do not answer from memory.';

/** Returns true if the textarea is not disabled */
async function textareaIsEnabled() {
  return browser.execute(() => {
    const el = document.querySelector('[data-component="textarea"]');
    if (!el) return false;
    return !el.disabled && el.getAttribute('aria-disabled') !== 'true';
  });
}

/** Returns true if there is no active spinner / processing indicator */
async function loadingIndicatorGone() {
  return browser.execute(() => {
    const html = document.body.innerHTML;
    return (
      !/processing\.\.\./i.test(html) &&
      !/animate-spin/i.test(html) &&
      !document.querySelector('[data-streaming="optimistic"]')
    );
  });
}

/** Returns true if the scroll container is at (or very near) the bottom */
async function isAtBottom(threshold = 80) {
  return browser.execute((threshold) => {
    const el = document.querySelector('[data-component="chat-scroll-container"]');
    if (!el) return false;
    return el.scrollHeight - el.scrollTop - el.clientHeight <= threshold;
  }, threshold);
}

/** Returns true if the chat container has overflow (content taller than viewport) */
async function hasScrollOverflow() {
  return browser.execute(() => {
    const el = document.querySelector('[data-component="chat-scroll-container"]');
    if (!el) return false;
    return el.scrollHeight > el.clientHeight + 100;
  });
}

/** Returns true if the jump-to-bottom button is present and visible in the DOM */
async function jumpToBottomButtonVisible() {
  return browser.execute(() => {
    const btn = document.querySelector('[data-component="jump-to-bottom-button"]');
    if (!btn) return false;
    const style = window.getComputedStyle(btn);
    return style.display !== 'none' && style.visibility !== 'hidden' && style.opacity !== '0';
  });
}

// ─── Suite ────────────────────────────────────────────────────────────────────

describe('Chat Scroll UX', () => {

  let textarea;

  before(async () => {
    await waitForBackend();
    // Create session with E2E_MODEL to ensure consistent, cost-effective model usage.
    ({ textarea } = await createSessionWithModel());
  });

  // ── Test 1 ──────────────────────────────────────────────────────────────────
  it('input textarea is enabled (not disabled) after the LLM finishes responding', async () => {
    // textarea is set up in before() via createSessionWithModel()
    await submitPrompt(textarea, SHORT_PROMPT);

    // Wait for the loading indicator to disappear and the textarea to re-enable.
    // This is the primary regression check for the onComplete fix.
    // We wait up to 90s to account for slow LLM responses.
    await waitFor(async () => {
      const enabled = await textareaIsEnabled();
      const noSpinner = await loadingIndicatorGone();
      return enabled && noSpinner;
    }, 90_000, 500);

    const isDisabled = await browser.execute(() => {
      const el = document.querySelector('[data-component="textarea"]');
      if (!el) return true;
      return el.disabled || el.getAttribute('aria-disabled') === 'true';
    });

    assert.equal(isDisabled, false, 'Textarea must not be disabled after LLM response completes');
  });

  // ── Test 2 ──────────────────────────────────────────────────────────────────
  it('chat container is scrolled to the bottom after the response completes', async () => {
    // After test 1 the response is done and the textarea is re-enabled.
    // The scroll container must be at the bottom.
    const atBottom = await isAtBottom();
    assert.equal(
      atBottom,
      true,
      'Chat scroll container must be at the bottom after the response is complete'
    );
  });

  // ── Test 3 ──────────────────────────────────────────────────────────────────
  it('jump-to-bottom button is NOT visible when already at the bottom', async () => {
    const visible = await jumpToBottomButtonVisible();
    assert.equal(visible, false, 'Jump-to-bottom button must be hidden when scroll is at bottom');
  });

  // ── Test 4 ──────────────────────────────────────────────────────────────────
  it('jump-to-bottom button appears when user scrolls up — requires overflow', async () => {
    // First, generate a long response so the container has enough content to scroll.
    // We may already have overflow from test 1; if not, submit the long prompt.
    const hasOverflow = await hasScrollOverflow();
    if (!hasOverflow) {
      const textarea = await $('[data-component="textarea"]');
      await textarea.waitForExist({ timeout: 10_000 });
      await submitPrompt(textarea, LONG_PROMPT);

      // Wait for the response to complete
      await waitFor(async () => {
        const enabled = await textareaIsEnabled();
        const noSpinner = await loadingIndicatorGone();
        return enabled && noSpinner;
      }, 120_000, 500);
    }

    // Verify we now have overflow
    const overflow = await hasScrollOverflow();
    if (!overflow) {
      // If there's still no overflow, skip gracefully with a note
      console.warn('[chat-scroll-ux] Skipping scroll test: container has no overflow content');
      return;
    }

    // Programmatically scroll to top to simulate user scrolling up
    await browser.execute(() => {
      const el = document.querySelector('[data-component="chat-scroll-container"]');
      if (el) {
        el.scrollTop = 0;
        el.dispatchEvent(new Event('scroll', { bubbles: true }));
      }
    });

    // Wait for the reactive state to update (SolidJS effects are synchronous but
    // we give it a couple frames to render the button)
    await new Promise((r) => setTimeout(r, 600));

    const visible = await jumpToBottomButtonVisible();
    assert.equal(
      visible,
      true,
      'Jump-to-bottom button must appear when user scrolls away from bottom'
    );
  });

  // ── Test 5 ──────────────────────────────────────────────────────────────────
  it('clicking jump-to-bottom button scrolls to bottom and hides the button', async () => {
    // This test depends on test 4 having left the button visible.
    // If the button isn't visible (e.g. no overflow), skip.
    const visible = await jumpToBottomButtonVisible();
    if (!visible) {
      console.warn('[chat-scroll-ux] Skipping: jump-to-bottom button not visible (no overflow)');
      return;
    }

    const btn = await $('[data-component="jump-to-bottom-button"]');
    await btn.waitForExist({ timeout: 5_000 });
    await btn.click();

    await new Promise((r) => setTimeout(r, 800));

    const atBottom = await isAtBottom();
    assert.equal(atBottom, true, 'Scroll container must be at bottom after clicking jump-to-bottom');

    const stillVisible = await jumpToBottomButtonVisible();
    assert.equal(stillVisible, false, 'Jump-to-bottom button must hide after scrolling to bottom');
  });

  // ── Test 6 ──────────────────────────────────────────────────────────────────
  it('auto-scroll follows streaming output when user is at the bottom', async () => {
    // Start a fresh session for a clean state, reusing E2E_MODEL
    ({ textarea } = await createSessionWithModel());
    await submitPrompt(textarea, LONG_PROMPT);

    // Wait for streaming to start (optimistic bubble or draft visible)
    await waitFor(async () => {
      return browser.execute(() => {
        return (
          !!document.querySelector('[data-component="streaming-draft"]') ||
          !!document.querySelector('[data-streaming="optimistic"]') ||
          /animate-spin/i.test(document.body.innerHTML)
        );
      });
    }, 15_000);

    // During streaming, the container should stay near the bottom.
    // Sample 3 times at 1s intervals.
    let bottomCount = 0;
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 1_000));
      if (await isAtBottom(150)) bottomCount++;
    }

    // At least 2 out of 3 samples should be near bottom while following
    assert.ok(
      bottomCount >= 2,
      `Expected auto-scroll to keep view at bottom during streaming (${bottomCount}/3 samples were at bottom)`
    );
  });

});
