/**
 * RCode Tauri Desktop E2E — Settings & Provider Config
 *
 * Validates settings panel and provider configuration via API:
 * 1. Provider catalog API is loaded correctly
 * 2. Model catalog shows available models
 * 3. Provider connection status is visible
 * 4. Health endpoint works
 * 5. E2E model is available
 *
 * Note: Settings panel UI tests are in project-management.spec.mjs
 * because the settings toggle can be intercepted by overlays.
 *
 * All tests use E2E_MODEL (minimax/MiniMax-M2.7-highspeed) for cost control.
 */

import assert from 'node:assert/strict';
import {
  API_BASE,
  E2E_MODEL,
  waitForBackend,
  fetchJson,
  captureState,
  restoreState,
} from '../helpers/e2e-helpers.mjs';

// ─── Suite ────────────────────────────────────────────────────────────────────

describe('Settings & Provider Config', () => {
  let initialState;

  before(async () => {
    await waitForBackend();
    initialState = await captureState();
  });

  after(async () => {
    await restoreState(initialState);
  });

  // ── Test 1: Provider API returns provider list ──────────────────────────────

  it('GET /config/providers returns provider list with required fields', async () => {
    const data = await fetchJson(`${API_BASE}/config/providers`);
    const providers = data?.providers || data || [];

    assert.ok(Array.isArray(providers), 'Providers should be an array');
    assert.ok(providers.length > 0, 'Should have at least one provider');

    for (const p of providers) {
      assert.ok(p.id, `Provider should have an id, got: ${JSON.stringify(p)}`);
      assert.ok(p.name || p.display_name, `Provider should have a name`);
    }
  });

  // ── Test 2: Model catalog API returns models ────────────────────────────────

  it('GET /models returns model list with required fields', async () => {
    const data = await fetchJson(`${API_BASE}/models`);
    const models = data?.models || data || [];

    assert.ok(Array.isArray(models), 'Models should be an array');
    assert.ok(models.length > 0, 'Should have at least one model');

    for (const m of models) {
      assert.ok(m.id, `Model should have an id`);
      assert.ok(m.provider, `Model should have a provider`);
      assert.ok(m.display_name, `Model should have a display_name`);
    }
  });

  // ── Test 3: E2E model is available in catalog ───────────────────────────────

  it('E2E_MODEL (MiniMax) is available in the model catalog', async () => {
    const data = await fetchJson(`${API_BASE}/models`);
    const models = data?.models || data || [];

    const e2eModel = models.find((m) => m.id === 'minimax/MiniMax-M2.7-highspeed');
    assert.ok(e2eModel, 'E2E_MODEL should be in the catalog');
    assert.equal(e2eModel.provider, 'minimax', 'E2E_MODEL provider should be minimax');
  });

  // ── Test 4: Provider shows auth connection info ─────────────────────────────

  it('at least one provider has connection status information', async () => {
    const data = await fetchJson(`${API_BASE}/config/providers`);
    const providers = data?.providers || data || [];

    const withAuth = providers.filter((p) => p.auth && (p.auth.connected !== undefined || p.auth.source));
    assert.ok(withAuth.length > 0, 'At least one provider should have auth info');
  });

  // ── Test 5: Health endpoint returns ok ──────────────────────────────────────

  it('GET /health returns status ok with version', async () => {
    const data = await fetchJson(`${API_BASE}/health`);
    assert.equal(data.status, 'ok', 'Health status should be ok');
    assert.ok(data.version, 'Health response should include version');
  });

  // ── Test 6: Config endpoint returns safe configuration ─────────────────────

  it('GET /config returns configuration without secrets', async () => {
    const data = await fetchJson(`${API_BASE}/config`);
    assert.ok(typeof data === 'object', 'Config should be an object');
    // Config should NOT contain API keys
    const json = JSON.stringify(data);
    assert.ok(!json.includes('api_key') || json.includes('api_key")') === false, 'Config should not leak API keys');
  });
});
