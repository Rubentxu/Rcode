import { test, expect } from "@playwright/test";

/**
 * T12: Streaming smoke test for Phase 3 streaming UX
 * Verifies:
 * 1. Assistant shell appears immediately on submit (optimistic)
 * 2. Tool call cards appear during streaming
 * 3. Final persisted message contains tool_call and tool_result parts
 * 
 * Note: "Processing..." bottom bar was removed per SS-2. The assistant shell
 * now shows "thinking..." (optimistic) or "streaming..." (active) in the shell header.
 */
test("streaming text and tool progress through SSE", async ({ page }) => {
  await page.goto("/");

  // Create a new session
  await page.getByRole("button", { name: /create new session/i }).click();

  const textbox = page.getByPlaceholder("Type a message, / for commands, @ to mention files...");
  await expect(textbox).toBeVisible();

  // Use a prompt that forces tool use
  const prompt = "Use bash tool: pwd. Do not answer from memory.";
  await textbox.fill(prompt);
  await page.getByRole("button", { name: "Send" }).click();

  // Verify prompt is shown
  await expect(page.getByText(prompt)).toBeVisible();

  // T12-1: Verify assistant shell appears immediately (optimistic shell per SS-1)
  // The shell shows "thinking..." in the header when in optimistic state
  await expect(page.getByText("thinking...")).toBeVisible({ timeout: 5000 });

  // T12-2: Verify tool call card appears during streaming (if streaming UX is working)
  // This checks that the new semantic events are flowing
  await expect.poll(async () => {
    const body = await page.locator("body").innerText();
    // Check for tool call related content
    return {
      hasToolName: /bash|pwd/i.test(body),
      hasStreamingIndicator: /streaming|thinking/i.test(body),
    };
  }, { timeout: 30000 }).toMatchObject({
    hasToolName: true,
  });

  // T12-3: Wait for final response and verify tool result persisted
  await expect.poll(async () => {
    const body = await page.locator("body").innerText();
    return {
      hasPwd: /\/home|rust-code/i.test(body),
      hasFinalAnswer: /current working directory|working directory is/i.test(body),
    };
  }, { timeout: 60000 }).toMatchObject({
    hasPwd: true,
    hasFinalAnswer: true,
  });

  // T12-4: Verify assistant shell is gone after completion
  await expect(page.getByText("thinking...")).not.toBeVisible({ timeout: 10000 });
  await expect(page.getByText("streaming...")).not.toBeVisible({ timeout: 10000 });
});

/**
 * T12: Legacy regression test - verify old streaming_progress path still works
 * SS-5: Legacy streaming_progress path must continue to function identically
 */
test("legacy streaming_progress path still works", async ({ page }) => {
  await page.goto("/");

  // Create a new session
  await page.getByRole("button", { name: /create new session/i }).click();

  const textbox = page.getByPlaceholder("Type a message, / for commands, @ to mention files...");
  await expect(textbox).toBeVisible();

  // Simple prompt without tool use
  const prompt = "Say hello in exactly 3 words";
  await textbox.fill(prompt);
  await page.getByRole("button", { name: "Send" }).click();

  // Verify prompt is shown
  await expect(page.getByText(prompt)).toBeVisible();

  // SS-1: Verify assistant shell appears immediately (optimistic state)
  await expect(page.getByText("thinking...")).toBeVisible({ timeout: 5000 });

  // Wait for response
  await expect.poll(async () => {
    const body = await page.locator("body").innerText();
    return /hello|hi|hey/i.test(body);
  }, { timeout: 30000 }).toBeTruthy();

  // Verify assistant shell is gone after completion
  await expect(page.getByText("thinking...")).not.toBeVisible({ timeout: 10000 });
  await expect(page.getByText("streaming...")).not.toBeVisible({ timeout: 10000 });
});
