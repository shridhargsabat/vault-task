import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Debounced text field that flushes pending edits when the edited entity
 * (`key`, e.g. task id) changes or the component unmounts — so a 300ms
 * pending note save is never lost or misrouted on quick task switching.
 */
export function useDebouncedField(
  key: string,
  initial: string,
  delay: number,
  onSave: (key: string, value: string) => void,
): readonly [string, (next: string) => void, boolean] {
  const [value, setValue] = useState(initial);
  const [dirty, setDirty] = useState(false);
  const timer = useRef<number | null>(null);
  const savedRef = useRef(initial);
  const latestRef = useRef(initial);
  const pendingKeyRef = useRef(key);
  const saveRef = useRef(onSave);
  saveRef.current = onSave;

  // Entity switch: flush the previous entity's pending save, then reset.
  useEffect(() => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    if (latestRef.current !== savedRef.current) {
      saveRef.current(pendingKeyRef.current, latestRef.current);
    }
    pendingKeyRef.current = key;
    savedRef.current = initial;
    latestRef.current = initial;
    setValue(initial);
    setDirty(false);
    // `initial` intentionally excluded: reset is keyed on entity switch only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  // Unmount: flush.
  useEffect(
    () => () => {
      if (timer.current) {
        clearTimeout(timer.current);
        timer.current = null;
      }
      if (latestRef.current !== savedRef.current) {
        saveRef.current(pendingKeyRef.current, latestRef.current);
      }
    },
    [],
  );

  const update = useCallback(
    (next: string) => {
      pendingKeyRef.current = key;
      latestRef.current = next;
      setValue(next);
      setDirty(next !== savedRef.current);
      if (timer.current) clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        timer.current = null;
        if (next !== savedRef.current) {
          savedRef.current = next;
          setDirty(false);
          saveRef.current(key, next);
        }
      }, delay);
    },
    [delay, key],
  );

  return [value, update, dirty] as const;
}
