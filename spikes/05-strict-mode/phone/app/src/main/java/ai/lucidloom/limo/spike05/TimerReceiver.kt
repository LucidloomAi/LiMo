package ai.lucidloom.limo.spike05

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * The dead man's timer's receiver: its own alarm, a restart, the first unlock
 * after a restart, an update of this app, and a change of the clock. Each runs
 * the same check, which releases the phone if the deadline has passed.
 *
 * Android lets an owner step down only once the phone has been unlocked since
 * it started. This app is not direct boot aware, so it cannot run before that
 * unlock anyway; BOOT_COMPLETED arrives right after it. USER_UNLOCKED is
 * listed too, although Android may send it only to receivers registered at
 * run time (from memory, not verified).
 *
 * goAsync keeps the process alive until the check (and a release, if one is
 * due) has finished on the worker thread. The hold goes through
 * [Strict.holds], because a release lets go of every held broadcast before
 * it waits for Android to remove the admin: Android delivers this app's
 * broadcasts one after another, and "admin disabled" must not queue behind a
 * broadcast that is itself waiting for the admin to go. A broadcast that
 * arrives during that wait is not held at all.
 */
class TimerReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        val source = when (intent.action) {
            Alarms.ACTION_TIMER -> "alarm"
            Intent.ACTION_BOOT_COMPLETED -> "boot"
            Intent.ACTION_USER_UNLOCKED -> "user_unlocked"
            Intent.ACTION_MY_PACKAGE_REPLACED -> "package_replaced"
            Intent.ACTION_TIME_CHANGED -> "time_set"
            else -> return
        }
        val app = context.applicationContext
        val pending = goAsync()
        val hold = Strict.holds.hold { pending.finish() }
        if (!hold.held) SpikeLog.i("broadcast not held source=$source reason=release_waiting")
        Strict.worker.execute {
            try {
                if (source == "boot") SpikeLog.i("boot seen owner=${LogText.yesNo(Strict.isOwner(app))}")
                Strict.check(app, source)
            } finally {
                Strict.holds.done(hold)
            }
        }
    }
}
