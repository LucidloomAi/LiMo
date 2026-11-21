package ai.lucidloom.limo.child.dev

import ai.lucidloom.limo.child.watch.GuardReach
import ai.lucidloom.limo.child.watch.IntentAnswer
import ai.lucidloom.limo.child.watch.NamedActivity
import ai.lucidloom.limo.child.watch.PageIntent
import android.util.Log

/**
 * Debug builds only (src/debug). The probe: while a person at the PC has
 * switched it on (the broadcast [DevReceiver.ACTION_PROBE] with `on` true),
 * LiMo logs, tag `LiMoWatch`, what the guard reads and asks:
 *
 *     probe answered intent=device_admin opens=com.android.settings/com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd preinstalled
 *     probe window package=com.android.settings class=com.android.settings.SubSettings
 *     probe resumed package=com.android.settings class=com.android.settings.SubSettings
 *
 * so that the dangerous pages can be walked on a real phone once, and the
 * package manager's answer (watch/Guarded.kt, `GuardReach`) checked against
 * the real names of the windows and of the activities that resume. It sees
 * exactly what a release build reads and matches, nothing more: a window's
 * class only for the packages the guard reads windows of
 * (`GuardReach.readsClassName`), a resumed activity's only for those it
 * reads Android's usage record of (`GuardReach.readsResumedClassName`),
 * never another app's class name, never text, never anything on the
 * screen. Off by default, and off again whenever LiMo's process starts,
 * since the switch is held in memory only. A release build has none of
 * this: its watch has its own `WindowPage`, with no probe, and `npm run
 * inspect` checks that no trace of it is in the APK.
 */
object PageProbe {
  /** Where the lines go: Android's log, tag `LiMoWatch`; the JVM tests put their own here. */
  internal var sink: (String) -> Unit = { Log.i("LiMoWatch", it) }

  /** Switched on by a person at the PC; off at every start of LiMo's process. */
  @Volatile
  var on: Boolean = false
    private set

  /** The resumed activities logged since the probe was switched on: each once, since usage is read again every half second. */
  private val resumedSeen = HashSet<String>()

  /** On or off, with one line saying so. */
  fun switch(on: Boolean) {
    synchronized(resumedSeen) { resumedSeen.clear() }
    this.on = on
    sink(if (on) "probe on" else "probe off")
  }

  /** A window event of [packageName] with [className]: logged while the probe is on, for the packages the guard reads windows of only. */
  internal fun window(reach: GuardReach, packageName: String?, className: String?) {
    if (!on || !reach.readsClassName(packageName)) return
    sink("probe window package=$packageName class=${shown(className)}")
  }

  /**
   * An activity of [packageName] with [className] resumed, in Android's
   * usage record: logged once while the probe is on, for the packages the
   * guard reads that record of only.
   */
  internal fun resumed(reach: GuardReach, packageName: String?, className: String?) {
    if (!on || !reach.readsResumedClassName(packageName)) return
    val line = "probe resumed package=$packageName class=${shown(className)}"
    if (synchronized(resumedSeen) { resumedSeen.add(line) }) sink(line)
  }

  /** What the package manager answered for each of the guard's intents (the watch's last answers), while the probe is on. */
  internal fun answered(answers: Map<PageIntent, IntentAnswer>) {
    if (!on) return
    for (intent in PageIntent.values()) {
      val name = intent.name.lowercase()
      val answer = answers[intent]
      if (answer == null) {
        sink("probe answered intent=$name unread")
        continue
      }
      sink("probe answered intent=$name opens=${answer.opens?.let(::shown) ?: "none"}")
      for (handler in answer.handlers) sink("probe answered intent=$name handler=${shown(handler)}")
    }
  }

  private fun shown(activity: NamedActivity): String {
    val target = activity.aliasTarget?.let { " target=${shown(it)}" }.orEmpty()
    return "${activity.packageName}/${shown(activity.className)}$target ${if (activity.preinstalled) "preinstalled" else "installed"}"
  }

  /** A class name cut at [MAX_CLASS_CHARS], with only what a class name can hold. */
  private fun shown(className: String?): String =
    className?.take(MAX_CLASS_CHARS)?.filter { it.isLetterOrDigit() || it in "._$" }?.ifEmpty { null } ?: "none"

  /** A class name is a few dozen characters; anything longer is cut. */
  private const val MAX_CLASS_CHARS = 200
}
