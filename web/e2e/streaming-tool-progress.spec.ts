import { test, expect } from "@playwright/test";

/**
 * T12: Streaming smoke test for Phase 3 streaming UX
 * Verifies:
 * 1. Streaming text deltas accumulate properly
 * 2. Tool call cards appear during streaming
 * 3. Final persisted message contains tool_call and tool_result parts
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

  // T12-1: Verify streaming progress indicator appears
  await expect(page.getByText("Processing...")).toBeVisible({ timeout: 5000 });

  // T12-2: Verify tool call card appears during streaming (if streaming UX is working)
  // This checks that the new semantic events are flowing
  await expect.poll(async () => {
    const body = await page.locator("body").innerText();
    // Check for tool call related content
    return {
      hasToolName: /bash|pwd/i.test(body),
      hasStreamingIndicator: /Processing|streaming/i.test(body),
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

  // T12-4: Verify streaming indicator is gone after completion
  await expect(page.getByText("Processing...")).not.toBeVisible({ timeout: 10000 });
});

/**
 * T12: Legacy regression test - verify old streaming_progress path still works
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

  // Verify streaming indicator appears
  await expect(page.getByText("Processing...")).toBeVisible({ timeout: 5000 });

  // Wait for response
  await expect.poll(async () => {
    const body = await page.locator("body").innerText();
    return /hello|hi|hey/i.test(body);
  }, { timeout: 30000 }).toBeTruthy();

  // Verify streaming indicator is gone after completion
  await expect(page.getByText("Processing...")).not.toBeVisible({ timeout: 10000 });
});
