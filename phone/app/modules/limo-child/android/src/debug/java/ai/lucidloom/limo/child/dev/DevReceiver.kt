package ai.lucidloom.limo.child.dev

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Debug builds only (src/debug). The development tool on the PC will send
 * its commands here over the USB cable, before the link exists: set up a
 * test child, read the status, take a code. A stub: it does nothing.
 *
 * Only the adb shell and the system can send to it (it is guarded by
 * android.permission.DUMP, see src/debug/AndroidManifest.xml), and a release
 * build does not contain it.
 */
class DevReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) = Unit
}
