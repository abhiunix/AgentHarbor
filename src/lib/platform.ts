// Cross-platform detection utilities

const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
const platform = typeof navigator !== 'undefined' ? navigator.platform : '';

export const isMac = platform.toUpperCase().includes('MAC') || ua.includes('Macintosh');
export const isWindows = platform.toUpperCase().includes('WIN') || ua.includes('Windows');
export const isLinux = !isMac && !isWindows;

/** Keyboard modifier key display string */
export const modKey = isMac ? '⌘' : 'Ctrl';

/** Keyboard modifier symbol for shortcut display (e.g., "⌘K" or "Ctrl+K") */
export const modSymbol = isMac ? '⌘' : 'Ctrl+';

/** Platform file manager name */
export const fileManagerName = isMac ? 'Finder' : isWindows ? 'File Explorer' : 'File Manager';

/** Platform terminal name */
export const terminalName = isMac ? 'Terminal' : isWindows ? 'Command Prompt' : 'Terminal';

/** Platform credential store name */
export const credentialStoreName = isMac ? 'Keychain' : isWindows ? 'Credential Manager' : 'Secret Service';

/** Extract the last component from a file path (handles both / and \ separators) */
export function basename(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

/** Split a file path into components (handles both / and \ separators) */
export function pathParts(path: string): string[] {
  return path.split(/[/\\]/).filter(Boolean);
}
