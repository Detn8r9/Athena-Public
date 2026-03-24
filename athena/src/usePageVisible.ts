import { useEffect, useState } from "react";

/**
 * True when the document is visible (window in foreground / tab active).
 * Use to slow or pause polling timers and reduce CPU + battery use when idle in background.
 */
export function usePageVisible(): boolean {
  const [visible, setVisible] = useState(() =>
    typeof document === "undefined" ? true : document.visibilityState === "visible",
  );

  useEffect(() => {
    const sync = () => setVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", sync);
    return () => document.removeEventListener("visibilitychange", sync);
  }, []);

  return visible;
}
