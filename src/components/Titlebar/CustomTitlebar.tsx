import {
  Lock,
  Maximize2,
  Minimize2,
  Pin,
  PinOff,
  Search,
  X,
} from "lucide-react";
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

  const focusSearch = () => {
    // Phase 3 list UI listens for this to focus its search input.
    window.dispatchEvent(new CustomEvent("vaulttask:focus-search"));
  };

  return (
    <header className="flex h-11 shrink-0 items-center gap-0.5 border-b border-neutral-800 bg-neutral-900/90 px-1.5">
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
    </header>
  );
}
