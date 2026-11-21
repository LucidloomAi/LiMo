// Spans of time and times of day in the guardian's screens: the one place
// they are written, by the phone engine's rules (phone/engine/src/words.rs),
// so a guardian reads the same words the child does.
//
//   time left     "1 h 12 min", "2 h", "14 min", "0:42" under a minute;
//                 minutes rounded up, so it never says less is left than is
//   time used     "48 min", "1 h", "1 h 5 min"; minutes rounded down, so the
//                 two add up to the day
//   in a sentence "30 min", "1 hour", "1 hour 30 min", "2 hours"
//   a time        "9:42 pm" or "21:42", on the PC's own clock
//
// The service sends every time as a number (seconds, or unix seconds); only
// this module turns one into words. One difference from the engine, from the
// approved drawing: no time left at all is "0 min", where a child's own
// countdown says "0:00".

import type { DailyBudget, Weekday } from '../guardianSeam';

/** How a screen shows times: the zone's distance from UTC, and 12 or 24 hours. */
export type Clock = { readonly utcOffsetSecs: number; readonly hours24: boolean };

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
const WEEKDAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];

/** Whole minutes and hours of a span of minutes, as the engine joins them: "1 h 5 min". */
function hoursAndMinutes(m: number): string {
  const h = Math.floor(m / 60);
  const rest = m % 60;
  if (h === 0) return `${rest} min`;
  if (rest === 0) return `${h} h`;
  return `${h} h ${rest} min`;
}

/** The time left, as it is shown large: rounded up to the minute; "0:42" under a minute; "0 min" with none. */
export function timeLeft(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  if (s === 0) return '0 min';
  if (s < 60) return `0:${String(s).padStart(2, '0')}`;
  return hoursAndMinutes(Math.ceil(s / 60));
}

/** A span of time used, rounded down to the minute: "0 min", "48 min", "2 h", "1 h 5 min". */
export function usedSpan(secs: number): string {
  return hoursAndMinutes(Math.floor(Math.max(0, secs) / 60));
}

/** A number of minutes as a figure, such as a day's time: "2 h", "1 h 30 min". */
export function minutesFigure(minutes: number): string {
  return hoursAndMinutes(Math.max(0, Math.round(minutes)));
}

/** A span of minutes inside a sentence: "30 min", "1 hour", "1 hour 30 min", "2 hours". */
export function minutesInSentence(minutes: number): string {
  const m = Math.max(0, Math.round(minutes));
  const h = Math.floor(m / 60);
  const rest = m % 60;
  if (h === 0) return `${rest} min`;
  const hours = h === 1 ? '1 hour' : `${h} hours`;
  return rest === 0 ? hours : `${hours} ${rest} min`;
}

/**
 * The line under a child's bar, and how much of the bar is used: "48 min
 * used of 2 h" with the part from 0 to 1 in thousandths; "48 min used today"
 * with no limit (no bar), and on a day with no time at all (a full bar).
 */
export function usedLine(usedSecs: number, budgetSecs: number | null): { words: string; part: number | null } {
  const used = usedSpan(usedSecs);
  if (budgetSecs === null) return { words: `${used} used today`, part: null };
  if (budgetSecs <= 0) return { words: `${used} used today`, part: 1 };
  const part = Math.min(1, usedSecs / budgetSecs);
  return { words: `${used} used of ${usedSpan(budgetSecs)}`, part: Math.round(part * 1000) / 1000 };
}

/**
 * The time left as it is shown large, as the engine says it: "1 h 12 min";
 * "No limit today"; or "Open until 9:30 pm" while a guardian's unlock is in
 * force (one that has ended is not said).
 */
export function leftWords(
  leftSecs: number | null,
  unlockedUntil: number | null,
  now: number,
  clock: Clock,
): { kind: 'left' | 'no-limit' | 'open'; words: string } {
  if (unlockedUntil !== null && unlockedUntil > now) return { kind: 'open', words: `Open until ${untilInSentence(now, unlockedUntil, clock)}` };
  if (leftSecs === null) return { kind: 'no-limit', words: 'No limit today' };
  return { kind: 'left', words: timeLeft(leftSecs) };
}

/** A moment on the screen's clock: its date parts and time of day. */
function local(at: number, clock: Clock) {
  const d = new Date((at + clock.utcOffsetSecs) * 1000);
  return {
    year: d.getUTCFullYear(),
    month: d.getUTCMonth(),
    date: d.getUTCDate(),
    weekday: d.getUTCDay(),
    hour: d.getUTCHours(),
    minute: d.getUTCMinutes(),
    /** Days since 1 January 1970 on this clock. */
    day: Math.floor((at + clock.utcOffsetSecs) / 86_400),
  };
}

/** A time of day from its hour and minute: "9:42 pm" or "21:42". */
export function clockText(hour: number, minute: number, clock: Clock): string {
  const mm = String(minute).padStart(2, '0');
  if (clock.hours24) return `${String(hour).padStart(2, '0')}:${mm}`;
  const half = hour < 12 ? 'am' : 'pm';
  const h = hour % 12 === 0 ? 12 : hour % 12;
  return `${h}:${mm} ${half}`;
}

