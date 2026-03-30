export type HolidayEvent = {
  id: string;
  title: string;
  start_ts: number;
  end_ts: number;
  all_day: true;
  source: "holiday";
  category: string;
  notes: string | null;
};

function dayStartMs(year: number, month1: number, day: number): number {
  return new Date(year, month1 - 1, day, 0, 0, 0, 0).getTime();
}

function allDayRange(year: number, month1: number, day: number): [number, number] {
  const start = dayStartMs(year, month1, day);
  const end = start + 24 * 60 * 60 * 1000 - 1;
  return [start, end];
}

function nthWeekdayOfMonth(year: number, month1: number, weekday: number, nth: number): number {
  const first = new Date(year, month1 - 1, 1);
  const firstDow = first.getDay();
  const offset = (weekday - firstDow + 7) % 7;
  return 1 + offset + (nth - 1) * 7;
}

function lastWeekdayOfMonth(year: number, month1: number, weekday: number): number {
  const last = new Date(year, month1, 0);
  const lastDow = last.getDay();
  const offset = (lastDow - weekday + 7) % 7;
  return last.getDate() - offset;
}

function addHoliday(out: HolidayEvent[], year: number, month1: number, day: number, title: string, region: string) {
  const [start, end] = allDayRange(year, month1, day);
  out.push({
    id: `holiday-${region}-${year}-${month1}-${day}-${title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
    title,
    start_ts: start,
    end_ts: end,
    all_day: true,
    source: "holiday",
    category: "holiday",
    notes: null,
  });
}

function buildUs(year: number): HolidayEvent[] {
  const out: HolidayEvent[] = [];
  addHoliday(out, year, 1, 1, "New Year's Day", "US");
  addHoliday(out, year, 7, 4, "Independence Day", "US");
  addHoliday(out, year, 11, 11, "Veterans Day", "US");
  addHoliday(out, year, 12, 25, "Christmas Day", "US");
  addHoliday(out, year, 6, 19, "Juneteenth", "US");
  addHoliday(out, year, 5, lastWeekdayOfMonth(year, 5, 1), "Memorial Day", "US");
  addHoliday(out, year, 9, nthWeekdayOfMonth(year, 9, 1, 1), "Labor Day", "US");
  addHoliday(out, year, 11, nthWeekdayOfMonth(year, 11, 4, 4), "Thanksgiving", "US");
  return out;
}

function buildCanada(year: number): HolidayEvent[] {
  const out: HolidayEvent[] = [];
  addHoliday(out, year, 1, 1, "New Year's Day", "CA");
  addHoliday(out, year, 7, 1, "Canada Day", "CA");
  addHoliday(out, year, 11, 11, "Remembrance Day", "CA");
  addHoliday(out, year, 12, 25, "Christmas Day", "CA");
  addHoliday(out, year, 12, 26, "Boxing Day", "CA");
  addHoliday(out, year, 9, nthWeekdayOfMonth(year, 9, 1, 1), "Labour Day", "CA");
  addHoliday(out, year, 10, nthWeekdayOfMonth(year, 10, 1, 2), "Thanksgiving", "CA");
  return out;
}

function buildUk(year: number): HolidayEvent[] {
  const out: HolidayEvent[] = [];
  addHoliday(out, year, 1, 1, "New Year's Day", "GB");
  addHoliday(out, year, 12, 25, "Christmas Day", "GB");
  addHoliday(out, year, 12, 26, "Boxing Day", "GB");
  addHoliday(out, year, 5, nthWeekdayOfMonth(year, 5, 1, 1), "Early May Bank Holiday", "GB");
  addHoliday(out, year, 5, lastWeekdayOfMonth(year, 5, 1), "Spring Bank Holiday", "GB");
  addHoliday(out, year, 8, lastWeekdayOfMonth(year, 8, 1), "Summer Bank Holiday", "GB");
  return out;
}

function buildAu(year: number): HolidayEvent[] {
  const out: HolidayEvent[] = [];
  addHoliday(out, year, 1, 1, "New Year's Day", "AU");
  addHoliday(out, year, 1, 26, "Australia Day", "AU");
  addHoliday(out, year, 4, 25, "ANZAC Day", "AU");
  addHoliday(out, year, 12, 25, "Christmas Day", "AU");
  addHoliday(out, year, 12, 26, "Boxing Day", "AU");
  addHoliday(out, year, 6, nthWeekdayOfMonth(year, 6, 1, 2), "King's Birthday", "AU");
  return out;
}

export function getRegionalHolidays(year: number, regionRaw: string | null | undefined): HolidayEvent[] {
  const region = (regionRaw ?? "US").trim().toUpperCase();
  if (region.startsWith("CA")) return buildCanada(year);
  if (region.startsWith("GB") || region.startsWith("UK")) return buildUk(year);
  if (region.startsWith("AU")) return buildAu(year);
  return buildUs(year);
}
