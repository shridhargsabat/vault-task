import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useVaultStore } from "../store/useVaultStore";

/**
 * Reports user activity to the Rust watchdog (throttled to one heartbeat
 * per 2s) and binds Ctrl/Cmd+L to instant lock. Enabled only while the
 * vault is unlocked.
 */
export function useActivityTracker(enabled: boolean) {
  const lockVault = useVaultStore((s) => s.lockVault);

  useEffect(() => {
    if (!enabled) return;
    let lastSent = 0;
    const beat = () => {
      const now = Date.now();
      if (now - lastSent < 2000) return;
      lastSent = now;
      invoke("record_activity_heartbeat").catch(() => {
        /* vault locked between check and call — watchdog owns that race */
      });
    };
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "l") {
        e.preventDefault();
        void lockVault();
        return;
      }
      beat();
    };
    const opts: AddEventListenerOptions = { passive: true };
    window.addEventListener("mousemove", beat, opts);
    window.addEventListener("mousedown", beat, opts);
    window.addEventListener("keydown", onKey);
    window.addEventListener("wheel", beat, opts);
    window.addEventListener("touchstart", beat, opts);
    beat(); // mounting while unlocked counts as activity
    return () => {
      window.removeEventListener("mousemove", beat);
      window.removeEventListener("mousedown", beat);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("wheel", beat);
      window.removeEventListener("touchstart", beat);
    };
  }, [enabled, lockVault]);
}
