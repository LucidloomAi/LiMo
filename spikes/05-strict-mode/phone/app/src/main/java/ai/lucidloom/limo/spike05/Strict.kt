package ai.lucidloom.limo.spike05

import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.SystemClock
import android.os.UserManager
import android.provider.Settings
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

/**
 * Everything this app does as Device Owner, and the one way it undoes it.
 *
 * All work runs on [worker], one task at a time, and the public calls also
 * hold [lock], so an apply and a release can never run at the same time.
 * Never call these from the main thread: when this app removes itself as an
 * admin, Android first delivers "admin disabled" to this app's main thread
 * and waits for it, and only then removes the admin.
 */
object Strict {

    val worker: ExecutorService = Executors.newSingleThreadExecutor { r -> Thread(r, "limo-spike05-worker") }

    private val lock = Any()
    private var lastState: String? = null

    /** Broadcasts held open by [TimerReceiver]; see [HoldSet]. */
    val holds = HoldSet()

    /** When the last release ended, on the elapsed clock. In memory only. */
    @Volatile
    private var lastReleaseEndElapsed: Long? = null

    const val SOURCE_TICK = "tick"

    fun admin(ctx: Context): ComponentName = ComponentName(ctx.applicationContext, AdminReceiver::class.java)

    private fun dpm(ctx: Context): DevicePolicyManager = ctx.getSystemService(DevicePolicyManager::class.java)

    fun isAdmin(ctx: Context): Boolean = try {
        dpm(ctx).isAdminActive(admin(ctx))
    } catch (_: Exception) {
        false
    }

    fun isOwner(ctx: Context): Boolean = try {
        dpm(ctx).isDeviceOwnerApp(ctx.packageName)
    } catch (_: Exception) {
        false
    }

    fun clocksNow(ctx: Context): TimerRules.Clocks =
        TimerRules.Clocks(System.currentTimeMillis(), SystemClock.elapsedRealtime(), bootCount(ctx))

    private fun bootCount(ctx: Context): Int = try {
        Settings.Global.getInt(ctx.contentResolver, Settings.Global.BOOT_COUNT, -1)
    } catch (_: Exception) {
        -1
    }

    /** Milliseconds left on the timer, or null when there is no deadline. */
    fun leftMsNow(ctx: Context): Long? {
        val r = Store.timer(ctx)
        return if (r.isSet) TimerRules.leftMs(r, clocksNow(ctx)) else null
    }

    /**
     * True or false, or null when the package is not installed or its state
     * cannot be read. Android 10 and later answer for any visible package;
     * on Android 9 only a Device Owner can ask.
     */
    fun isSuspendedNow(ctx: Context, pkg: String): Boolean? = try {
        if (Build.VERSION.SDK_INT >= 29) {
            ctx.packageManager.isPackageSuspended(pkg)
        } else if (isOwner(ctx)) {
            dpm(ctx).isPackageSuspended(admin(ctx), pkg)
        } else {
            null
        }
    } catch (_: PackageManager.NameNotFoundException) {
        null
    } catch (_: Exception) {
        null
    }

    // the deadline check