/** A minute of the day (0 to 1440) as a time: 1260 is "9:00 pm"; 1440, the midnight at the day's end, "12:00 am". */
export function minuteOfDay(minute: number, clock: Clock): string {
  const m = ((Math.round(minute) % 1440) + 1440) % 1440;
  return clockText(Math.floor(m / 60), m % 60, clock);
}

/** The time of day of a moment: "9:42 pm" or "21:42". Seconds are dropped. */
export function timeOfDay(at: number, clock: Clock): string {
  const t = local(at, clock);
  return clockText(t.hour, t.minute, clock);
}

/** A moment rounded up to the next whole minute, so that a time is never said earlier than it is. */
function upToMinute(at: number): number {
  const over = ((at % 60) + 60) % 60;
  return over === 0 ? at : at + (60 - over);
}

/** "Friday 27 November". */
function dayAndMonth(t: ReturnType<typeof local>): string {
  return `${WEEKDAYS[t.weekday]} ${t.date} ${MONTHS[t.month]}`;
}

/**
 * A later moment inside a sentence: "9:47 pm" today, else "tomorrow, 7:00
 * am", "Friday, 7:00 am", or "Friday 4 December, 7:00 am" a week or more
 * on. Rounded up to the minute.
 */
export function untilInSentence(now: number, at: number, clock: Clock): string {
  const up = upToMinute(at);
  const time = timeOfDay(up, clock);
  const then = local(up, clock);
  const days = then.day - local(now, clock).day;
  if (up <= now || days <= 0) return time;
  if (days === 1) return `tomorrow, ${time}`;
  if (days <= 6) return `${WEEKDAYS[then.weekday]}, ${time}`;
  return `${dayAndMonth(then)}, ${time}`;
}

/** Today's date at the top of Today: "Saturday 21 November". */
export function dateLine(now: number, clock: Clock): string {
  return dayAndMonth(local(now, clock));
}

/**
 * When a device was last heard, said from now: "3 min ago", "1 h 5 min ago"
 * within a day, else "on Friday 20 November".
 */
export function heardAgo(now: number, at: number, clock: Clock): string {
  const secs = Math.max(0, now - at);
  if (secs < 86_400) return `${usedSpan(secs)} ago`;
  return `on ${dayAndMonth(local(at, clock))}`;
}

const WEEK: readonly Weekday[] = ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday'];
const DAY_NAMES = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];

/** "a", "a and b", "a, b and c". */
function andList(items: readonly string[]): string {
  if (items.length <= 1) return items[0] ?? '';
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`;
}

/** The days of a rule when it is not every day: "from Monday to Friday" for three or more in a row, else "on Saturday and Sunday". */
function daysWords(days: readonly Weekday[]): string | null {
  const on = WEEK.map((_, i) => i).filter((i) => days.includes(WEEK[i]));
  if (on.length === 0 || on.length === 7) return null;
  const has = (i: number) => on.includes(i % 7);
  const first = [0, 1, 2, 3, 4, 5, 6].find((i) => has(i) && !has(i + 6));
  const run = first !== undefined && on.every((_, k) => has(first + k));
  if (run && on.length >= 3) return `from ${DAY_NAMES[first!]} to ${DAY_NAMES[(first! + on.length - 1) % 7]}`;
  return `on ${andList(on.map((i) => DAY_NAMES[i]))}`;
}

/**
 * The time a day, as the engine says it: "2 h", "No limit", "No screen
 * time", or each with its days when the days differ, in the week's order:
 * "1 h 30 min from Monday to Thursday; 2 h on Friday; 3 h on Saturday and
 * Sunday".
 */
export function timeADayWords(budgets: Readonly<Record<Weekday, DailyBudget>>): string {
  const groups: { key: string; budget: DailyBudget; days: Weekday[] }[] = [];
  for (const day of WEEK) {
    const b = budgets[day];
    const key = b === 'no_limit' ? 'no_limit' : String(b.minutes);
    const g = groups.find((x) => x.key === key);
    if (g) g.days.push(day);
    else groups.push({ key, budget: b, days: [day] });
  }
  const parts = groups.map(({ budget, days }) => {
    const time = budget === 'no_limit' ? 'no limit' : budget.minutes === 0 ? 'no screen time' : minutesFigure(budget.minutes);
    const when = daysWords(days);
    return when ? `${time} ${when}` : time;
  });
  const said = parts.join('; ');
  return said.charAt(0).toUpperCase() + said.slice(1);
}

/** The clock of the PC (or the phone) the screens run on: its zone now, and whether it shows 24 hours. */
export function clockHere(): Clock {
  const utcOffsetSecs = -new Date().getTimezoneOffset() * 60;
  let hours24 = false;
  try {
    const cycle = new Intl.DateTimeFormat(undefined, { hour: 'numeric' }).resolvedOptions().hourCycle;
    hours24 = cycle === 'h23' || cycle === 'h24';
  } catch {
    hours24 = false;
  }
  return { utcOffsetSecs, hours24 };
}
