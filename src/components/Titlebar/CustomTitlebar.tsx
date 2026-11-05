import { useState } from "react";
import {
  Lock,
  Maximize2,
  Minimize2,
  MoreHorizontal,
  Pin,
  PinOff,
  Power,
  RefreshCw,
  Search,
  X,
} from "lucide-react";
import { exit } from "@tauri-apps/plugin-process";
import { useVaultStore } from "../../store/useVaultStore";
import { useWindowMode } from "../../hooks/useWindowMode";

function BarButton(props: {
  title: string;
  onClick: () => void;
  active?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      title={props.title}
      onClick={props.onClick}
      className={`grid h-8 w-8 shrink-0 place-items-center rounded-md transition-colors ${
        props.active
          ? "bg-emerald-600/30 text-emerald-300"
          : "text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
      }`}
    >
      {props.children}
    </button>
  );
}

/**
 * Custom draggable titlebar for the frameless window.
 * Only the middle spacer is a drag region so buttons stay clickable.
 */
export function CustomTitlebar() {
  const { mode, pinned, toggleMaximize, hideToTray, setPinned } =
    useWindowMode();
  const lockVault = useVaultStore((s) => s.lockVault);
  const locked = useVaultStore((s) => s.status?.locked ?? true);
  const requestUpdateCheck = useVaultStore((s) => s.requestUpdateCheck);
  const [menuOpen, setMenuOpen] = useState(false);

  const focusSearch = () => {
    // Phase 3 list UI listens for this to focus its search input.
    window.dispatchEvent(new CustomEvent("vaulttask:focus-search"));
  };

  return (
    <header className="relative flex h-11 shrink-0 items-center gap-0.5 border-b border-neutral-800 bg-neutral-900/90 px-1.5">
      <span
        title={locked ? "Vault locked" : "Vault unlocked"}
        className={`ml-1 h-2 w-2 shrink-0 rounded-full ${
          locked ? "bg-amber-400" : "bg-emerald-400"
        }`}
      />
      <BarButton title="Search (Ctrl+K)" onClick={focusSearch}>
        <Search className="h-4 w-4" />
      </BarButton>

      {/* Draggable spacer — drag the frameless window from here. */}
      <div data-tauri-drag-region className="h-full min-w-4 flex-1" />

      {mode === "popover" && (
        <BarButton
          title={pinned ? "Unpin popover" : "Pin popover"}
          onClick={() => void setPinned(!pinned)}
          active={pinned}
        >
          {pinned ? <PinOff className="h-4 w-4" /> : <Pin className="h-4 w-4" />}
        </BarButton>
      )}
      <BarButton title="Lock now (Ctrl+L)" onClick={() => void lockVault()}>
        <Lock className="h-4 w-4" />
      </BarButton>
      <BarButton
        title={mode === "maximized" ? "Restore compact popover" : "Maximize workspace"}
        onClick={() => void toggleMaximize()}
      >
        {mode === "maximized" ? (
          <Minimize2 className="h-4 w-4" />
        ) : (
          <Maximize2 className="h-4 w-4" />
        )}
      </BarButton>
      <BarButton title="Hide to tray" onClick={() => void hideToTray()}>
        <X className="h-4 w-4" />
      </BarButton>
      <BarButton
        title="More actions"
        onClick={() => setMenuOpen((v) => !v)}
        active={menuOpen}
      >
        <MoreHorizontal className="h-4 w-4" />
      </BarButton>
      {menuOpen && (
        <>
          {/* Click-outside dismiss (app-native, no OS menus). */}
          <div className="fixed inset-0 z-40" onClick={() => setMenuOpen(false)} />
          <div className="absolute right-1.5 top-11 z-50 w-52 rounded-lg border border-neutral-700 bg-neutral-900 py-1 shadow-xl">
            <button
              onClick={() => {
                setMenuOpen(false);
                requestUpdateCheck();
              }}
              className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-neutral-200 hover:bg-neutral-800"
            >
              <RefreshCw className="h-3.5 w-3.5 text-neutral-400" />
              Check for Updates
            </button>
            <button
              onClick={() => void exit(0)}
              className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-red-300 hover:bg-neutral-800"
            >
              <Power className="h-3.5 w-3.5" />
              Quit VaultTask
            </button>
          </div>
        </>
      )}
    </header>
  );
}