    /**
     * The dead man's timer's check. Runs every time any part of the app
     * starts (the process, the activity, every receiver), every 5 seconds
     * while the activity is open, and on every alarm. If in doubt, release.
     */
    fun check(ctx: Context, source: String) {
        val c = ctx.applicationContext
        synchronized(lock) {
            val admin = isAdmin(c)
            val owner = isOwner(c)
            when (val d = TimerRules.decide(admin, owner, Store.timer(c), clocksNow(c))) {
                is TimerRules.Decision.Idle -> Unit
                is TimerRules.Decision.Forget -> {
                    Store.clearTimer(c)
                    Alarms.cancel(c)
                    SpikeLog.i("timer cleared reason=not_admin source=$source")
                }
                is TimerRules.Decision.Start -> {
                    Store.saveTimer(c, d.record)
                    Alarms.schedule(c, d.record)
                    SpikeLog.i("timer started deadline_min=${TimerRules.DEFAULT_MINUTES} source=$source")
                }
                is TimerRules.Decision.Keep -> {
                    Store.saveTimer(c, d.record)
                    if (source != SOURCE_TICK) Alarms.schedule(c, d.record)
                }
                is TimerRules.Decision.Release -> {
                    val nowElapsed = SystemClock.elapsedRealtime()
                    val lastEnd = lastReleaseEndElapsed
                    if (TimerRules.releaseJustRan(lastEnd, nowElapsed)) {
                        // A check queued while a release ran (the second
                        // alarm, "admin disabled"). Do not run it all again;
                        // a release that did not finish has its own retry.
                        SpikeLog.i("timer skipped source=$source reason=release_just_ran ago_ms=${nowElapsed - (lastEnd ?: nowElapsed)}")
                        logState(c, force = true)
                        return
                    }
                    SpikeLog.i("timer fired source=$source reason=${d.reason}")
                    releaseEverything(c, "timer")
                    return
                }
            }
            logState(c, force = source != SOURCE_TICK)
        }
    }

    /** "admin enabled": a fresh deadline, unless one is still running. */
    fun onAdminEnabled(ctx: Context) {
        val c = ctx.applicationContext
        synchronized(lock) {
            val before = Store.timer(c)
            val now = clocksNow(c)
            val after = TimerRules.onAdminEnabled(before, now)
            if (after != before) {
                Store.saveTimer(c, after)
                SpikeLog.i("timer started deadline_min=${TimerRules.minutesText(TimerRules.leftMs(after, now))} source=admin_enabled")
            } else {
                SpikeLog.i("timer kept deadline_min=${TimerRules.minutesText(TimerRules.leftMs(after, now))} source=admin_enabled")
            }
        }
        check(c, "admin_enabled")
    }

    fun logState(ctx: Context, force: Boolean) {
        val c = ctx.applicationContext
        val line = LogText.state(isAdmin(c), isOwner(c), leftMsNow(c))
        if (force || line != lastState) {
            SpikeLog.i(line)
            lastState = line
        }
    }

    // changing the deadline

    /** Lowers the time left to [minutes] (0 to 120). Never raises it. */
    fun lowerDeadline(ctx: Context, minutes: Int, source: String) {
        val c = ctx.applicationContext
        synchronized(lock) {
            if (!isAdmin(c) && !isOwner(c)) {
                SpikeLog.w("timer refused requested_min=$minutes reason=not_admin source=$source")
                return
            }
            when (val ch = TimerRules.lowerTo(Store.timer(c), clocksNow(c), minutes)) {
                is TimerRules.Change.Done -> {
                    Store.saveTimer(c, ch.record)
                    Alarms.schedule(c, ch.record)
                    SpikeLog.i("timer set deadline_min=${TimerRules.minutesText(ch.leftMs)} source=$source")
                }
                is TimerRules.Change.Refused ->
                    SpikeLog.w("timer refused requested_min=$minutes reason=${ch.reason} source=$source")
            }
        }
        // A deadline of 0 releases at once.
        check(c, source)
    }

    /** The explicit "extend 30 minutes" action, never beyond 120 minutes from now. */
    fun extendDeadline(ctx: Context, source: String) {
        val c = ctx.applicationContext
        synchronized(lock) {
            if (!isAdmin(c) && !isOwner(c)) {
                SpikeLog.w("timer refused action=extend reason=not_admin source=$source")
                return
            }
            when (val ch = TimerRules.extend(Store.timer(c), clocksNow(c))) {
                is TimerRules.Change.Done -> {
                    Store.saveTimer(c, ch.record)
                    Alarms.schedule(c, ch.record)
                    SpikeLog.i("timer extended deadline_min=${TimerRules.minutesText(ch.leftMs)} source=$source")
                }
                is TimerRules.Change.Refused ->
                    SpikeLog.w("timer refused action=extend reason=${ch.reason} source=$source")
            }
            logState(c, force = true)
        }
    }

