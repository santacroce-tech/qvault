/**
 * Mock for @tauri-apps/plugin-dialog when running in a regular browser.
 */
export async function open(_options?: any): Promise<string | null> {
  return null;
}

export async function save(_options?: any): Promise<string | null> {
  return null;
}

export async function message(_message: string, _options?: any): Promise<void> {}
export async function ask(_message: string, _options?: any): Promise<boolean> { return false; }
export async function confirm(_message: string, _options?: any): Promise<boolean> { return false; }
