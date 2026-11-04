import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useVaultStore } from "../store/useVaultStore";
import type { WindowMode } from "../types/models";

/** Dual-mode window controls (popover flyout <-> maximized workspace). */
export function useWindowMode() {
  const mode = useVaultStore((s) => s.windowMode);
  const pinned = useVaultStore((s) => s.isPinned);
  const setWindowMode = useVaultStore((s) => s.setWindowMode);
  const setPinnedState = useVaultStore((s) => s.setPinnedState);

  const setMode = useCallback(
    async (next: WindowMode) => {
      const applied = await invoke<string>("set_window_mode", { mode: next });
      setWindowMode(applied as WindowMode);
    },
    [setWindowMode],
  );

  const toggleMaximize = useCallback(async () => {
    const applied = await invoke<string>("toggle_maximize_mode");
    setWindowMode(applied as WindowMode);
  }, [setWindowMode]);

  const hideToTray = useCallback(() => invoke("hide_to_tray"), []);

  const setPinned = useCallback(
    async (next: boolean) => {
      await invoke("set_pinned", { pinned: next });
      setPinnedState(next);
    },
    [setPinnedState],
  );

  return { mode, pinned, setMode, toggleMaximize, hideToTray, setPinned };
}
