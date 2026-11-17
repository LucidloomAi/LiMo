package ai.lucidloom.limo.spike02

import android.content.Context
import android.content.SharedPreferences
import android.util.Log

/**
 * State shared by the activity and the service. They run in one process, and
 * everything here is used on the main thread only.
 *
 * The blocked list and the blocking deadline are saved, so they survive the
 * process being killed and a reboot (within the 10 minute window). The
 * measurements live in memory only.
 */
object SpikeState {

    const val TAG = "LiMoSpike"
    const val MAX_MEASUREMENTS = 50

    private const val PREFS = "spike02"
    private const val KEY_BLOCKED = "blocked"
    private const val KEY_UNTIL = "on_until_wall_ms"

    /** One cover shown. [trigger] is "event" or "unlock". */
    data class Measurement(
        val pkg: String,
        val shownMs: Long,
        val method: String,
        val trigger: String,
        val atWallMs: Long,
    )

    /** Newest first, at most [MAX_MEASUREMENTS]. */
    val measurements = ArrayDeque<Measurement>()

    /** How many measurements were ever added, so the activity knows when to redraw. */
    var measurementsAdded = 0L
        private set

    private var loaded = false
    private var blocked: Set<String> = emptySet()
    private var untilWallMs: Long = 0L

    private fun prefs(ctx: Context): SharedPreferences =
        ctx.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    private fun load(ctx: Context) {
        if (loaded) return
        val p = prefs(ctx)
        blocked = p.getStringSet(KEY_BLOCKED, emptySet())?.toSet() ?: emptySet()
        untilWallMs = p.getLong(KEY_UNTIL, 0L)
        loaded = true
    }

    fun blocked(ctx: Context): Set<String> {
        load(ctx)
        return blocked
    }

    fun untilWallMs(ctx: Context): Long {
        load(ctx)
        return untilWallMs
    }

    fun isOn(ctx: Context, nowWallMs: Long = System.currentTimeMillis()): Boolean =
        CoverRules.isBlockingActive(untilWallMs(ctx), nowWallMs)

    fun msLeft(ctx: Context, nowWallMs: Long = System.currentTimeMillis()): Long =
        CoverRules.msLeft(untilWallMs(ctx), nowWallMs)

    fun setBlocked(ctx: Context, packages: Set<String>) {
        load(ctx)
        blocked = packages.toSet()
        prefs(ctx).edit().putStringSet(KEY_BLOCKED, HashSet(blocked)).apply()
        Log.i(TAG, "state blocked=${blocked.sorted().joinToString(",")}")
        BlockService.instance?.onSettingsChanged()
    }

    /** Switching on always starts a fresh 10 minute window. */
    fun setOn(ctx: Context, on: Boolean, reason: String) {
        load(ctx)
        untilWallMs = if (on) System.currentTimeMillis() + CoverRules.SAFETY_STOP_MS else 0L
        prefs(ctx).edit().putLong(KEY_UNTIL, untilWallMs).apply()
        Log.i(TAG, "state blocking=${if (on) "on" else "off"} reason=$reason")
        BlockService.instance?.onSettingsChanged()
    }

    /**
     * If a deadline is stored but no longer active (the 10 minutes ran out,
     * or a clock change broke the deadline rule), clear it and log it once.
     * Returns true when it did so.
     */
    fun expireIfDue(ctx: Context): Boolean {
        load(ctx)
        if (untilWallMs == 0L) return false
        val now = System.currentTimeMillis()
        if (CoverRules.isBlockingActive(untilWallMs, now)) return false
        val reason = if (untilWallMs <= now) "safety_stop" else "deadline_rule"
        untilWallMs = 0L
        prefs(ctx).edit().putLong(KEY_UNTIL, 0L).apply()
        Log.i(TAG, "state blocking=off reason=$reason")
        return true
    }

    fun addMeasurement(m: Measurement) {
        measurements.addFirst(m)
        while (measurements.size > MAX_MEASUREMENTS) measurements.removeLast()
        measurementsAdded += 1
    }

    fun clearMeasurements() {
        measurements.clear()
        measurementsAdded += 1
        Log.i(TAG, "state timings=cleared")
    }
}
