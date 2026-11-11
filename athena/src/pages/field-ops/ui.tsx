import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";

/** Fields every stored Field Ops record carries, alongside its own data. */
export type Stored<T> = T & { id: string; createdAt: number; updatedAt: number };

export type Person = { id: string; name: string };

/**
 * Load data from a backend command, with loading and error state and a `reload` to call after a
 * change. A failed reload keeps the last good data on screen.
 */
export function useCommand<T>(command: string, args?: Record<string, unknown>) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const argsKey = JSON.stringify(args ?? {});
  const alive = useRef(true);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<T>(command, JSON.parse(argsKey) as Record<string, unknown>);
      if (!alive.current) return;
      setData(result);
      setError(null);
    } catch (e) {
      if (!alive.current) return;
      setError(messageOf(e));
    } finally {
      if (alive.current) setLoading(false);
    }
  }, [command, argsKey]);

  useEffect(() => {
    alive.current = true;
    void reload();
    return () => {
      alive.current = false;
    };
  }, [reload]);

  return { data, error, loading, reload };
}

export function messageOf(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Something went wrong.";
}

/** Run a change, then reload; report failure through `setError`. */
export async function runAction(
  action: () => Promise<unknown>,
  setError: (message: string | null) => void,
  after?: () => Promise<unknown> | void,
): Promise<boolean> {
  try {
    await action();
    setError(null);
    await after?.();
    return true;
  } catch (e) {
    setError(messageOf(e));
    return false;
  }
}

/** The group's members, for pickers. Guests are left out. */
export function usePeople(): Person[] {
  const [people, setPeople] = useState<Person[]>([]);
  useEffect(() => {
    let alive = true;
    type Row = {
      id: string;
      name: string;
      displayName?: string | null;
      display_name?: string | null;
      clearanceLevel?: number;
      clearance_level?: number;
    };
    void invoke<Row[]>("list_personnel")
      .then((rows) => {
        if (!alive || !Array.isArray(rows)) return;
        setPeople(
          rows
            .filter((r) => (r.clearanceLevel ?? r.clearance_level ?? 1) > 0)
            .map((r) => ({ id: r.id, name: (r.displayName ?? r.display_name)?.trim() || r.name }))
            .sort((a, b) => a.name.localeCompare(b.name)),
        );
      })
      .catch(() => {
        if (alive) setPeople([]);
      });
    return () => {
      alive = false;
    };
  }, []);
  return people;
}

export function Field({ label, grow, children }: { label: string; grow?: boolean; children: ReactNode }) {
  return (
    <label className={grow ? "fo-field fo-field--grow" : "fo-field"}>
      {label}
      {children}
    </label>
  );
}

export function ErrorNote({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <p className="fo-error" role="alert">
      {message}
    </p>
  );
}

export function Badge({ tone, children }: { tone?: "bad" | "warn" | "good"; children: ReactNode }) {
  return <span className={tone ? `fo-badge fo-badge--${tone}` : "fo-badge"}>{children}</span>;
}

export function formatDate(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "—";
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

export function formatDateTime(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "—";
  return new Date(unixSeconds * 1000).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** "3 h", "2 days", "40 min" for a span of seconds. */
export function formatSpan(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 90 * 60) return `${Math.max(1, Math.round(s / 60))} min`;
  if (s < 48 * 3600) return `${Math.round(s / 3600)} h`;
  return `${Math.round(s / 86400)} days`;
}

/** 6.5 -> "06:30". Hours past 24 wrap into the next day. */
export function formatHour(hour: number): string {
  const total = Math.round((((hour % 24) + 24) % 24) * 60);
  const h = Math.floor(total / 60) % 24;
  const m = total % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}
