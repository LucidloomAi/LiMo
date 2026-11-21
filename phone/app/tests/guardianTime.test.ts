// The guardian screens' one module for spans of time and times of day
// (src/guardian/time.ts), against the phone engine's own cases in
// phone/engine/src/words.rs, so a guardian reads what the child reads.

import { WEEKDAYS, type DailyBudget, type Weekday } from '../src/guardianSeam';
import {
  clockText,
  dateLine,
  heardAgo,
  leftWords,
  minuteOfDay,
  minutesFigure,
  minutesInSentence,
  timeADayWords,
  timeLeft,
  timeOfDay,
  untilInSentence,
  usedLine,
  usedSpan,
} from '../src/guardian/time';

const H12 = { utcOffsetSecs: 0, hours24: false };
const H24 = { utcOffsetSecs: 0, hours24: true };
/** Wednesday 18 November 2026, 00:00 UTC: the engine's tests' own day. */
const WED = 1_794_960_000;
const at = (day: number, hour: number, minute: number) => WED + day * 86_400 + hour * 3_600 + minute * 60;

describe('spans of time, as the engine says them', () => {
  it('the time left: minutes rounded up, "0:42" under a minute', () => {
    expect(timeLeft(4_261)).toBe('1 h 12 min');
    expect(timeLeft(4_320)).toBe('1 h 12 min');
    expect(timeLeft(7_200)).toBe('2 h');
    expect(timeLeft(14 * 60)).toBe('14 min');
    expect(timeLeft(42)).toBe('0:42');
    expect(timeLeft(5)).toBe('0:05');
  });

  it('no time left at all is "0 min", as the drawing says it (the child’s countdown says "0:00")', () => {
    expect(timeLeft(0)).toBe('0 min');
  });

  it('time used: minutes rounded down', () => {
    expect(usedSpan(0)).toBe('0 min');
    expect(usedSpan(59)).toBe('0 min');
    expect(usedSpan(48 * 60 + 59)).toBe('48 min');
    expect(usedSpan(7_200)).toBe('2 h');
    expect(usedSpan(3_900)).toBe('1 h 5 min');
  });

  it('the line under the bar, and how much of the bar is used', () => {
    expect(usedLine(48 * 60, 7_200)).toEqual({ words: '48 min used of 2 h', part: 0.4 });
    expect(usedLine(31 * 60 + 59, 7_200)).toEqual({ words: '31 min used of 2 h', part: 0.267 });
    expect(usedLine(130 * 60, 7_200)).toEqual({ words: '2 h 10 min used of 2 h', part: 1 });
    expect(usedLine(48 * 60, null)).toEqual({ words: '48 min used today', part: null });
    expect(usedLine(0, 0)).toEqual({ words: '0 min used today', part: 1 });
    expect(usedLine(0, 5_400).part).toBe(0);
    expect(usedLine(3_600, 3_600)).toEqual({ words: '1 h used of 1 h', part: 1 });
  });

  it('a span inside a sentence', () => {
    expect(minutesInSentence(15)).toBe('15 min');
    expect(minutesInSentence(30)).toBe('30 min');
    expect(minutesInSentence(60)).toBe('1 hour');
    expect(minutesInSentence(90)).toBe('1 hour 30 min');
    expect(minutesInSentence(120)).toBe('2 hours');
    expect(minutesInSentence(150)).toBe('2 hours 30 min');
  });

  it('a day’s time as a figure', () => {
    expect(minutesFigure(120)).toBe('2 h');
    expect(minutesFigure(90)).toBe('1 h 30 min');
    expect(minutesFigure(0)).toBe('0 min');
  });

  it('the time left as it is shown large: the time, no limit, or open until a time', () => {
    const now = at(0, 16, 0);
    expect(leftWords(4_320, null, now, H12)).toEqual({ kind: 'left', words: '1 h 12 min' });
    expect(leftWords(null, null, now, H12)).toEqual({ kind: 'no-limit', words: 'No limit today' });
    expect(leftWords(0, at(0, 21, 30), now, H12)).toEqual({ kind: 'open', words: 'Open until 9:30 pm' });
    expect(leftWords(0, at(0, 21, 30), now, H24).words).toBe('Open until 21:30');
    // An unlock that has ended is not said.
    expect(leftWords(60, now - 1, now, H12).kind).toBe('left');
  });

  it('the time a day, as the engine says it in How it works', () => {
    const every = (b: DailyBudget) => Object.fromEntries(WEEKDAYS.map((d) => [d, b])) as Record<Weekday, DailyBudget>;
    expect(timeADayWords(every({ minutes: 120 }))).toBe('2 h');
    expect(timeADayWords(every('no_limit'))).toBe('No limit');
    expect(timeADayWords(every({ minutes: 90 }))).toBe('1 h 30 min');
    const week = { ...every({ minutes: 90 }), saturday: { minutes: 180 }, sunday: { minutes: 180 } };
    expect(timeADayWords(week)).toBe('1 h 30 min from Monday to Friday; 3 h on Saturday and Sunday');
    expect(timeADayWords({ ...week, sunday: 'no_limit', wednesday: { minutes: 0 } })).toBe(
      '1 h 30 min on Monday, Tuesday, Thursday and Friday; no screen time on Wednesday; 3 h on Saturday; no limit on Sunday',
    );
  });
});

describe('times of day, on the screen’s own clock', () => {
  it('12 or 24 hours, in the clock’s zone', () => {
    expect(timeOfDay(at(0, 21, 42), H12)).toBe('9:42 pm');
    expect(timeOfDay(at(0, 21, 42), H24)).toBe('21:42');
    expect(timeOfDay(at(0, 0, 0), H12)).toBe('12:00 am');
    expect(timeOfDay(at(0, 12, 0), H12)).toBe('12:00 pm');
    expect(timeOfDay(at(0, 7, 5), H24)).toBe('07:05');
    expect(timeOfDay(at(0, 23, 30), { utcOffsetSecs: 19_800, hours24: false })).toBe('5:00 am');
    expect(clockText(18, 17, H12)).toBe('6:17 pm');
  });

  it('a minute of the day, for bedtime', () => {
    expect(minuteOfDay(21 * 60, H12)).toBe('9:00 pm');
    expect(minuteOfDay(7 * 60, H12)).toBe('7:00 am');
    expect(minuteOfDay(0, H12)).toBe('12:00 am');
    expect(minuteOfDay(1_440, H24)).toBe('00:00');
  });

  it('a later moment in a sentence: today, tomorrow, a weekday, a date; rounded up to the minute', () => {
    const now = at(0, 21, 42);
    expect(untilInSentence(now, at(0, 21, 47), H12)).toBe('9:47 pm');
    expect(untilInSentence(now, at(0, 21, 46) + 1, H12)).toBe('9:47 pm');
    expect(untilInSentence(now, at(1, 7, 0), H24)).toBe('tomorrow, 07:00');
    expect(untilInSentence(now, at(2, 7, 0), H12)).toBe('Friday, 7:00 am');
    expect(untilInSentence(now, at(9, 7, 0), H12)).toBe('Friday 27 November, 7:00 am');
  });

  it('today’s date, and when a device was last heard', () => {
    expect(dateLine(at(3, 18, 12), H12)).toBe('Saturday 21 November');
    expect(heardAgo(at(0, 18, 12), at(0, 18, 9), H12)).toBe('3 min ago');
    expect(heardAgo(at(0, 18, 12), at(0, 17, 7), H12)).toBe('1 h 5 min ago');
    expect(heardAgo(at(3, 18, 12), at(1, 9, 0), H12)).toBe('on Thursday 19 November');
  });
});