    // strict mode

    /** Saves the list of packages to suspend. Returns how many were accepted. */
    fun setWantedList(ctx: Context, raw: String, source: String): Int {
        val c = ctx.applicationContext
        synchronized(lock) {
            val sys = SystemPackages.load(c)
            val choice = SuspendRules.choose(raw, c.packageName, sys.neverSuspend, sys.launchable)
            for ((p, why) in choice.refused) SpikeLog.w("suspend refused package=$p reason=$why")
            Store.saveWanted(c, choice.accepted)
            SpikeLog.i("suspend list=${SuspendRules.joinList(choice.accepted)} source=$source")
            return choice.accepted.size
        }
    }

    /** Carries out switch changes in the order [Steps.plan] gives. */
    fun applyPlan(ctx: Context, plan: List<Pair<Step, Boolean>>, source: String) {
        val c = ctx.applicationContext
        synchronized(lock) {
            if (plan.isNotEmpty()) SpikeLog.i("apply start source=$source steps=${plan.joinToString(",") { "${it.first.key}:${if (it.second) "on" else "off"}" }}")
            for ((step, on) in plan) applyOne(c, step, on)
            logSuspended(c)
            logState(c, force = true)
        }
    }

    private fun applyOne(c: Context, step: Step, on: Boolean) {
        val value = if (on) "on" else "off"
        if (!isOwner(c)) {
            SpikeLog.w("apply step=${step.key} failed value=$value error=not_owner")
            return
        }
        val dpm = dpm(c)
        val admin = admin(c)
        try {
            val detail = when {
                step == Step.BLOCK_UNINSTALL -> {
                    dpm.setUninstallBlocked(admin, c.packageName, on)
                    ""
                }
                step.isUserRestriction -> {
                    if (on) dpm.addUserRestriction(admin, step.key) else dpm.clearUserRestriction(admin, step.key)
                    ""
                }
                on -> suspendWanted(c)
                else -> {
                    // Switching suspension off lifts this app's own list and
                    // then sweeps, the same as release does, so it also works
                    // when the list was lost. The PC's force-release sends this
                    // first, because Android does not lift suspensions when the
                    // owner is removed from the PC.
                    val mine = Store.mine(c)
                    val still = unsuspend(c, mine)
                    val (swept, stillSwept) = sweepSuspensions(c)
                    val left = (still + stillSwept).distinct()
                    if (left.isNotEmpty()) throw IllegalStateException("still_suspended=${left.joinToString(",")}")
                    "count=${mine.size} swept=${swept.size}"
                }
            }
            SpikeLog.i("apply step=${step.key} ok value=$value" + if (detail.isEmpty()) "" else " $detail")
        } catch (e: Exception) {
            SpikeLog.w("apply step=${step.key} failed value=$value error=${LogText.oneLine(e)}")
        }
    }

    /** Suspends the wanted list (rules checked again now) and lifts what is no longer on it. */
    private fun suspendWanted(c: Context): String {
        val sys = SystemPackages.load(c)
        val choice = SuspendRules.choose(Store.wanted(c).joinToString(","), c.packageName, sys.neverSuspend, sys.launchable)
        for ((p, why) in choice.refused) SpikeLog.w("suspend refused package=$p reason=$why")
        val lift = SuspendRules.toLift(Store.mine(c), choice.accepted)
        if (lift.isNotEmpty()) {
            val still = unsuspend(c, lift)
            if (still.isNotEmpty()) SpikeLog.w("suspend lift_failed packages=${still.joinToString(",")}")
        }
        if (choice.accepted.isEmpty()) return "count=0"
        // Write ahead: on disk before Android is asked.
        Store.saveMine(c, Store.mine(c) + choice.accepted)
        val notDone = dpm(c).setPackagesSuspended(admin(c), choice.accepted.toTypedArray(), true).toList()
        if (notDone.isNotEmpty()) throw IllegalStateException("not_suspended=${notDone.joinToString(",")}")
        return "count=${choice.accepted.size}"
    }

