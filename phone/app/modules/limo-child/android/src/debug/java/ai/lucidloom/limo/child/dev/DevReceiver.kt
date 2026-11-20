package ai.lucidloom.limo.child.dev

import ai.lucidloom.limo.child.host.Host
import ai.lucidloom.limo.child.keep.Keeper
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import java.io.File

/**
 * Debug builds only (src/debug). The development tool on the PC talks to
 * the app here over the USB cable, before the link exists, with three
 * broadcasts:
 *
 * - [ACTION_PAIR]: take the test family's pairing file
 *   (`files/limo-dev/pairing.json`, see [DevPairing]) once, delete it,
 *   make the engine from it, keep its state and the child's name, and
 *   start the keeper;
 * - [ACTION_FORGET]: wipe the kept state and stop the keeper;
 * - [ACTION_PROBE] with the boolean extra `on`: switch the probe of
 *   Settings' and the installer's page names on or off ([PageProbe]). A
 *   person at the PC sends it by hand (see dev/README.md).
 *
 * Only the adb shell and the system can send to it (it is guarded by
 * android.permission.DUMP, see src/debug/AndroidManifest.xml), and a release
 * build does not contain it. Its log lines (tag `LiMo`) say what happened in
 * a word, never what the file held.
 */
class DevReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val app = context.applicationContext ?: context
    val pending = goAsync()
    try {
      when (intent.action) {
        ACTION_PAIR -> pair(app) { pending.finish() }
        ACTION_FORGET -> forget(app) { pending.finish() }
        ACTION_PROBE -> {
          PageProbe.switch(intent.getBooleanExtra(EXTRA_ON, false))
          Log.i(TAG, if (PageProbe.on) "dev probe on" else "dev probe off")
          pending.finish()
        }
        else -> pending.finish()
      }
    } catch (e: Exception) {
      Log.i(TAG, "dev failed error=${e.javaClass.simpleName}")
      pending.finish()
    }
  }

  private fun pair(app: Context, done: () -> Unit) {
    when (val taken = DevPairing.take(pairingFile(app), System.currentTimeMillis().floorDiv(1_000L))) {
      DevPairing.Taken.NoFile -> {
        Log.i(TAG, "dev pair no_file")
        done()
      }
      is DevPairing.Taken.Refused -> {
        Log.i(TAG, "dev pair refused why=${taken.why}")
        done()
      }
      is DevPairing.Taken.Pairing -> Host.of(app).pair(taken.welcome, taken.codeSecret, taken.childName) { ok ->
        Log.i(TAG, if (ok) "dev pair taken" else "dev pair refused why=engine")
        if (ok) Keeper.start(app, "dev_pair")
        done()
      }
    }
  }

  private fun forget(app: Context, done: () -> Unit) {
    Host.of(app).forget {
      Keeper.stop(app)
      Log.i(TAG, "dev forget done")
      done()
    }
  }

  companion object {
    private const val TAG = "LiMo"

    /** Take the pairing file. */
    const val ACTION_PAIR = "ai.lucidloom.limo.child.dev.PAIR"

    /** Wipe the kept state. */
    const val ACTION_FORGET = "ai.lucidloom.limo.child.dev.FORGET"

    /** Switch the probe on or off, by the boolean extra [EXTRA_ON]; with none, off. */
    const val ACTION_PROBE = "ai.lucidloom.limo.child.dev.PROBE"

    /** The probe's switch: true for on. */
    const val EXTRA_ON = "on"

    /** Where the development tool puts the pairing file, through `run-as`. */
    internal fun pairingFile(context: Context): File = File(context.filesDir, "limo-dev/pairing.json")
  }
}
