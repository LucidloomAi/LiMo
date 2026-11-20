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
 * step ("Android asks once. Choose Allow."). The seam (folder seam/) opens
 * [question] from `child.openStep` and waits for its answer, and [list]
 * where a phone has no such question.
 *
 * Why it matters: Android may stop or hold back an app that is optimised,
 * and from Android 12 an app left out of optimisation may start its
 * foreground service from the background, so LiMo's alarm can bring it back
 * after it was stopped. Whether it is left out is read by the watch
 * (`permissions().batteryExempt`).
 */
object Battery {
  /**
   * Android's own question whether LiMo may run in the background
   * (`ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`, which the manifest's
   * `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` lets it ask). It answers with a
   * result when opened for one.
   */
  // LiMo's whole work is keeping limits while the child uses the phone:
  // being stopped in the background is what it must not be.
  @SuppressLint("BatteryLife")
  @JvmStatic
  fun question(context: Context): Intent =
    Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${context.packageName}"))

  /** Android's list of apps left out of battery optimisation, where a phone has no question. */
  @JvmStatic
  fun list(): Intent = Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)

  /**
   * Shows [question], or [list] where a phone has no such screen, without
   * waiting for an answer. True when one of them opened.
   */
  @JvmStatic
  fun askToLeaveOutOfOptimisation(context: Context): Boolean {
    for (intent in listOf(question(context), list())) {
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
