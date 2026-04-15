import { SHORTCUTS } from "../../hooks/useKeyboardShortcuts";

interface ShortcutsHelpProps {
  isOpen: boolean;
  onClose: () => void;
}

export function ShortcutsHelp({ isOpen, onClose }: ShortcutsHelpProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center">
      <div 
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative bg-app-sidebar border border-border rounded-xl w-full max-w-md shadow-2xl">
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <h2 className="text-lg font-semibold text-text-primary">Keyboard Shortcuts</h2>
          <button
            onClick={onClose}
            className="w-8 h-8 flex items-center justify-center rounded-full hover:bg-white/10 text-text-muted hover:text-text-primary transition-colors"
          >
            ✕
          </button>
        </div>

        <div className="p-6 space-y-3">
          {SHORTCUTS.map((shortcut, index) => (
            <div
              key={index}
              className="flex items-center justify-between py-2"
            >
              <span className="text-sm text-text-primary">{shortcut.description}</span>
              <div className="flex items-center gap-1">
                {shortcut.keys.map((key, keyIndex) => (
                  <kbd
                    key={keyIndex}
                    className="px-2 py-1 text-xs font-mono bg-app-bg border border-border rounded text-text-muted"
                  >
                    {key}
                  </kbd>
                ))}
              </div>
            </div>
          ))}
        </div>

        <div className="px-6 py-4 border-t border-border">
          <p className="text-xs text-text-muted text-center">
            Press <kbd className="px-1.5 py-0.5 text-[10px] font-mono bg-app-bg border border-border rounded">Esc</kbd> to close
          </p>
        </div>
      </div>
    </div>
  );
}
