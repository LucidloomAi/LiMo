package ai.lucidloom.limo.child.dev

import ai.lucidloom.limo.child.watch.Guarded
import android.util.Log

/**
 * Debug builds only (src/debug). The probe: while a person at the PC has
 * switched it on (the broadcast [DevReceiver.ACTION_PROBE] with `on` true),
 * LiMo logs the package and the class name of each window event of
 * Android's Settings and the package installer, tag `LiMoWatch`:
 *
 *     probe window package=com.android.settings class=com.android.settings.SubSettings
 *
 * so that the dangerous pages can be walked on a real phone once and their
 * real names learned for the guard's list (watch/Guarded.kt). It sees
 * exactly the events whose class name a release build reads and matches
 * (`Guarded.readsClassName`), nothing more: never another app's class name,
 * never text, never anything on the screen. Off by default, and off again
 * whenever LiMo's process starts, since the switch is held in memory only.
 * A release build has none of this: its watch has its own `WindowPage`, with
 * no probe, and `npm run inspect` checks that no trace of it is in the APK.
 */
object PageProbe {
  /** Where the lines go: Android's log, tag `LiMoWatch`; the JVM tests put their own here. */
  internal var sink: (String) -> Unit = { Log.i("LiMoWatch", it) }

  /** Switched on by a person at the PC; off at every start of LiMo's process. */
  @Volatile
  var on: Boolean = false
    private set

  /** On or off, with one line saying so. */
  fun switch(on: Boolean) {
    this.on = on
    sink(if (on) "probe on" else "probe off")
  }

  /** A window event of [packageName] with [className]: logged while the probe is on, for Settings and the installer only. */
  fun window(packageName: String?, className: String?) {
    if (!on || !Guarded.readsClassName(packageName)) return
    val name = className?.take(MAX_CLASS_CHARS)?.filter { it.isLetterOrDigit() || it in "._$" }
    sink("probe window package=$packageName class=${name?.ifEmpty { null } ?: "none"}")
  }

  /** A class name is a few dozen characters; anything longer is cut. */
  private const val MAX_CLASS_CHARS = 200
}
