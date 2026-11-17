package ai.lucidloom.limo.spike05

import android.content.Context
import android.content.SharedPreferences

/**
 * What the app keeps on disk, in its own private preferences. Every write is
 * synchronous (commit), so it is on disk before the next step runs.
 *
 * - the dead man's timer's record;
 * - the list of packages the tester wants suspended;
 * - the packages this app has asked Android to suspend and has not yet seen
 *   lifted. A package is added here BEFORE Android is asked to suspend it, so
 *   a crash in between can never leave a suspension nobody knows about.
 */
object Store {

    private const val PREFS = "spike05"
    private const val K_WALL = "timer_wall_ms"
    private const val K_ELAPSED = "timer_elapsed_ms"
    private const val K_BOOT = "timer_boot_count"
    private const val K_CHECKED = "timer_checked_elapsed_ms"
    private const val K_WANTED = "suspend_wanted"
    private const val K_MINE = "suspended_by_me"

    private fun prefs(ctx: Context): SharedPreferences =
        ctx.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun timer(ctx: Context): TimerRules.Record {
        val p = prefs(ctx)
        return TimerRules.Record(
            wallMs = p.getLong(K_WALL, 0L),
            elapsedMs = p.getLong(K_ELAPSED, 0L),
            bootCount = p.getInt(K_BOOT, -1),
            checkedElapsedMs = p.getLong(K_CHECKED, 0L),
        )
    }

    fun saveTimer(ctx: Context, r: TimerRules.Record): Boolean =
        prefs(ctx).edit()
            .putLong(K_WALL, r.wallMs)
            .putLong(K_ELAPSED, r.elapsedMs)
            .putInt(K_BOOT, r.bootCount)
            .putLong(K_CHECKED, r.checkedElapsedMs)
            .commit()

    fun clearTimer(ctx: Context): Boolean = saveTimer(ctx, TimerRules.Record.NONE)

    fun wanted(ctx: Context): List<String> = SuspendRules.splitStored(prefs(ctx).getString(K_WANTED, ""))

    fun saveWanted(ctx: Context, list: List<String>): Boolean =
        prefs(ctx).edit().putString(K_WANTED, list.joinToString(",")).commit()

    fun mine(ctx: Context): List<String> = SuspendRules.splitStored(prefs(ctx).getString(K_MINE, ""))

    fun saveMine(ctx: Context, list: List<String>): Boolean =
        prefs(ctx).edit().putString(K_MINE, list.distinct().joinToString(",")).commit()
}
