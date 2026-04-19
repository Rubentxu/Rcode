/**
 * Debug Inspector entry point.
 *
 * Only activates in Tauri context (where window.__TAURI__ exists).
 * This ensures the inspector is available in:
 * - `tauri dev` (DEV mode)
 * - `tauri build --debug` (debug builds for e2e tests)
 *
 * The inspector is read-only and exposes no dangerous operations.
 */

if (typeof window !== 'undefined' && Boolean((window as any).__TAURI__)) {
  import('./DebugInspector').then(({ install }) => install());
}

export {};
