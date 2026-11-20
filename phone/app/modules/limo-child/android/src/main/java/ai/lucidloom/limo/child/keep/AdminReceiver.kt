package ai.lucidloom.limo.child.keep

import ai.lucidloom.limo.child.module.R
import ai.lucidloom.limo.child.watch.Watch
import android.app.admin.DeviceAdminReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * The device admin of the base protection. It holds no policy
 * (res/xml/limo_admin.xml): while it is active Android refuses to uninstall
 * LiMo, and Settings greys out LiMo's Force stop and Clear storage
 * (keep/README.md, "What Android refuses an active device admin").
 *
 * **When someone asks to switch it off** (Deactivate on its page in
 * Settings), Android asks this receiver first ([onDisableRequested]). It
 * tells the watch at once, which raises the guard over that page
 * (watch/Guarded.kt), and answers with a warning, which Settings shows in a
 * dialog before it switches the admin off; that dialog is then under the
 * guard until a guardian types the PIN. The answer must come at once and
 * must not be empty: Settings waits two seconds at most, and with no answer,
 * or an empty one, it switches the admin off with no dialog at all.
 *
 * In strict mode (folder strict/) the same component is the phone's device
 * owner. Android remembers both by this class name, so it must not be
 * renamed or moved.
 */
class AdminReceiver : DeviceAdminReceiver() {
  override fun onDisableRequested(context: Context, intent: Intent): CharSequence {
    try {
      Watch.of(context).deviceAdminOffAsked()
    } catch (e: Exception) {
      Log.i(TAG, "keep failed what=admin_off error=${e.javaClass.simpleName}")
    }
    // The process may have been started for this broadcast: the host with it.
    try {
      Keeper.start(context, "admin")
    } catch (e: Exception) {
      Log.i(TAG, "keep failed what=admin_start error=${e.javaClass.simpleName}")
    }
    Log.i(TAG, "keep admin_off asked")
    return warning { context.getText(R.string.limo_admin_disable_warning) }
  }

  internal companion object {
    private const val TAG = "LiMo"

    /** Only if LiMo's own words cannot be read: anything but empty keeps Settings' dialog. */
    const val WARNING_FALLBACK = "LiMo"

    /**
     * The words to answer Android with: [read]'s, or [WARNING_FALLBACK] when
     * they cannot be read or are blank. Never empty: an empty answer lets
     * Settings switch the admin off at once, with no dialog.
     */
    fun warning(read: () -> CharSequence?): CharSequence {
      val words = try {
        read()
      } catch (e: Exception) {
        null
      }
      return if (words.isNullOrBlank()) WARNING_FALLBACK else words
    }
  }
}
