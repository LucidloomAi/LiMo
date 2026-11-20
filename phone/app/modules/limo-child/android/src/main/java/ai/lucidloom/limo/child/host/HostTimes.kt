package ai.lucidloom.limo.child.host

/**
 * When the host ticks, and its times, in one place.
 *
 * - **The screen on:** once a second, as the engine's README asks while the
 *   screen is in use, and at once on any change the watch tells.
 * - **The screen off:** nothing counts, so no tick every second. The next
 *   tick is at the engine's next check (`next_check_in_secs`), on the host's
 *   own thread while the phone is awake, and by an ordinary alarm (not an
 *   exact one: LiMo holds no exact alarm permission) that wakes it from
 *   sleep. Android may deliver that alarm late, by up to about three
 *   quarters of the wait; nothing is counted meanwhile, and turning the
 *   screen on ticks at once.
 * - **Always** an alarm at most [WATCHDOG_MS] away, so that a LiMo stopped
 *   from Android's list of running apps comes back by itself. While the
 *   screen is on it is set again only when it comes within
 *   [ALARM_MIN_AHEAD_MS], about every ten minutes, not every second.
 */
internal object HostTimes {
  /** While the screen is on. */
  const val EVERY_SECOND_MS = 1_000L

  /** No alarm is ever further away than this. */
  const val WATCHDOG_MS = 15L * 60 * 1_000

  /** An alarm closer than this is set again. */
  const val ALARM_MIN_AHEAD_MS = 5L * 60 * 1_000

  /** An alarm wanted this much earlier than the one set replaces it. */
  const val ALARM_SLACK_MS = 1_000L

  /** After a failure with no next check from the engine, or with no engine made. */
  const val RETRY_MS = 60_000L

  /** After a broken engine was made again, before the next tick. */
  const val AFTER_BROKEN_MS = 1_000L

  /** One log line a minute at most for each kind of failure. */
  const val LOG_REPEAT_MS = 60_000L

  /** The next tick, and the alarm to set (null: the one set stays). */
  data class Plan(val tickInMs: Long, val alarmAt: Long?)

  /**
   * The next tick and alarm after a tick at [nowMs] (the elapsed count),
   * with the screen [interactive] or not, the engine's next check (null
   * after a failure) and the alarm already set ([alarmAt]).
   */
  fun plan(interactive: Boolean, nextCheckInSecs: Long?, nowMs: Long, alarmAt: Long?): Plan {
    val checkMs = if (nextCheckInSecs == null) RETRY_MS else nextCheckInSecs.coerceAtLeast(1) * 1_000
    val tickIn = if (interactive) EVERY_SECOND_MS else checkMs.coerceAtMost(WATCHDOG_MS)
    val want = nowMs + if (interactive) WATCHDOG_MS else checkMs.coerceAtMost(WATCHDOG_MS)
    val set = alarmAt == null || want < alarmAt - ALARM_SLACK_MS || alarmAt - nowMs < ALARM_MIN_AHEAD_MS
    return Plan(tickIn, if (set) want else null)
  }
}
