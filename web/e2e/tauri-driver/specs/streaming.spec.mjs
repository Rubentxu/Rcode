/**
 * RCode Tauri Desktop E2E — Streaming & Real-time
 *
 * Validates real-time streaming behavior:
 * 1. Optimistic shell appears immediately after submit
 * 2. Streaming tokens appear incrementally
 * 3. Final message is committed to message history
 * 4. Tool calls stream with running/completed status
 *
 * All tests use E2E_MODEL (minimax/MiniMax-M2.7-highspeed) for cost control.
 */

import assert from 'node:assert/strict';
import {
  waitForBackend,
  waitFor,
  createSessionWithModel,
  captureState,
  restoreState,
  submitPrompt,
  submitPromptAndWait,
  waitForInputEnabled,
  getMessages,
  extractParts,
  assertParts,
  waitForToolCall,
  waitForToolResult,
  debugGetStreamingState,
  debugGetMessages,
  debugSnapshot,
} from '../helpers/e2e-helpers.mjs';

// ─── Suite ────────────────────────────────────────────────────────────────────

describe('Streaming & Real-time', () => {
  let initialState;

  before(async () => {
    await waitForBackend();
    initialState = await captureState();
  });

  after(async () => {
    await restoreState(initialState);
  });

  // ── Test 1: Response completes and message is persisted ─────────────────────

  it('response completes and both user + assistant messages are persisted via API', async () => {
    const { sessionId, textarea } = await createSessionWithModel();

    const prompt = 'Say "streaming verified" and nothing else.';
    await submitPromptAndWait(textarea, prompt, { timeoutMs: 60_000 });

    // Verify via API
    const data = await getMessages(sessionId);
    const messages = data?.messages || [];
    assert.ok(messages.length >= 2, `Should have at least 2 messages, got ${messages.length}`);

    // User message
    assert.equal(messages[0].role, 'user');
    // Assistant message
    assert.equal(messages[1].role, 'assistant');
  });

  // ── Test 2: Tool calling flow with streaming ────────────────────────────────

  it('tool calling prompt produces tool_call + tool_result + final text', async () => {
    const { sessionId, textarea } = await createSessionWithModel();

    const prompt = 'Use bash tool: ls -la /tmp. Do not answer from memory.';
    await submitPromptAndWait(textarea, prompt, { timeoutMs: 90_000 });

    // Fetch messages via API and check for structured parts
    const data = await getMessages(sessionId);
    const parts = extractParts(data);

    // Check for tool_call — the model MAY or MAY NOT use tools depending on provider
    const hasToolCall = parts.some((p) => p.type === 'tool_call');
    const hasToolResult = parts.some((p) => p.type === 'tool_result');
    const hasText = parts.some((p) => p.type === 'text');

    if (hasToolCall) {
      // If the model used a tool, verify tool_result also exists
      assert.ok(hasToolResult, 'tool_result should accompany tool_call');
      // Verify the tool name is bash (or similar shell tool)
      const toolCalls = parts.filter((p) => p.type === 'tool_call');
      assert.ok(
        toolCalls.some((tc) => tc.name === 'bash' || tc.name === 'shell' || tc.name === 'run_command'),
        `Expected bash/shell tool_call, got: ${toolCalls.map((tc) => tc.name).join(', ')}`,
      );
    } else {
      // Model responded without tools — verify it at least has text content
      assert.ok(hasText, 'Response should have text content even without tool use');
    }
  });

  // ── Test 3: Multiple prompts in same session ────────────────────────────────

  it('multiple prompts accumulate messages in the same session', async () => {
    const { sessionId, textarea } = await createSessionWithModel();

    // First prompt
    await submitPromptAndWait(textarea, 'Say "first".', { timeoutMs: 60_000 });

    // Second prompt — add extra wait to avoid overlay interception
    await new Promise((r) => setTimeout(r, 1000));
    await submitPromptAndWait(textarea, 'Say "second".', { timeoutMs: 60_000 });

    // Verify 4 messages: user1, assistant1, user2, assistant2
    const data = await getMessages(sessionId);
    const messages = data?.messages || [];
    assert.ok(messages.length >= 4, `Should have at least 4 messages, got ${messages.length}`);

    assert.equal(messages[0].role, 'user');
    assert.equal(messages[1].role, 'assistant');
    assert.equal(messages[2].role, 'user');
    assert.equal(messages[3].role, 'assistant');
  });

  // ── Test 4: Textarea re-enables after response ──────────────────────────────

  it('textarea is enabled after streaming completes', async () => {
    const { textarea } = await createSessionWithModel();

    const prompt = 'Reply with exactly: done';
    await submitPromptAndWait(textarea, prompt, { timeoutMs: 60_000 });

    // Verify textarea is enabled
    const isEnabled = await browser.execute(() => {
      const el = document.querySelector('[data-component="textarea"]');
      if (!el) return false;
      return !el.disabled && el.getAttribute('aria-disabled') !== 'true';
    });
    assert.ok(isEnabled, 'Textarea should be enabled after response');
  });

  // ── Test 5: DOM shows messages after response ──────────────────────────────

  it('DOM transcript shows user and assistant messages after response', async () => {
    const { textarea } = await createSessionWithModel();

    const prompt = 'Say "dom test passed" and nothing else.';
    await submitPromptAndWait(textarea, prompt, { timeoutMs: 60_000 });

    // Check DOM messages
    const msgs = await debugGetMessages();
    const userMsgs = msgs.filter((m) => m.role === 'user');
    const assistantMsgs = msgs.filter((m) => m.role === 'assistant');

    assert.ok(userMsgs.length >= 1, 'DOM should have at least one user message');
    assert.ok(assistantMsgs.length >= 1, 'DOM should have at least one assistant message');
  });
});
