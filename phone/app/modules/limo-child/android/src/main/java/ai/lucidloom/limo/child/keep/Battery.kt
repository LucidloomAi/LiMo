package ai.lucidloom.limo.child.keep

import android.annotation.SuppressLint
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings

/**
 * Leaving LiMo out of battery optimisation, for the setup screens' Battery
 * step ("Android asks once. Choose Allow."). Nothing calls it yet: the
 * piece that answers the screens' requests does, from `child.openStep`.
 *
 * Why it matters: Android may stop or hold back an app that is optimised,
 * and from Android 12 an app left out of optimisation may start its
 * foreground service from the background, so LiMo's alarm can bring it back
 * after it was stopped. Whether it is left out is read by the watch
 * (`permissions().batteryExempt`).
 */
object Battery {
  /**
   * Shows Android's own question whether LiMo may run in the background
   * (`ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`, which the manifest's
   * `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` lets it ask). Where a phone has
   * no such screen, Android's list of apps left out instead. True when one
   * of them opened.
   */
  // LiMo's whole work is keeping limits while the child uses the phone:
  // being stopped in the background is what it must not be.
  @SuppressLint("BatteryLife")
  @JvmStatic
  fun askToLeaveOutOfOptimisation(context: Context): Boolean {
    val ask = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${context.packageName}"))
    val list = Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)
    for (intent in listOf(ask, list)) {
      if (context !is Activity) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      try {
        context.startActivity(intent)
        return true
      } catch (_: ActivityNotFoundException) {
        // The next one.
      } catch (_: SecurityException) {
        // The next one.
      }
    }
    return false
  }
}
