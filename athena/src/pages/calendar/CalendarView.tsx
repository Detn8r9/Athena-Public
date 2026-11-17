import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../shared/ipc/athenaIpc";
import type { MapMarker } from "../../domains/features/map/MapView";
import { getRegionalHolidays, type HolidayEvent } from "../../shared/lib/calendarHolidays";
import { useLexicon } from "../../shared/lexicon";

export type CalendarEventRow = {
  id: string;
  title: string;
  notes: string | null;
  start_ts: number;
  end_ts: number;
  all_day: boolean;
  location_label: string | null;
  linked_marker_id: string | null;
  participant_names: string[];
  reminder_minutes_before: number | null;
  recurrence_rule: string | null;
  category: string | null;
  source: string;
  updated_at: number;
  origin_node_id: number | null;
  deleted: boolean;
};

type Occurrence = CalendarEventRow & { occurrence_start_ts: number; occurrence_end_ts: number };
type CalendarTab = "month" | "week" | "day" | "agenda";

type CalendarViewProps = {
  timeFormat: "12" | "24";
  calendarRegion: string;
  quietHoursStart: number;
  quietHoursEnd: number;
  meshConnected: boolean;
  onGoToMarker: (markerId: string | null) => void;
  onOpenMapView: () => void;
};

const MS_DAY = 24 * 60 * 60 * 1000;

function formatDateTime(ts: number, timeFormat: "12" | "24"): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
    hour12: timeFormat === "12",
  }).format(new Date(ts));
}

function formatTimeOnly(ts: number, timeFormat: "12" | "24"): string {
  return new Intl.DateTimeFormat(undefined, {
    timeStyle: "short",
    hour12: timeFormat === "12",
  }).format(new Date(ts));
}

