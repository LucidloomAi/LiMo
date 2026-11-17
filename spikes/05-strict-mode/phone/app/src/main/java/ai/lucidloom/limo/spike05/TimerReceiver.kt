package ai.lucidloom.limo.spike05

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * The dead man's timer's receiver: its own alarm, a restart, an update of this
 * app, and a change of the clock. Each runs the same check, which releases the
 * phone if the deadline has passed.
 *
 * goAsync keeps the process alive until the check (and a release, if one is
 * due) has finished on the worker thread.
 */
class TimerReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        val source = when (intent.action) {
            Alarms.ACTION_TIMER -> "alarm"
            Intent.ACTION_BOOT_COMPLETED -> "boot"
            Intent.ACTION_MY_PACKAGE_REPLACED -> "package_replaced"
            Intent.ACTION_TIME_CHANGED -> "time_set"
            else -> return
        }
        val app = context.applicationContext
        val pending = goAsync()
        Strict.worker.execute {
            try {
                if (source == "boot") SpikeLog.i("boot seen owner=${LogText.yesNo(Strict.isOwner(app))}")
                Strict.check(app, source)
            } finally {
                pending.finish()
            }
        }
    }
}