    /**
     * Asks Android to lift these suspensions, then keeps in the record only
     * the packages that are really still suspended. Returns those. A package
     * that is no longer installed has nothing left to lift.
     */
    private fun unsuspend(c: Context, pkgs: List<String>): List<String> {
        if (pkgs.isNotEmpty()) dpm(c).setPackagesSuspended(admin(c), pkgs.toTypedArray(), false)
        val still = pkgs.filter { isSuspendedNow(c, it) == true }
        Store.saveMine(c, Store.mine(c).filter { it !in pkgs || it in still })
        return still
    }

    /**
     * Lifts every suspension Android reports on an installed package other
     * than this app, in case this app's own list was lost. A Device Owner's
     * unsuspend only takes back suspensions made through a Device Owner;
     * anything still suspended afterwards was suspended by someone else.
     * Returns the packages found suspended, and those still suspended.
     */
    private fun sweepSuspensions(c: Context): Pair<List<String>, List<String>> {
        val found = installedPackages(c).filter { it != c.packageName && isSuspendedNow(c, it) == true }
        if (found.isEmpty()) return Pair(emptyList(), emptyList())
        dpm(c).setPackagesSuspended(admin(c), found.toTypedArray(), false)
        val still = found.filter { isSuspendedNow(c, it) == true }
        Store.saveMine(c, Store.mine(c).filter { it !in found || it in still })
        return Pair(found, still)
    }

    private fun userUnlocked(c: Context): Boolean = try {
        c.getSystemService(UserManager::class.java).isUserUnlocked
    } catch (_: Exception) {
        true
    }

    private fun logSuspended(c: Context) {
        val now = Store.mine(c).filter { isSuspendedNow(c, it) == true }
        SpikeLog.i("suspended list=${SuspendRules.joinList(now)}")
    }

    @Suppress("DEPRECATION")
    private fun installedPackages(c: Context): List<String> =
        if (Build.VERSION.SDK_INT >= 33) {
            c.packageManager.getInstalledApplications(PackageManager.ApplicationInfoFlags.of(0L)).map { it.packageName }
        } else {
            c.packageManager.getInstalledApplications(0).map { it.packageName }
        }

    // release

    /**
     * The one release, used by every way out: the button, the adb intent and
     * the timer. It undoes everything this app set, in [Steps.RELEASE_STEPS]
     * order, each step in its own try block so one failure cannot stop the
     * rest, and logs each step. Then it clears Device Owner and removes
     * itself as an admin. It does every step every time, whether or not the
     * app thinks it was set: undoing something that was never set does no
     * harm.
     *
     * If the app is still owner or admin at the end, the timer stays armed
     * and tries again in a minute.
     */
    fun releaseEverything(ctx: Context, trigger: String) {
        val c = ctx.applicationContext
        synchronized(lock) {
            SpikeLog.i(
                "release start trigger=$trigger owner=${LogText.yesNo(isOwner(c))} admin=${LogText.yesNo(isAdmin(c))} " +
                    "user_unlocked=${LogText.yesNo(userUnlocked(c))}",
            )
            // Arm the retry first: if this process dies half way, the retry
            // alarm still comes. It also replaces the deadline alarms, so the
            // other one cannot fire in the middle of this release.
            Alarms.armRetryOnly(c)
            for (name in Steps.RELEASE_STEPS) {
                try {
                    releaseStep(c, name)
                } catch (e: Exception) {
                    SpikeLog.w("release step=$name failed error=${LogText.oneLine(e)}")
                }
            }
            val owner = isOwner(c)
            val admin = isAdmin(c)
            if (!owner && !admin) {
                Store.clearTimer(c)
                Alarms.cancel(c)
            } else {
                // Never give up. Android lets an owner step down only once the
                // phone has been unlocked since it started; the deadline stays
                // stored, so the next alarm, unlock, restart or start of any
                // part of the app tries again.
                SpikeLog.w(
                    "release incomplete owner=${LogText.yesNo(owner)} admin=${LogText.yesNo(admin)} " +
                        "user_unlocked=${LogText.yesNo(userUnlocked(c))} next_try_s=${TimerRules.RETRY_MS / 1000L}",
                )
                Alarms.scheduleRetry(c)
            }
            lastReleaseEndElapsed = SystemClock.elapsedRealtime()
            SpikeLog.i(LogText.releaseDone(owner, admin))
            logState(c, force = true)
        }
    }