function dayKey(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function startOfWeekSunday(d: Date): Date {
  const x = new Date(d);
  x.setHours(0, 0, 0, 0);
  const day = x.getDay();
  x.setDate(x.getDate() - day);
  return x;
}

function parseQuickAdd(input: string, untitledLabel: string): Partial<CalendarEventRow> | null {
  const t = input.trim();
  if (!t) return null;
  const dateMatch = t.match(/(\d{4}-\d{2}-\d{2})/);
  const timeMatch = t.match(/\b(\d{1,2})(?::(\d{2}))?\s*(am|pm)?\b/i);
  const locationMatch = t.match(/\bat\s+(.+)$/i);
  const now = new Date();
  const when = new Date(now);
  if (dateMatch) {
    const [yy, mm, dd] = dateMatch[1].split("-").map((n) => Number.parseInt(n, 10));
    if (Number.isFinite(yy) && Number.isFinite(mm) && Number.isFinite(dd)) {
      when.setFullYear(yy, mm - 1, dd);
    }
  } else if (/\btomorrow\b/i.test(t)) {
    when.setDate(when.getDate() + 1);
  }
  if (timeMatch) {
    let hour = Number.parseInt(timeMatch[1], 10);
    const minute = Number.parseInt(timeMatch[2] ?? "0", 10);
    const ap = (timeMatch[3] ?? "").toLowerCase();
    if (ap === "pm" && hour < 12) hour += 12;
    if (ap === "am" && hour === 12) hour = 0;
    when.setHours(hour, minute, 0, 0);
  } else {
    when.setHours(9, 0, 0, 0);
  }
  const title = t
    .replace(dateMatch?.[0] ?? "", "")
    .replace(timeMatch?.[0] ?? "", "")
    .replace(locationMatch?.[0] ?? "", "")
    .replace(/\s+/g, " ")
    .trim();
  const start = when.getTime();
  const end = start + 60 * 60 * 1000;
  return {
    title: title || untitledLabel,
    location_label: locationMatch?.[1]?.trim() ?? null,
    start_ts: start,
    end_ts: end,
    all_day: false,
  };
}

function eventIntersects(start: number, end: number, rangeStart: number, rangeEnd: number): boolean {
  return end >= rangeStart && start <= rangeEnd;
}

function expandOccurrences(event: CalendarEventRow, rangeStart: number, rangeEnd: number): Occurrence[] {
  const rule = (event.recurrence_rule ?? "").trim().toLowerCase();
  if (!rule) {
    return eventIntersects(event.start_ts, event.end_ts, rangeStart, rangeEnd)
      ? [{ ...event, occurrence_start_ts: event.start_ts, occurrence_end_ts: event.end_ts }]
      : [];
  }
  const duration = Math.max(1, event.end_ts - event.start_ts);
  const out: Occurrence[] = [];
  let cursor = new Date(event.start_ts);
  const hardCap = 500;
  for (let i = 0; i < hardCap; i += 1) {
    const start = cursor.getTime();
    const end = start + duration;
    if (start > rangeEnd) break;
    if (eventIntersects(start, end, rangeStart, rangeEnd)) {
      out.push({ ...event, occurrence_start_ts: start, occurrence_end_ts: end });
    }
    if (rule === "daily") cursor.setDate(cursor.getDate() + 1);
    else if (rule === "weekly") cursor.setDate(cursor.getDate() + 7);
    else if (rule === "monthly") cursor.setMonth(cursor.getMonth() + 1);
    else if (rule === "yearly") cursor.setFullYear(cursor.getFullYear() + 1);
    else break;
  }
  return out;
}

function buildIcs(events: CalendarEventRow[]): string {
  const lines: string[] = ["BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Athena//Calendar//EN"];
  for (const ev of events) {
    if (ev.deleted || ev.source === "holiday") continue;
    const start = new Date(ev.start_ts).toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
    const end = new Date(ev.end_ts).toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
    lines.push("BEGIN:VEVENT");
    lines.push(`UID:${ev.id}`);
    lines.push(`DTSTART:${start}`);
    lines.push(`DTEND:${end}`);
    lines.push(`SUMMARY:${(ev.title || "").replace(/\n/g, " ")}`);
    if (ev.notes) lines.push(`DESCRIPTION:${ev.notes.replace(/\n/g, "\\n")}`);
    if (ev.location_label) lines.push(`LOCATION:${ev.location_label.replace(/\n/g, " ")}`);
    if (ev.recurrence_rule) lines.push(`RRULE:FREQ=${ev.recurrence_rule.toUpperCase()}`);
    lines.push("END:VEVENT");
  }
  lines.push("END:VCALENDAR");
  return `${lines.join("\r\n")}\r\n`;
}

function categoryDotClass(category: string | null): string {
  const c = (category ?? "").toLowerCase();
  if (c === "holiday" || c === "health") return "calendar-dot-health";
  if (c === "mission" || c === "comms") return "calendar-dot-mission";
  if (c === "maintenance") return "calendar-dot-maint";
  return "calendar-dot-default";
}

function buildMonthCells(visibleMonth: Date): (Date | null)[] {
  const y = visibleMonth.getFullYear();
  const m = visibleMonth.getMonth();
  const first = new Date(y, m, 1);
  const pad = first.getDay();
  const daysInMonth = new Date(y, m + 1, 0).getDate();
  const cells: (Date | null)[] = [];
  for (let i = 0; i < pad; i += 1) cells.push(null);
  for (let d = 1; d <= daysInMonth; d += 1) cells.push(new Date(y, m, d));
  while (cells.length % 7 !== 0) cells.push(null);
  while (cells.length < 42) cells.push(null);
  return cells;
}

function assignOccurrencesToDays(occurrences: Occurrence[]): Map<string, Occurrence[]> {
  const map = new Map<string, Occurrence[]>();
  for (const occ of occurrences) {
    const start = new Date(occ.occurrence_start_ts);
    const end = new Date(occ.occurrence_end_ts);
    const cursor = new Date(start.getFullYear(), start.getMonth(), start.getDate());
    const endDay = new Date(end.getFullYear(), end.getMonth(), end.getDate());
    while (cursor <= endDay) {
      const k = dayKey(cursor);
      const list = map.get(k) ?? [];
      list.push(occ);
      map.set(k, list);
      cursor.setDate(cursor.getDate() + 1);
    }
  }
  for (const [, list] of map) {
    list.sort((a, b) => a.occurrence_start_ts - b.occurrence_start_ts);
  }
  return map;
}

export function CalendarView({
  timeFormat,
  calendarRegion,
  quietHoursStart,
  quietHoursEnd,
  meshConnected,
  onGoToMarker,
  onOpenMapView,
}: CalendarViewProps) {
  const lx = useLexicon();
  const templates = useMemo(
    () =>
      [
        { name: lx.calendarTemplateMedName, title: lx.calendarTemplateMedTitle, category: "health", reminder: 30 },
        { name: lx.calendarTemplatePatrolName, title: lx.calendarTemplatePatrolTitle, category: "mission", reminder: 60 },
        { name: lx.calendarTemplateSupplyName, title: lx.calendarTemplateSupplyTitle, category: "maintenance", reminder: 120 },
        { name: lx.calendarTemplateRadioName, title: lx.calendarTemplateRadioTitle, category: "comms", reminder: 15 },
      ],
    [lx],
  );
  const [events, setEvents] = useState<CalendarEventRow[]>([]);
  const [markers, setMarkers] = useState<MapMarker[]>([]);
  const [tab, setTab] = useState<CalendarTab>("month");
  const [query, setQuery] = useState("");
  const [quickAdd, setQuickAdd] = useState("");
  const [selectedDay, setSelectedDay] = useState(() => new Date());
  const [visibleMonth, setVisibleMonth] = useState(() => {
    const d = new Date();
    return new Date(d.getFullYear(), d.getMonth(), 1);
  });
  const [draft, setDraft] = useState<CalendarEventRow>({
    id: "",
    title: "",
    notes: null,
    start_ts: Date.now() + 30 * 60 * 1000,
    end_ts: Date.now() + 90 * 60 * 1000,
    all_day: false,
    location_label: null,
    linked_marker_id: null,
    participant_names: [],
    reminder_minutes_before: 30,
    recurrence_rule: null,
    category: "personal",
    source: "user",
    updated_at: 0,
    origin_node_id: null,
    deleted: false,
  });
  const [error, setError] = useState<string | null>(null);
  const [notices, setNotices] = useState<string[]>([]);
  const [showMoreActions, setShowMoreActions] = useState(false);
  const [eventEditorOpen, setEventEditorOpen] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  const draftDefaultsForSelectedDay = useCallback((): CalendarEventRow => {
    const start = new Date(selectedDay);
    start.setHours(9, 0, 0, 0);
    const end = new Date(start);
    end.setHours(10, 0, 0, 0);
    return {
      id: "",
      title: "",
      notes: null,
      start_ts: start.getTime(),
      end_ts: end.getTime(),
      all_day: false,
      location_label: null,
      linked_marker_id: null,
      participant_names: [],
      reminder_minutes_before: 30,
      recurrence_rule: null,
      category: "personal",
      source: "user",
      updated_at: 0,
      origin_node_id: null,
      deleted: false,
    };
  }, [selectedDay]);

  const openNewEventSheet = useCallback(() => {
    setDraft(draftDefaultsForSelectedDay());
    setEventEditorOpen(true);
  }, [draftDefaultsForSelectedDay]);

  const closeNewEventSheet = useCallback(() => {
    setEventEditorOpen(false);
  }, []);

  const monthStart = new Date(visibleMonth.getFullYear(), visibleMonth.getMonth(), 1).getTime();
  const monthEnd = new Date(visibleMonth.getFullYear(), visibleMonth.getMonth() + 1, 0, 23, 59, 59, 999).getTime();
  const rangeStart = monthStart - 14 * MS_DAY;
  const rangeEnd = monthEnd + 14 * MS_DAY;

  const dayStart = new Date(selectedDay.getFullYear(), selectedDay.getMonth(), selectedDay.getDate()).getTime();
  const dayEnd = dayStart + MS_DAY - 1;

  async function load() {
    setError(null);
    try {
      const [dbEvents, markerRows] = await Promise.all([
        invoke<CalendarEventRow[]>("calendar_list_events", { rangeStart, rangeEnd, includeDeleted: false }),
        invoke<MapMarker[]>("get_map_markers").catch(() => []),
      ]);
      setEvents(Array.isArray(dbEvents) ? dbEvents : []);
      setMarkers(Array.isArray(markerRows) ? markerRows : []);
    } catch (e) {
      setError(String(e));
    }
  }

  const loadRef = useRef(load);
  useEffect(() => {
    loadRef.current = load;
  }, [rangeStart, rangeEnd]);

  useEffect(() => {
    void load();
  }, [rangeStart, rangeEnd]);

  useEffect(() => {
    const unlisten = listen("athena:calendar-updated", () => void loadRef.current());
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const timer = setInterval(() => {
      const now = Date.now();
      const nowHour = new Date(now).getHours();
      const quietWindowActive =
        quietHoursStart === quietHoursEnd
          ? false
          : quietHoursStart < quietHoursEnd
            ? nowHour >= quietHoursStart && nowHour < quietHoursEnd
            : nowHour >= quietHoursStart || nowHour < quietHoursEnd;
      if (quietWindowActive) return;
      const due = events.filter((ev) => {
        if (ev.deleted || ev.reminder_minutes_before == null) return false;
        const remindAt = ev.start_ts - ev.reminder_minutes_before * 60 * 1000;
        return remindAt <= now && remindAt >= now - 30_000;
      });
      if (due.length) {
        setNotices((prev) => [...due.map((d) => `Reminder: ${d.title}`), ...prev].slice(0, 4));
      }
    }, 30_000);
    return () => clearInterval(timer);
  }, [events, quietHoursStart, quietHoursEnd]);

  const holidayRows = useMemo<CalendarEventRow[]>(() => {
    const years = new Set<number>([new Date(rangeStart).getFullYear(), new Date(rangeEnd).getFullYear()]);
    const out: CalendarEventRow[] = [];
    for (const y of years) {
      const rows: HolidayEvent[] = getRegionalHolidays(y, calendarRegion);
      for (const h of rows) {
        out.push({
          id: h.id,
          title: h.title,
          notes: h.notes,
          start_ts: h.start_ts,
          end_ts: h.end_ts,
          all_day: true,
          location_label: null,
          linked_marker_id: null,
          participant_names: [],
          reminder_minutes_before: null,
          recurrence_rule: null,
          category: h.category,
          source: "holiday",
          updated_at: h.start_ts,
          origin_node_id: null,
          deleted: false,
        });
      }
    }
    return out;
  }, [rangeStart, rangeEnd, calendarRegion]);

  const occurrences = useMemo(() => {
    const merged = [...events, ...holidayRows];
    const expanded = merged.flatMap((ev) => expandOccurrences(ev, rangeStart, rangeEnd));
    return expanded
      .filter((ev) =>
        query.trim()
          ? `${ev.title} ${ev.notes ?? ""} ${ev.location_label ?? ""}`.toLowerCase().includes(query.trim().toLowerCase())
          : true,
      )
      .sort((a, b) => a.occurrence_start_ts - b.occurrence_start_ts);
  }, [events, holidayRows, rangeStart, rangeEnd, query]);

  const occurrencesByDay = useMemo(() => assignOccurrencesToDays(occurrences), [occurrences]);

  const upcoming = occurrences.filter((o) => o.occurrence_start_ts >= Date.now()).slice(0, 8);
  const forSelectedDay = occurrences.filter((o) => eventIntersects(o.occurrence_start_ts, o.occurrence_end_ts, dayStart, dayEnd));

  const weekDays = useMemo(() => {
    const ws = startOfWeekSunday(selectedDay);
    const out: Date[] = [];
    for (let i = 0; i < 7; i += 1) {
      const d = new Date(ws);
      d.setDate(ws.getDate() + i);
      out.push(d);
    }
    return out;
  }, [selectedDay.getTime()]);

  const monthCells = useMemo(() => buildMonthCells(visibleMonth), [visibleMonth.getTime()]);

  const weekLabel = useMemo(() => {
    const a = weekDays[0];
    const b = weekDays[6];
    return `${a.toLocaleDateString(undefined, { month: "short", day: "numeric" })} – ${b.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`;
  }, [weekDays]);

  const monthTitle = visibleMonth.toLocaleDateString(undefined, { month: "long", year: "numeric" });

  function goToday() {
    const n = new Date();
    setSelectedDay(n);
    setVisibleMonth(new Date(n.getFullYear(), n.getMonth(), 1));
  }

  function prevNav() {
    if (tab === "month") {
      setVisibleMonth((m) => new Date(m.getFullYear(), m.getMonth() - 1, 1));
    } else if (tab === "week") {
      setSelectedDay((d) => {
        const x = new Date(d);
        x.setDate(x.getDate() - 7);
        return x;
      });
    } else if (tab === "day") {
      setSelectedDay((d) => {
        const x = new Date(d);
        x.setDate(x.getDate() - 1);
        return x;
      });
    } else {
      setVisibleMonth((m) => new Date(m.getFullYear(), m.getMonth() - 1, 1));
    }
  }

  function nextNav() {
    if (tab === "month") {
      setVisibleMonth((m) => new Date(m.getFullYear(), m.getMonth() + 1, 1));
    } else if (tab === "week") {
      setSelectedDay((d) => {
        const x = new Date(d);
        x.setDate(x.getDate() + 7);
        return x;
      });
    } else if (tab === "day") {
      setSelectedDay((d) => {
        const x = new Date(d);
        x.setDate(x.getDate() + 1);
        return x;
      });
    } else {
      setVisibleMonth((m) => new Date(m.getFullYear(), m.getMonth() + 1, 1));
    }
  }

  const navTitle =
    tab === "month"
      ? monthTitle
      : tab === "week"
        ? weekLabel
        : tab === "day"
          ? selectedDay.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric", year: "numeric" })
          : monthTitle;

  async function saveDraft() {
    setError(null);
    try {
      await invoke<CalendarEventRow>("calendar_upsert_event", { event: { ...draft, updated_at: Date.now(), source: "user" } });
      setDraft(draftDefaultsForSelectedDay());
      setEventEditorOpen(false);
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    if (!eventEditorOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeNewEventSheet();
    };
    window.addEventListener("keydown", onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = prev;
    };
  }, [eventEditorOpen, closeNewEventSheet]);

  async function deleteEvent(id: string) {
    setError(null);
    try {
      await invoke("calendar_delete_event", { id });
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  async function requestMeshSync() {
    if (!meshConnected) return;
    try {
      await invoke("calendar_request_mesh_sync");
    } catch (e) {
      setError(String(e));
    }
  }

  async function importIcsText(text: string) {
    const chunks = text.split("BEGIN:VEVENT").slice(1);
    for (const chunk of chunks) {
      const uid = chunk.match(/UID:(.+)/)?.[1]?.trim() ?? `ics-${Date.now()}-${Math.random().toString(16).slice(2)}`;
      const summary = chunk.match(/SUMMARY:(.+)/)?.[1]?.trim() ?? lx.calendarImportedEventDefault;
      const dtStart = chunk.match(/DTSTART:(.+)/)?.[1]?.trim();
      const dtEnd = chunk.match(/DTEND:(.+)/)?.[1]?.trim();
      if (!dtStart || !dtEnd) continue;
      const start = Date.parse(dtStart.replace(/^(\d{8})T(\d{6})Z$/, "$1T$2Z"));
      const end = Date.parse(dtEnd.replace(/^(\d{8})T(\d{6})Z$/, "$1T$2Z"));
      if (!Number.isFinite(start) || !Number.isFinite(end)) continue;
      const existing = events.find((e) => e.id === uid);
      if (existing && existing.updated_at > Date.now() - 5_000) {
        continue;
      }
      await invoke("calendar_upsert_event", {
        event: {
          id: uid,
          title: summary,
          notes: chunk.match(/DESCRIPTION:(.+)/)?.[1]?.replace(/\\n/g, "\n") ?? null,
          start_ts: start,
          end_ts: end,
          all_day: false,
          location_label: chunk.match(/LOCATION:(.+)/)?.[1]?.trim() ?? null,
          linked_marker_id: null,
          participant_names: [],
          reminder_minutes_before: null,
          recurrence_rule: chunk.match(/RRULE:FREQ=([A-Z]+)/)?.[1]?.toLowerCase() ?? null,
          category: "imported",
          source: "user",
          updated_at: Date.now(),
          origin_node_id: null,
          deleted: false,
        },
      });
    }
    await load();
  }

  const today = new Date();
  const isToday = (d: Date | null) =>
    d != null && d.getDate() === today.getDate() && d.getMonth() === today.getMonth() && d.getFullYear() === today.getFullYear();

  const isSelected = (d: Date | null) =>
    d != null &&
    d.getDate() === selectedDay.getDate() &&
    d.getMonth() === selectedDay.getMonth() &&
    d.getFullYear() === selectedDay.getFullYear();

  const weekDayNames = useMemo(() => {
    const base = new Date(2024, 6, 7);
    return Array.from({ length: 7 }, (_, i) => {
      const x = new Date(base);
      x.setDate(base.getDate() + i);
      return x.toLocaleDateString(undefined, { weekday: "short" });
    });
  }, []);

  const hours = Array.from({ length: 24 }, (_, h) => h);

  return (
    <div className="calendar-view calendar-apple">
      <header className="calendar-top-bar glass-panel">
        <div className="calendar-nav-cluster">
          <button type="button" className="calendar-nav-btn" aria-label={lx.calendarAriaPrev} onClick={prevNav}>
            ‹
          </button>
          <button type="button" className="calendar-nav-btn" aria-label={lx.calendarAriaNext} onClick={nextNav}>
            ›
          </button>
          <h1 className="calendar-nav-title">{navTitle}</h1>
          <button type="button" className="calendar-today-btn" onClick={goToday}>
            {lx.calendarToday}
          </button>
        </div>
        <div className="calendar-segmented" role="tablist">
          {(["month", "week", "day", "agenda"] as const).map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={tab === t}
              className={tab === t ? "calendar-segment-active" : ""}
              onClick={() => setTab(t)}
            >
              {t === "month"
                ? lx.calendarTabMonth
                : t === "week"
                  ? lx.calendarTabWeek
                  : t === "day"
                    ? lx.calendarTabDay
                    : lx.calendarTabAgenda}
            </button>
          ))}
        </div>
        <div className="calendar-top-search">
          <input
            className="calendar-search-field"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={lx.calendarSearchPlaceholder}
            aria-label={lx.calendarSearchAria}
          />
          <button type="button" className="calendar-more-toggle secondary" onClick={() => setShowMoreActions((v) => !v)}>
            {lx.calendarMore}
          </button>
        </div>
      </header>

      {showMoreActions ? (
        <div className="calendar-more-row glass-panel">
          <input
            className="calendar-quick-field"
            value={quickAdd}
            onChange={(e) => setQuickAdd(e.target.value)}
            placeholder={lx.calendarQuickAddPlaceholder}
          />
          <button
            type="button"
            className="secondary"
            onClick={() => {
              const parsed = parseQuickAdd(quickAdd, lx.calendarUntitledEvent);
              if (!parsed) return;
              setDraft((prev) => ({ ...prev, ...parsed }));
              setQuickAdd("");
            }}
          >
            {lx.calendarParse}
          </button>
          <button type="button" className="secondary" onClick={() => void requestMeshSync()} disabled={!meshConnected}>
            {lx.calendarPullMesh}
          </button>
          <button
            type="button"
            className="secondary"
            onClick={() => {
              const blob = new Blob([buildIcs(events)], { type: "text/calendar;charset=utf-8" });
              const url = URL.createObjectURL(blob);
              const a = document.createElement("a");
              a.href = url;
              a.download = "athena-calendar.ics";
              a.click();
              URL.revokeObjectURL(url);
            }}
          >
            {lx.calendarExportIcs}
          </button>
          <button type="button" className="secondary" onClick={() => fileRef.current?.click()}>
            {lx.calendarImportIcs}
          </button>
          <input
            ref={fileRef}
            type="file"
            accept=".ics,text/calendar"
            style={{ display: "none" }}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (!f) return;
              void f.text().then((text) => importIcsText(text)).catch((err) => setError(String(err)));
              e.currentTarget.value = "";
            }}
          />
        </div>
      ) : null}

      <div className="calendar-body">
        <main className="calendar-canvas glass-panel">
          {tab === "month" ? (
            <div className="calendar-month-wrap">
              <div className="calendar-weekday-row">
                {weekDayNames.map((name) => (
                  <div key={name} className="calendar-weekday-head">
                    {name}
                  </div>
                ))}
              </div>
              <div className="calendar-month-grid">
                {monthCells.map((cell, idx) => {
                  if (!cell) {
                    return <div key={`empty-${idx}`} className="calendar-day-cell calendar-day-outside" />;
                  }
                  const k = dayKey(cell);
                  const dayOcc = occurrencesByDay.get(k) ?? [];
                  const dots = dayOcc.slice(0, 4);
                  return (
                    <button
                      key={k}
                      type="button"
                      className={`calendar-day-cell ${isToday(cell) ? "calendar-day-today" : ""} ${isSelected(cell) ? "calendar-day-selected" : ""}`}
                      onClick={() => {
                        setSelectedDay(cell);
                        setVisibleMonth(new Date(cell.getFullYear(), cell.getMonth(), 1));
                      }}
                    >
                      <span className="calendar-day-num">{cell.getDate()}</span>
                      <span className="calendar-day-dots" aria-hidden>
                        {dots.map((o) => (
                          <span key={`${o.id}-${o.occurrence_start_ts}`} className={`calendar-dot ${categoryDotClass(o.category)}`} />
                        ))}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          ) : null}

          {tab === "week" ? (
            <div className="calendar-week-wrap">
              <div className="calendar-week-columns">
                {weekDays.map((d) => {
                  const k = dayKey(d);
                  const list = occurrencesByDay.get(k) ?? [];
                  return (
                    <div key={k} className={`calendar-week-col ${isSelected(d) ? "calendar-week-col-selected" : ""}`}>
                      <button
                        type="button"
                        className={`calendar-week-col-head ${isToday(d) ? "calendar-day-today" : ""}`}
                        onClick={() => setSelectedDay(d)}
                      >
                        <span className="calendar-week-dow">{d.toLocaleDateString(undefined, { weekday: "short" })}</span>
                        <span className="calendar-week-dom">{d.getDate()}</span>
                      </button>
                      <ul className="calendar-week-events">
                        {list.map((row) => (
                          <li key={`${row.id}-${row.occurrence_start_ts}`} className="calendar-week-pill">
                            <span className={`calendar-week-pill-bar ${categoryDotClass(row.category)}`} />
                            <span className="calendar-week-pill-text">
                              {!row.all_day ? formatTimeOnly(row.occurrence_start_ts, timeFormat) : lx.calendarAllDay} · {row.title}
                            </span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  );
                })}
              </div>
            </div>
          ) : null}

          {tab === "day" ? (
            <div className="calendar-day-timeline">
              {forSelectedDay.some((r) => r.all_day) ? (
                <div className="calendar-day-allday-strip">
                  {forSelectedDay
                    .filter((r) => r.all_day)
                    .map((row) => (
                      <div key={`${row.id}-allday`} className="calendar-day-allday-chip">
                        {row.title}
                      </div>
                    ))}
                </div>
              ) : null}
              <div className="calendar-day-scroll">
                <div className="calendar-day-hours">
                  {hours.map((h) => (
                    <div key={h} className="calendar-hour-row">
                      <span className="calendar-hour-label">
                        {timeFormat === "12"
                          ? new Date(2000, 0, 1, h).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit", hour12: true })
                          : `${String(h).padStart(2, "0")}:00`}
                      </span>
                      <div className="calendar-hour-line" />
                    </div>
                  ))}
                </div>
                <div className="calendar-day-events-layer">
                  {forSelectedDay
                    .filter((row) => !row.all_day)
                    .map((row) => {
                      const start = new Date(row.occurrence_start_ts);
                      const end = new Date(row.occurrence_end_ts);
                      const startM = start.getHours() * 60 + start.getMinutes();
                      const endM = end.getHours() * 60 + end.getMinutes();
                      const topPct = (startM / (24 * 60)) * 100;
                      const heightPct = Math.max(2, ((endM - startM) / (24 * 60)) * 100);
                      return (
                        <div
                          key={`${row.id}-${row.occurrence_start_ts}`}
                          className="calendar-day-block"
                          style={{ top: `${topPct}%`, height: `${heightPct}%` }}
                        >
                          <strong>{row.title}</strong>
                          <span className="calendar-day-block-time">
                            {formatTimeOnly(row.occurrence_start_ts, timeFormat)} – {formatTimeOnly(row.occurrence_end_ts, timeFormat)}
                          </span>
                        </div>
                      );
                    })}
                </div>
              </div>
            </div>
          ) : null}

          {tab === "agenda" ? (
            <div className="calendar-agenda-wrap">
              <ul className="calendar-agenda-list">
                {occurrences.map((row) => (
                  <li key={`${row.id}-${row.occurrence_start_ts}`} className="calendar-agenda-row">
                    <div className="calendar-agenda-date">
                      <span className="calendar-agenda-dow">{new Date(row.occurrence_start_ts).toLocaleDateString(undefined, { weekday: "short" })}</span>
                      <span className="calendar-agenda-dom">{new Date(row.occurrence_start_ts).getDate()}</span>
                    </div>
                    <div className="calendar-agenda-body">
                      <div className="calendar-agenda-title">{row.title}</div>
                      <div className="calendar-agenda-meta text-muted small">
                        {row.all_day ? lx.calendarAllDay : formatDateTime(row.occurrence_start_ts, timeFormat)}
                        {row.location_label ? ` · ${row.location_label}` : ""}
                        {row.source === "holiday" ? ` · ${lx.calendarHolidayTag}` : ""}
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </main>

        <aside className="calendar-inspector glass-panel">
          <div className="calendar-inspector-toolbar">
            <div className="calendar-inspector-date">
              {selectedDay.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" })}
            </div>
            <button type="button" className="calendar-add-event-btn" onClick={openNewEventSheet}>
              + {lx.calendarNewEvent}
            </button>
          </div>
          <div className="calendar-inspector-scroll">
            {upcoming.length ? (
              <div className="calendar-inspector-section">
                <h3 className="calendar-inspector-heading">{lx.calendarUpcoming}</h3>
                <ul className="calendar-mini-list">
                  {upcoming.map((row) => (
                    <li key={`${row.id}-${row.occurrence_start_ts}`} className="calendar-mini-item">
                      <span className={`calendar-mini-dot ${categoryDotClass(row.category)}`} />
                      <div className="calendar-mini-body">
                        <div className="calendar-mini-title">{row.title}</div>
                        <div className="calendar-mini-meta text-muted small">{row.all_day
                            ? `${new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(row.occurrence_start_ts))} · ${lx.calendarAllDay}`
                            : formatDateTime(row.occurrence_start_ts, timeFormat)}</div>
                      </div>
                      {row.linked_marker_id ? (
                        <button type="button" className="secondary small-btn" onClick={() => onGoToMarker(row.linked_marker_id)}>
                          {lx.calendarMapBtn}
                        </button>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}

            <div className="calendar-inspector-section calendar-inspector-events">
              <h3 className="calendar-inspector-heading">{lx.calendarThisDay}</h3>
              <ul className="calendar-event-cards">
                {forSelectedDay.length === 0 ? (
                  <li className="calendar-empty-day text-muted">{lx.calendarNoEvents}</li>
                ) : null}
                {forSelectedDay.map((row) => (
                  <li key={`${row.id}-${row.occurrence_start_ts}`} className="calendar-event-card">
                    <div className="calendar-event-card-time">
                      {row.all_day ? lx.calendarAllDay : formatTimeOnly(row.occurrence_start_ts, timeFormat)}
                    </div>
                    <div className="calendar-event-card-main">
                      <div className="calendar-event-card-title">{row.title}</div>
                      {row.location_label ? <div className="calendar-event-card-loc text-muted small">{row.location_label}</div> : null}
                    </div>
                    <div className="calendar-event-card-actions">
                      {row.linked_marker_id ? (
                        <button type="button" className="secondary small-btn" onClick={() => onGoToMarker(row.linked_marker_id)}>
                          {lx.calendarPinBtn}
                        </button>
                      ) : null}
                      {row.source !== "holiday" ? (
                        <button type="button" className="secondary small-btn" onClick={() => void deleteEvent(row.id)}>
                          {lx.actionDelete}
                        </button>
                      ) : null}
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </aside>
      </div>

      {eventEditorOpen ? (
        <div
          className="calendar-modal-backdrop"
          role="presentation"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) closeNewEventSheet();
          }}
        >
          <div
            className="calendar-event-sheet"
            role="dialog"
            aria-modal="true"
            aria-labelledby="calendar-event-sheet-title"
            onMouseDown={(e) => e.stopPropagation()}
          >
            <div className="calendar-event-sheet-header">
              <h2 id="calendar-event-sheet-title" className="calendar-event-sheet-title">
                {lx.calendarNewEvent}
              </h2>
              <button type="button" className="calendar-event-sheet-close" aria-label={lx.actionCancel} onClick={closeNewEventSheet}>
                ×
              </button>
            </div>
            <div className="calendar-event-sheet-body calendar-form-section">
              <p className="calendar-event-sheet-hint text-muted small">{lx.calendarTemplatesHint}</p>
              <div className="calendar-template-chips">
                {templates.map((tpl) => (
                  <button
                    key={tpl.name}
                    type="button"
                    className="calendar-template-chip"
                    onClick={() => setDraft((prev) => ({ ...prev, title: tpl.title, category: tpl.category, reminder_minutes_before: tpl.reminder }))}
                  >
                    {tpl.name}
                  </button>
                ))}
              </div>
              <label className="calendar-field-label">{lx.calendarFieldTitle}</label>
              <input
                placeholder={lx.calendarFieldTitle}
                value={draft.title}
                onChange={(e) => setDraft((prev) => ({ ...prev, title: e.target.value }))}
                autoFocus
              />
              <label className="calendar-field-label">{lx.calendarFieldNotes}</label>
              <textarea
                placeholder={lx.calendarFieldNotes}
                value={draft.notes ?? ""}
                onChange={(e) => setDraft((prev) => ({ ...prev, notes: e.target.value || null }))}
                rows={3}
              />
              <div className="calendar-form-two">
                <div className="calendar-datetime-field">
                  <label className="calendar-field-label">{lx.calendarFieldStarts}</label>
                  <input
                    type="datetime-local"
                    value={new Date(draft.start_ts).toISOString().slice(0, 16)}
                    onChange={(e) => setDraft((prev) => ({ ...prev, start_ts: new Date(e.target.value).getTime() }))}
                  />
                </div>
                <div className="calendar-datetime-field">
                  <label className="calendar-field-label">{lx.calendarFieldEnds}</label>
                  <input
                    type="datetime-local"
                    value={new Date(draft.end_ts).toISOString().slice(0, 16)}
                    onChange={(e) => setDraft((prev) => ({ ...prev, end_ts: new Date(e.target.value).getTime() }))}
                  />
                </div>
              </div>
              <label className="calendar-field-label">{lx.calendarFieldLocation}</label>
              <input
                placeholder={lx.calendarFieldLocation}
                value={draft.location_label ?? ""}
                onChange={(e) => setDraft((prev) => ({ ...prev, location_label: e.target.value || null }))}
              />
              <label className="calendar-field-label">{lx.calendarFieldMapPin}</label>
              <select value={draft.linked_marker_id ?? ""} onChange={(e) => setDraft((prev) => ({ ...prev, linked_marker_id: e.target.value || null }))}>
                <option value="">{lx.calendarPinNone}</option>
                {markers.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.title}
                  </option>
                ))}
              </select>
              {markers.length === 0 ? (
                <p className="text-muted small" style={{ margin: "0.35rem 0 0" }}>
                  {lx.calendarNoPinsHint}
                </p>
              ) : null}
              {markers.length === 0 ? (
                <button type="button" className="secondary" style={{ marginTop: "0.35rem", alignSelf: "flex-start" }} onClick={onOpenMapView}>
                  {lx.calendarOpenMapPins}
                </button>
              ) : null}
              <label className="calendar-field-label">{lx.calendarFieldParticipants}</label>
              <input
                placeholder={lx.calendarParticipantsPlaceholder}
                value={(draft.participant_names ?? []).join(", ")}
                onChange={(e) =>
                  setDraft((prev) => ({
                    ...prev,
                    participant_names: e.target.value.split(",").map((s) => s.trim()).filter(Boolean),
                  }))
                }
              />
              <div className="calendar-form-row-inline">
                <div>
                  <label className="calendar-field-label">{lx.calendarFieldRepeat}</label>
                  <select value={draft.recurrence_rule ?? ""} onChange={(e) => setDraft((prev) => ({ ...prev, recurrence_rule: e.target.value || null }))}>
                    <option value="">{lx.calendarRepeatNone}</option>
                    <option value="daily">{lx.calendarRepeatDaily}</option>
                    <option value="weekly">{lx.calendarRepeatWeekly}</option>
                    <option value="monthly">{lx.calendarRepeatMonthly}</option>
                    <option value="yearly">{lx.calendarRepeatYearly}</option>
                  </select>
                </div>
                <div>
                  <label className="calendar-field-label">{lx.calendarFieldAlert}</label>
                  <select
                    value={String(draft.reminder_minutes_before ?? "")}
                    onChange={(e) => setDraft((prev) => ({ ...prev, reminder_minutes_before: e.target.value ? Number.parseInt(e.target.value, 10) : null }))}
                  >
                    <option value="">{lx.calendarAlertNone}</option>
                    <option value="10">{lx.calendarAlert10}</option>
                    <option value="30">{lx.calendarAlert30}</option>
                    <option value="60">{lx.calendarAlert60}</option>
                    <option value="1440">{lx.calendarAlert1440}</option>
                  </select>
                </div>
              </div>
            </div>
            <div className="calendar-event-sheet-footer">
              <button type="button" className="secondary" onClick={closeNewEventSheet}>
                {lx.actionCancel}
              </button>
              <button type="button" className="calendar-save-btn calendar-save-btn-inline" onClick={() => void saveDraft()} disabled={!draft.title.trim()}>
                {lx.calendarAddToCalendar}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {notices.length ? (
        <div className="calendar-notice-stack">
          {notices.map((n, idx) => (
            <div key={`${n}-${idx}`} className="calendar-notice">
              {n}
            </div>
          ))}
        </div>
      ) : null}
      {error ? <p className="error calendar-error-banner">{error}</p> : null}
    </div>
  );
}
