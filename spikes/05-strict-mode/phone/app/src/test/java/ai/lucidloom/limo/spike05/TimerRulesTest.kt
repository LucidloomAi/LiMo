package ai.lucidloom.limo.spike05

import ai.lucidloom.limo.spike05.TimerRules.Change
import ai.lucidloom.limo.spike05.TimerRules.Clocks
import ai.lucidloom.limo.spike05.TimerRules.Decision
import ai.lucidloom.limo.spike05.TimerRules.MAX_MS
import ai.lucidloom.limo.spike05.TimerRules.MINUTE_MS
import ai.lucidloom.limo.spike05.TimerRules.Record
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TimerRulesTest {

    private val wall0 = 1_800_000_000_000L
    private val t0 = Clocks(wallMs = wall0, elapsedMs = 50_000L, bootCount = 7)

    /** The same boot, [ms] later on both clocks. */
    private fun later(c: Clocks, ms: Long) = Clocks(c.wallMs + ms, c.elapsedMs + ms, c.bootCount)

    private fun keepLeft(d: Decision): Long {
        assertTrue("expected Keep, got $d", d is Decision.Keep)
        return (d as Decision.Keep).leftMs
    }

    private fun releaseReason(d: Decision): String {
        assertTrue("expected Release, got $d", d is Decision.Release)
        return (d as Decision.Release).reason
    }

    // starting

    @Test
    fun becomingOwnerStartsThirtyMinutes() {
        val d = TimerRules.decide(isAdmin = true, isOwner = true, r = Record.NONE, now = t0)
        assertTrue(d is Decision.Start)
        val r = (d as Decision.Start).record
        assertEquals(30 * MINUTE_MS, TimerRules.leftMs(r, t0))
        assertEquals(wall0 + 30 * MINUTE_MS, r.wallMs)
    }

    @Test
    fun notAdminForgetsOrIdles() {
        val r = TimerRules.startIn(t0, 30 * MINUTE_MS)
        assertEquals(Decision.Forget, TimerRules.decide(false, false, r, t0))
        assertEquals(Decision.Idle, TimerRules.decide(false, false, Record.NONE, t0))
    }

    @Test
    fun adminWithoutOwnerStillHasATimer() {
        assertTrue(TimerRules.decide(isAdmin = true, isOwner = false, r = Record.NONE, now = t0) is Decision.Start)
    }

    // running down and firing

    @Test
    fun countsDownAndFiresAtTheDeadline() {
        val r = TimerRules.startIn(t0, 30 * MINUTE_MS)
        assertEquals(20 * MINUTE_MS, keepLeft(TimerRules.decide(true, true, r, later(t0, 10 * MINUTE_MS))))
        assertEquals(1L, keepLeft(TimerRules.decide(true, true, r, later(t0, 30 * MINUTE_MS - 1L))))
        assertEquals("deadline_passed", releaseReason(TimerRules.decide(true, true, r, later(t0, 30 * MINUTE_MS))))
        assertEquals("deadline_passed", releaseReason(TimerRules.decide(true, true, r, later(t0, 5 * 60 * MINUTE_MS))))
    }

    @Test
    fun clockMovedForwardReleases() {
        val r = TimerRules.startIn(t0, 30 * MINUTE_MS)
        // One minute of real time, but the clock was set 40 minutes ahead.
        val now = Clocks(t0.wallMs + 41 * MINUTE_MS, t0.elapsedMs + MINUTE_MS, t0.bootCount)
        assertEquals("clock_moved_forward", releaseReason(TimerRules.decide(true, true, r, now)))
    }

    @Test
    fun clockMovedBackCannotExtend() {
        val r = TimerRules.startIn(t0, 30 * MINUTE_MS)
        // Ten minutes of real time, with the clock set back by an hour.
        val now = Clocks(t0.wallMs + 10 * MINUTE_MS - 60 * MINUTE_MS, t0.elapsedMs + 10 * MINUTE_MS, t0.bootCount)
        assertEquals(20 * MINUTE_MS, keepLeft(TimerRules.decide(true, true, r, now)))
        assertEquals(20 * MINUTE_MS, TimerRules.leftMs(r, now))
    }

    @Test
    fun clockMovedFarBackReleases() {
        val r = TimerRules.startIn(t0, 30 * MINUTE_MS)
        // Set back by 100 minutes: the wall deadline now looks 130 minutes away.
        val now = Clocks(t0.wallMs - 100 * MINUTE_MS, t0.elapsedMs, t0.bootCount)
        assertEquals("deadline_too_far", releaseReason(TimerRules.decide(true, true, r, now)))
    }

    @Test
    fun aDamagedRecordFarAheadReleases() {
        val r = Record(t0.wallMs + MAX_MS + 1L, t0.elapsedMs + 10 * MINUTE_MS, t0.bootCount, t0.elapsedMs)
        assertEquals("deadline_too_far", releaseReason(TimerRules.decide(true, true, r, t0)))
        val r2 = Record(t0.wallMs + 10 * MINUTE_MS, t0.elapsedMs + MAX_MS + 1L, t0.bootCount, t0.elapsedMs)
        assertEquals("deadline_too_far", releaseReason(TimerRules.decide(true, true, r2, t0)))
    }

    // restarts

    @Test
    fun survivesARestartAndMovesToTheNewBoot() {
        val r0 = TimerRules.startIn(t0, 30 * MINUTE_MS)
        // A check 10 minutes in, then the phone is restarted.
        val checked = (TimerRules.decide(true, true, r0, later(t0, 10 * MINUTE_MS)) as Decision.Keep).record
        // Off for 2 minutes; the new boot's elapsed clock is at 1 minute.
        val afterBoot = Clocks(t0.wallMs + 13 * MINUTE_MS, MINUTE_MS, t0.bootCount + 1)
        val d = TimerRules.decide(true, true, checked, afterBoot)
        // Wall clock says 17 minutes; the elapsed reckoning says 20 minus 1.
        assertEquals(17 * MINUTE_MS, keepLeft(d))
        val moved = (d as Decision.Keep).record
        assertEquals(afterBoot.bootCount, moved.bootCount)
        assertEquals(afterBoot.elapsedMs + 17 * MINUTE_MS, moved.elapsedMs)
        // And it counts down on the new boot from there.
        assertEquals(7 * MINUTE_MS, keepLeft(TimerRules.decide(true, true, moved, later(afterBoot, 10 * MINUTE_MS))))
    }

    @Test
    fun clockMovedBackDuringARestartGainsLittle() {
        val r0 = TimerRules.startIn(t0, 30 * MINUTE_MS)
        val checked = (TimerRules.decide(true, true, r0, later(t0, 10 * MINUTE_MS)) as Decision.Keep).record
        // While off, the clock was set back 30 minutes. 5 minutes since boot.
        val afterBoot = Clocks(t0.wallMs + 10 * MINUTE_MS - 30 * MINUTE_MS, 5 * MINUTE_MS, t0.bootCount + 1)
        // 20 left at the last check, minus 5 since boot: 15, not the wall clock's 50.
        assertEquals(15 * MINUTE_MS, keepLeft(TimerRules.decide(true, true, checked, afterBoot)))
    }

    @Test
    fun deadlinePassedWhileOffReleasesAtBoot() {
        val r0 = TimerRules.startIn(t0, 30 * MINUTE_MS)
        val afterBoot = Clocks(t0.wallMs + 45 * MINUTE_MS, MINUTE_MS, t0.bootCount + 1)
        assertEquals("deadline_passed", releaseReason(TimerRules.decide(true, true, r0, afterBoot)))
    }

    @Test
    fun unknownBootCountUsesTheElapsedClockGoingBack() {
        val start = Clocks(wall0, 600_000L, -1)
        val r0 = TimerRules.startIn(start, 30 * MINUTE_MS)
        assertTrue(!TimerRules.isNewBoot(r0, later(start, MINUTE_MS)))
        assertTrue(TimerRules.isNewBoot(r0, Clocks(wall0 + 2 * MINUTE_MS, 1_000L, -1)))
    }

    // admin enabled again

    @Test
    fun enablingAgainNeverExtends() {
        val r = TimerRules.startIn(t0, 30 * MINUTE_MS)
        val now = later(t0, 20 * MINUTE_MS)
        assertEquals(r, TimerRules.onAdminEnabled(r, now))
        assertEquals(10 * MINUTE_MS, TimerRules.leftMs(TimerRules.onAdminEnabled(r, now), now))
    }

    @Test
    fun enablingWithAnOldPassedDeadlineStartsFresh() {
        val r = TimerRules.startIn(t0, 30 * MINUTE_MS)
        val now = later(t0, 3 * 60 * MINUTE_MS)
        assertEquals(30 * MINUTE_MS, TimerRules.leftMs(TimerRules.onAdminEnabled(r, now), now))
        assertEquals(30 * MINUTE_MS, TimerRules.leftMs(TimerRules.onAdminEnabled(Record.NONE, now), now))
    }

    // lowering and extending

    @Test
    fun lowersOnlyDownward() {
        val r = TimerRules.startIn(t0, 30 * MINUTE_MS)
        val done = TimerRules.lowerTo(r, t0, 2)
        assertTrue(done is Change.Done)
        assertEquals(2 * MINUTE_MS, TimerRules.leftMs((done as Change.Done).record, t0))
        assertEquals(Change.Refused("not_lower"), TimerRules.lowerTo(r, t0, 30))
        assertEquals(Change.Refused("not_lower"), TimerRules.lowerTo(r, t0, 45))
        assertEquals(Change.Refused("out_of_range"), TimerRules.lowerTo(r, t0, 121))
        assertEquals(Change.Refused("out_of_range"), TimerRules.lowerTo(r, t0, -1))
        assertEquals(Change.Refused("no_deadline"), TimerRules.lowerTo(Record.NONE, t0, 2))
        // Zero means "release at the next check".
        val zero = TimerRules.lowerTo(r, t0, 0) as Change.Done
        assertEquals("deadline_passed", releaseReason(TimerRules.decide(true, true, zero.record, t0)))
    }

    @Test
    fun extendsByThirtyButNeverBeyondOneHundredTwenty() {
        var r = TimerRules.startIn(t0, 30 * MINUTE_MS)
        r = (TimerRules.extend(r, t0) as Change.Done).record
        assertEquals(60 * MINUTE_MS, TimerRules.leftMs(r, t0))
        r = (TimerRules.extend(r, t0) as Change.Done).record
        r = (TimerRules.extend(r, t0) as Change.Done).record
        assertEquals(MAX_MS, TimerRules.leftMs(r, t0))
        assertEquals(Change.Refused("at_most"), TimerRules.extend(r, t0))
        // From 100 minutes left, extending gives 120, not 130.
        val r100 = TimerRules.startIn(t0, 100 * MINUTE_MS)
        assertEquals(MAX_MS, TimerRules.leftMs((TimerRules.extend(r100, t0) as Change.Done).record, t0))
        assertEquals(Change.Refused("no_deadline"), TimerRules.extend(Record.NONE, t0))
        val due = TimerRules.startIn(t0, 0L)
        assertEquals(Change.Refused("already_due"), TimerRules.extend(due, t0))
    }

    @Test
    fun neverStartsMoreThanOneHundredTwentyAhead() {
        val r = TimerRules.startIn(t0, 500 * MINUTE_MS)
        assertEquals(MAX_MS, TimerRules.leftMs(r, t0))
        val negative = TimerRules.startIn(t0, -5L)
        assertEquals(0L, TimerRules.leftMs(negative, t0))
    }

    // a second release right after the first

    @Test
    fun aReleaseThatJustRanStopsAnotherOneButNotTheRetry() {
        val end = 1_000_000L
        assertFalse(TimerRules.releaseJustRan(null, end))
        // The second alarm, 15 ms after the first release ended (as on the phone).
        assertTrue(TimerRules.releaseJustRan(end, end + 15L))
        // "admin disabled" arriving 5 seconds later.
        assertTrue(TimerRules.releaseJustRan(end, end + 5_000L))
        assertFalse(TimerRules.releaseJustRan(end, end + TimerRules.RELEASE_QUIET_MS))
        // The retry of a release that did not finish is not held back.
        assertFalse(TimerRules.releaseJustRan(end, end + TimerRules.RETRY_MS))
        // A clock that went backwards (a new process after a restart) never blocks.
        assertFalse(TimerRules.releaseJustRan(end, end - 1L))
    }

    // text

    @Test
    fun minutesAndTimeLeftText() {
        assertEquals("none", TimerRules.minutesText(null))
        assertEquals("30", TimerRules.minutesText(30 * MINUTE_MS))
        assertEquals("29", TimerRules.minutesText(28 * MINUTE_MS + 1L))
        assertEquals("0", TimerRules.minutesText(0L))
        assertEquals("0", TimerRules.minutesText(-5L))
        assertEquals("28 min 41 s", TimerRules.formatLeft(28 * MINUTE_MS + 41_000L))
        assertEquals("0 min 1 s", TimerRules.formatLeft(1L))
        assertEquals("due now", TimerRules.formatLeft(0L))
    }
}