    private fun releaseStep(c: Context, name: String) {
        val dpm = dpm(c)
        val admin = admin(c)
        when (name) {
            "unsuspend" -> {
                val mine = Store.mine(c)
                if (!isOwner(c)) {
                    val still = mine.filter { isSuspendedNow(c, it) == true }
                    Store.saveMine(c, still)
                    if (still.isEmpty()) {
                        skipped(name, "not_owner")
                    } else {
                        failed(name, "not_owner still_suspended=${still.joinToString(",")}")
                    }
                    return
                }
                val still = unsuspend(c, mine)
                if (still.isEmpty()) ok(name, "count=${mine.size}") else failed(name, "still_suspended=${still.joinToString(",")}")
            }
            "unsuspend_sweep" -> {
                // Belt and braces: any package Android reports as suspended,
                // in case this app's own list was lost.
                if (!isOwner(c)) {
                    skipped(name, "not_owner")
                    return
                }
                val (found, still) = sweepSuspensions(c)
                if (found.isEmpty()) {
                    ok(name, "found=0")
                } else if (still.isEmpty()) {
                    ok(name, "found=${found.size} lifted=${found.joinToString(",")}")
                } else {
                    failed(name, "found=${found.joinToString(",")} still_suspended=${still.joinToString(",")}")
                }
            }
            "uninstall_block" -> {
                if (!isOwner(c)) {
                    skipped(name, "not_owner")
                    return
                }
                dpm.setUninstallBlocked(admin, c.packageName, false)
                ok(name, "")
            }
            "device_owner" -> {
                if (!isOwner(c)) {
                    skipped(name, "not_owner")
                    return
                }
                @Suppress("DEPRECATION")
                dpm.clearDeviceOwnerApp(c.packageName)
                if (isOwner(c)) failed(name, "still_owner") else ok(name, "")
            }
            "active_admin" -> {
                if (!isAdmin(c)) {
                    skipped(name, "not_admin")
                    return
                }
                dpm.removeActiveAdmin(admin)
                // Android removes the admin a moment later, once it has
                // delivered "admin disabled" to this app's main thread. It
                // delivers this app's broadcasts one after another, so first
                // let go of every broadcast this app is holding open (the
                // alarm that started this release, a second alarm queued
                // behind it), and hold no new one until the wait is over.
                // Otherwise the release waits on itself.
                val finished = holds.finishAllAndClose()
                var waited = 0L
                try {
                    while (isAdmin(c) && waited < 5_000L) {
                        SystemClock.sleep(250L)
                        waited += 250L
                    }
                } finally {
                    holds.reopen()
                }
                if (isAdmin(c)) {
                    failed(name, "still_admin_after_ms=$waited broadcasts_finished=$finished")
                } else {
                    ok(name, "waited_ms=$waited broadcasts_finished=$finished")
                }
            }
            else -> {
                // The four user restrictions, by their Android names.
                if (!isOwner(c)) {
                    skipped(name, "not_owner")
                    return
                }
                dpm.clearUserRestriction(admin, name)
                ok(name, "")
            }
        }
    }

    private fun ok(name: String, detail: String) =
        SpikeLog.i("release step=$name ok" + if (detail.isEmpty()) "" else " $detail")

    private fun failed(name: String, error: String) = SpikeLog.w("release step=$name failed error=$error")

    private fun skipped(name: String, reason: String) = SpikeLog.i("release step=$name skipped reason=$reason")
}
