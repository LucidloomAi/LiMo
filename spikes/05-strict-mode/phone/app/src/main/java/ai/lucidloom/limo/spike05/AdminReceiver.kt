package ai.lucidloom.limo.spike05

import android.app.admin.DeviceAdminReceiver
import android.content.Context
import android.content.Intent

/**
 * The device admin that set-device-owner names. It does nothing by itself;
 * it only tells the rest of the app when it was enabled or disabled.
 *
 * Neither callback waits for the work it hands to [Strict.worker]. Android
 * removes an admin only after this app's "disabled" callback has returned, and
 * the release on the worker is waiting for exactly that removal.
 */
class AdminReceiver : DeviceAdminReceiver() {

    override fun onEnabled(context: Context, intent: Intent) {
        val app = context.applicationContext
        SpikeLog.i("admin enabled")
        Strict.worker.execute { Strict.onAdminEnabled(app) }
    }

    override fun onDisabled(context: Context, intent: Intent) {
        val app = context.applicationContext
        SpikeLog.i("admin disabled")
        Strict.worker.execute { Strict.check(app, "admin_disabled") }
    }
}
