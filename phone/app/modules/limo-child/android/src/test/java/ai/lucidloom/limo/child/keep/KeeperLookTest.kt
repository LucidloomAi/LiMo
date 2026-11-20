package ai.lucidloom.limo.child.keep

import ai.lucidloom.limo.child.ffi.TimeLeft
import ai.lucidloom.limo.child.ffi.WarningCause
import ai.lucidloom.limo.child.ffi.WarningShown
import ai.lucidloom.limo.child.host.HostTimes
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What the keeper's notifications show, in the engine's words, and when
 * they are posted again; and when the host ticks and sets its alarm.
 */
class KeeperLookTest {
  private val now = 1_794_997_800_000L

  private fun left(secs: Long?, counting: Boolean, words: String, locked: Boolean = false, until: Long? = null) =
    TimeLeft(secondsLeft = secs, counting = counting, unlockedUntil = until, away = true, locked = locked, words = words)

  @Test
  fun theTimeLeftIsTheEnginesWordsWithAndroidsCountdownWhileTimeIsCounted() {
    // Counting: the chronometer counts down to the moment the time runs out.
    val counting = KeeperLook.timeLeft(left(4_320, true, "1 h 12 min left"), now)
    assertEquals("1 h 12 min left", counting.title)
    assertEquals(now + 4_320_000, counting.countdownToMs)
    // Paused, locked, open under a guardian's unlock, no limit: the words alone.
    assertNull(KeeperLook.timeLeft(left(4_320, false, "1 h 12 min paused"), now).countdownToMs)
    val locked = KeeperLook.timeLeft(left(0, false, "Today's time is used up.", locked = true), now)
    assertEquals("Today's time is used up.", locked.title)
    assertNull(locked.countdownToMs)
    assertNull(KeeperLook.timeLeft(left(600, true, "Open until 9:30 pm", until = 1_795_000_000), now).countdownToMs)
    assertNull(KeeperLook.timeLeft(left(null, true, "No limit today"), now).countdownToMs)
  }

  @Test
  fun theNotificationIsPostedAgainOnlyWhenWhatItShowsChanges() {
    val first = KeeperLook.timeLeft(left(4_320, true, "1 h 12 min left"), now)
    assertTrue(KeeperLook.differs(null, first))
    // A second later, a second less: the same countdown, not posted again.
    val next = KeeperLook.timeLeft(left(4_319, true, "1 h 12 min left"), now + 1_000)
    assertFalse(KeeperLook.differs(first, next))
    // Rounding of whole seconds moves the end a little: still not posted.
    val jitter = KeeperLook.timeLeft(left(4_318, true, "1 h 12 min left"), now + 1_999)
    assertFalse(KeeperLook.differs(first, jitter))
    // The words change, once a minute: posted.
    assertTrue(KeeperLook.differs(first, KeeperLook.timeLeft(left(4_259, true, "1 h 11 min left"), now + 61_000)))
    // Time added: the end moves far: posted.
    assertTrue(KeeperLook.differs(first, KeeperLook.timeLeft(left(4_320 + 900, true, "1 h 12 min left"), now + 1_000)))
    // Counting stops, or starts: posted.
    val paused = KeeperLook.timeLeft(left(4_319, false, "1 h 12 min paused"), now + 1_000)
    assertTrue(KeeperLook.differs(first, paused))
    assertTrue(KeeperLook.differs(paused, first))
    assertFalse(KeeperLook.differs(paused, paused.copy()))
  }

  @Test
  fun aWarningShowsTheEnginesMessageHintAndWhenAndGoesAwayAfterTheLock() {
    val warning = WarningShown(
      cause = WarningCause.TIME_RUNS_OUT,
      minutes = 5,
      lockAt = now / 1_000 + 300,
      away = false,
      message = "5 minutes of screen time left today.",
      hint = "Save what you are doing.",
      whenWords = "Locks about 9:47 pm",
    )
    val look = KeeperLook.warning(warning, now / 1_000)
    assertEquals("5 minutes of screen time left today.", look.title)
    assertEquals("Save what you are doing.", look.text)
    assertEquals("Locks about 9:47 pm", look.whenWords)
    assertEquals(300_000 + KeeperLook.WARNING_MIN_MS, look.timeoutMs)
    // A lock already past: a minute all the same.
    assertEquals(KeeperLook.WARNING_MIN_MS, KeeperLook.warning(warning.copy(lockAt = now / 1_000 - 50), now / 1_000).timeoutMs)
  }

  @Test
  fun theHostTicksEverySecondWithTheScreenOnAndAtTheEnginesNextCheckWithItOff() {
    val t = 10_000_000L
    // The screen on: a tick in a second; the alarm a quarter of an hour away, set once.
    val on = HostTimes.plan(interactive = true, nextCheckInSecs = 1, nowMs = t, alarmAt = null)
    assertEquals(1_000L, on.tickInMs)
    assertEquals(t + HostTimes.WATCHDOG_MS, on.alarmAt)
    // A second later the alarm set stays: it is not set again every second.
    assertNull(HostTimes.plan(true, 1, t + 1_000, on.alarmAt).alarmAt)
    // Until it comes within five minutes: set again.
    val late = t + HostTimes.WATCHDOG_MS - HostTimes.ALARM_MIN_AHEAD_MS + 1
    assertEquals(late + HostTimes.WATCHDOG_MS, HostTimes.plan(true, 1, late, on.alarmAt).alarmAt)

    // The screen off: no tick every second; the engine's next check, by tick and by alarm.
    val off = HostTimes.plan(interactive = false, nextCheckInSecs = 90, nowMs = t, alarmAt = null)
    assertEquals(90_000L, off.tickInMs)
    assertEquals(t + 90_000, off.alarmAt)
    // A check further than a quarter of an hour: no further than that.
    val far = HostTimes.plan(false, 3 * 60 * 60, t, null)
    assertEquals(HostTimes.WATCHDOG_MS, far.tickInMs)
    assertEquals(t + HostTimes.WATCHDOG_MS, far.alarmAt)
    // An earlier check than the alarm set (bedtime soon): the alarm moves up.
    assertEquals(t + 30_000, HostTimes.plan(false, 30, t, t + HostTimes.WATCHDOG_MS).alarmAt)
    // After a failure, with no check from the engine: a minute.
    assertEquals(HostTimes.RETRY_MS, HostTimes.plan(false, null, t, null).tickInMs)
  }
}
