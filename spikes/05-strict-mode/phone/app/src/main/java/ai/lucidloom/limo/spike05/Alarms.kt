package ai.lucidloom.limo.spike05

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.SystemClock

/**
 * The dead man's timer's alarms. Two are set for each deadline, both to
 * [TimerReceiver], which runs the same check as everything else:
 *
 * - one on the elapsed clock, which no clock change can move;
 * - one on the wall clock, which fires early if someone moves the clock
 *   forward, and that ends the timer early too. It is set [WALL_LAG_MS]
 *   after the deadline, so that with no clock change the elapsed alarm comes
 *   first, and the release it starts replaces this one before it fires.
 *
 * Alarms do not survive a restart; the boot receiver sets them again.
 */
object Alarms {

    const val ACTION_TIMER = "ai.lucidloom.limo.spike05.TIMER"
    private const val CODE_ELAPSED = 1
    private const val CODE_WALL = 2
    private const val WALL_LAG_MS = 5_000L

    private fun pending(c: Context, code: Int): PendingIntent =
        PendingIntent.getBroadcast(
            c,
            code,
            Intent(c, TimerReceiver::class.java).setAction(ACTION_TIMER),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

    fun schedule(c: Context, r: TimerRules.Record) {
        val am = c.getSystemService(AlarmManager::class.java) ?: return
        val exactElapsed = set(am, AlarmManager.ELAPSED_REALTIME_WAKEUP, r.elapsedMs, pending(c, CODE_ELAPSED))
        val exactWall = set(am, AlarmManager.RTC_WAKEUP, r.wallMs + WALL_LAG_MS, pending(c, CODE_WALL))
        val inMs = r.elapsedMs - SystemClock.elapsedRealtime()
        SpikeLog.i("alarm scheduled exact=${LogText.yesNo(exactElapsed && exactWall)} in_min=${TimerRules.minutesText(inMs)}")
    }

    /** After a release that did not finish: try again in a minute. */
    fun scheduleRetry(c: Context) {
        val am = c.getSystemService(AlarmManager::class.java) ?: return
        val at = SystemClock.elapsedRealtime() + TimerRules.RETRY_MS
        val exact = set(am, AlarmManager.ELAPSED_REALTIME_WAKEUP, at, pending(c, CODE_ELAPSED))
        SpikeLog.i("alarm scheduled exact=${LogText.yesNo(exact)} retry_s=${TimerRules.RETRY_MS / 1000L}")
    }

    /**
     * At the start of a release: drop the wall clock alarm and put the retry
     * in place of the elapsed one, so no deadline alarm fires in the middle of
     * the release, and a retry still comes if this process dies half way.
     */
    fun armRetryOnly(c: Context) {
        val am = c.getSystemService(AlarmManager::class.java) ?: return
        am.cancel(pending(c, CODE_WALL))
        scheduleRetry(c)
    }

    fun cancel(c: Context) {
        val am = c.getSystemService(AlarmManager::class.java) ?: return
        am.cancel(pending(c, CODE_ELAPSED))
        am.cancel(pending(c, CODE_WALL))
    }

    /** Sets an exact alarm if Android allows it, else an inexact one. Returns whether it is exact. */
    private fun set(am: AlarmManager, type: Int, at: Long, pi: PendingIntent): Boolean {
        val mayExact = Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms()
        if (mayExact) {
            try {
                am.setExactAndAllowWhileIdle(type, at, pi)
                return true
            } catch (_: SecurityException) {
                // Fall through to an inexact alarm.
            }
        }
        am.setAndAllowWhileIdle(type, at, pi)
        return false
    }
}
