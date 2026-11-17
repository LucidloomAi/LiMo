package ai.lucidloom.limo.spike05

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HoldSetTest {

    private class Counter {
        var finished = 0
        val finisher: () -> Unit = { finished += 1 }
    }

    @Test
    fun heldBroadcastsFinishWhenTheirWorkIsDone() {
        val set = HoldSet()
        val a = Counter()
        val h = set.hold(a.finisher)
        assertTrue(h.held)
        assertEquals(0, a.finished)
        assertEquals(1, set.count())
        set.done(h)
        assertEquals(1, a.finished)
        assertEquals(0, set.count())
    }

    @Test
    fun theReleaseLetsGoOfEveryHeldBroadcastBeforeItWaits() {
        // The phone's case: the alarm that started the release, and the
        // second alarm queued behind it.
        val set = HoldSet()
        val first = Counter()
        val second = Counter()
        val h1 = set.hold(first.finisher)
        val h2 = set.hold(second.finisher)
        assertEquals(2, set.finishAllAndClose())
        assertEquals(1, first.finished)
        assertEquals(1, second.finished)
        assertEquals(0, set.count())
        // Their work ends later: never finished twice.
        set.done(h1)
        set.done(h2)
        assertEquals(1, first.finished)
        assertEquals(1, second.finished)
    }

    @Test
    fun nothingIsHeldWhileTheReleaseWaits() {
        val set = HoldSet()
        set.finishAllAndClose()
        val late = Counter()
        val h = set.hold(late.finisher)
        assertFalse(h.held)
        assertEquals(1, late.finished)
        assertEquals(0, set.count())
        set.done(h)
        assertEquals(1, late.finished)
        // After the wait, broadcasts are held again.
        set.reopen()
        val next = Counter()
        assertTrue(set.hold(next.finisher).held)
        assertEquals(0, next.finished)
        assertEquals(1, set.count())
    }

    @Test
    fun finishingWithNothingHeldIsHarmless() {
        val set = HoldSet()
        assertEquals(0, set.finishAllAndClose())
        set.reopen()
        assertEquals(0, set.count())
    }

    @Test
    fun aFinisherThatAndroidAlreadyClosedDoesNotThrow() {
        val set = HoldSet()
        val h = set.hold { throw IllegalStateException("Broadcast already finished") }
        set.done(h)
        set.done(h)
        assertEquals(0, set.count())
    }
}
