/**
 * Mock for @tauri-apps/api/webview when running in a regular browser.
 */
export function getCurrentWebview() {
  return {
    async onDragDropEvent(_handler: any) {
      // No-op in browser
      return () => {};
    },
  };
}
