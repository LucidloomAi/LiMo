package ai.lucidloom.limo.child.keep

import ai.lucidloom.limo.child.ffi.TimeLeft
import ai.lucidloom.limo.child.ffi.WarningShown
import kotlin.math.abs

/**
 * What the time left notification shows: the engine's words as its title,
 * and, while time is counted, Android's own chronometer counting down to
 * the moment the time runs out, so that the seconds move without LiMo.
 */
internal data class TimeLeftLook(
  /** The engine's words: "1 h 12 min left", "1 h 12 min paused", "No limit today", "Open until 9:30 pm", or the lock's title. */
  val title: String,
  /** While counting, the wall clock moment (milliseconds) the chronometer counts down to; null otherwise. */
  val countdownToMs: Long?,
)

/** What a warning's heads up notification shows: the engine's message, hint and when, as they are. */
internal data class WarningLook(
  /** "5 minutes of screen time left today." */
  val title: String,
  /** "Save what you are doing." */
  val text: String,
  /** "Locks about 9:47 pm". */
  val whenWords: String,
  /** It goes away by itself this long after it is posted: a minute after the lock it warns of. */
  val timeoutMs: Long,
)

/** The keeper's notifications as plain values, and when to post them again. No Android in here. */
internal object KeeperLook {
  /** The countdown's end may move by this much between two ticks (whole seconds) before the notification is posted again. */
  const val DRIFT_MS = 2_000L

  /** A warning stays at least this long. */
  const val WARNING_MIN_MS = 60_000L

  fun timeLeft(t: TimeLeft, nowWallMs: Long): TimeLeftLook {
    val left = t.secondsLeft
    // A countdown only while the day's own time is counted: not while
    // locked, nor under a guardian's unlock ("Open until ..."), nor with no limit.
    val end = if (left != null && t.counting && !t.locked && t.unlockedUntil == null) nowWallMs + left * 1_000 else null
    return TimeLeftLook(t.words, end)
  }

  /**
   * Post the notification again: the first time, when the words change
   * (once a minute while counting, every second in the last minute), when
   * the countdown starts or stops, or when its end moved by more than
   * [DRIFT_MS] (the clock was set, or time was added). Not at every tick.
   */
  fun differs(before: TimeLeftLook?, now: TimeLeftLook): Boolean {
    if (before == null) return true
    if (before.title != now.title) return true
    val a = before.countdownToMs
    val b = now.countdownToMs
    if ((a == null) != (b == null)) return true
    return a != null && b != null && abs(a - b) > DRIFT_MS
  }

  fun warning(w: WarningShown, nowWallSecs: Long): WarningLook {
    val untilLock = (w.lockAt - nowWallSecs).coerceAtLeast(0) * 1_000
    return WarningLook(w.message, w.hint, w.whenWords, (untilLock + WARNING_MIN_MS).coerceAtLeast(WARNING_MIN_MS))
  }
}
