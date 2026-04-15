import { useState, useRef, useCallback, RefObject } from "react";
import { Sidebar } from "./Sidebar";
import { Header } from "./Header";
import { MainContent } from "./MainContent";
import { ShortcutsHelp } from "../common/ShortcutsHelp";
import { UpdateBanner } from "../common/UpdateBanner";
import { useKeyboardShortcuts } from "../../hooks/useKeyboardShortcuts";

export function AppLayout() {
  const [showShortcutsHelp, setShowShortcutsHelp] = useState(false);
  const [showNewMenu, setShowNewMenu] = useState(false);
  const searchInputRef = useRef<HTMLInputElement>(null) as RefObject<HTMLInputElement>;

  const handleFocusSearch = useCallback(() => {
    searchInputRef.current?.focus();
  }, []);

  const handleToggleShortcutsHelp = useCallback(() => {
    setShowShortcutsHelp((prev) => !prev);
  }, []);

  const handleOpenNewMenu = useCallback(() => {
    setShowNewMenu(true);
  }, []);

  useKeyboardShortcuts({
    onFocusSearch: handleFocusSearch,
    onToggleShortcutsHelp: handleToggleShortcutsHelp,
    onOpenNewMenu: handleOpenNewMenu,
  });

  return (
    <div className="flex h-screen bg-app-bg text-text-primary">
      <UpdateBanner />
      <Sidebar />
      <div className="flex-1 flex flex-col overflow-hidden">
        <Header 
          searchInputRef={searchInputRef}
          showNewMenu={showNewMenu}
          setShowNewMenu={setShowNewMenu}
        />
        <MainContent />
      </div>
      <ShortcutsHelp 
        isOpen={showShortcutsHelp} 
        onClose={() => setShowShortcutsHelp(false)} 
      />
    </div>
  );
}
