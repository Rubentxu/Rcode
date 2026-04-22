/**
 * Filesystem utilities that wrap Tauri dialogs when running in Tauri context,
 * and provide safe fallbacks in browser environments.
 */

export async function openFolderPicker(): Promise<string | null> {
  if (typeof window !== "undefined" && (window as any).__TAURI__) {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Project Folder",
      });
      if (selected && typeof selected === "string") return selected;
    } catch (err) {
      console.error("[rcode] openFolderPicker failed:", err);
    }
  }
  return null;
}
