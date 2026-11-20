package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.Front
import ai.lucidloom.limo.child.ffi.PhoneFacts

/**
 * What the engine's host asks the watch: the facts of the phone for each
 * call into the engine, word when one of them changed, and the apps a child
 * can open. One per process: [Watch.of].
 *
 * **Threads.** Every call may be made from any thread; calls run one at a
 * time. [factsNow], [look] and [permissions] read Android through a handful
 * of binder calls, a few milliseconds in all, so the main thread is fine for
 * them. [launchableApps] loads every app's label and may take a few hundred
 * milliseconds: call it from a thread the host owns. A [WatchListener] is
 * called on the main thread, never while the watch holds its lock, so it may
 * call [factsNow] at once. The watch has no thread of its own.
 *
 * **Never throws.** A reading Android refuses becomes that fact's fallback
 * and a log line (tag `LiMoWatch`) that names at most a package, never a
 * label or anything on the screen.
 */
interface PhoneWatch {
  /**
   * The facts now, for one call into the engine, read from Android in one
   * place ([WatchCore.look]), each with its own fallback. [inReach] and
   * [prodded] are the link's, passed through as they are.
   */
  fun factsNow(inReach: Boolean, prodded: Boolean): PhoneFacts

  /** The same facts, with what the engine does not take: see [WatchLook]. */
  fun look(inReach: Boolean, prodded: Boolean): WatchLook

  /**
   * Tells [listener] when something the facts depend on changed, at once:
   * the app in front, the screen, a call, the clock, the zone, a permission,
   * the apps. Close the handle to stop. While anyone listens, the watch keeps
   * a few receivers and listeners registered with Android; with nobody
   * listening it holds none.
   */
  fun listen(listener: WatchListener): AutoCloseable

  /**
   * The apps a child can open, each with its label, sorted by label: the
   * list a guardian picks blocked and allowed apps from. LiMo itself is left
   * out. Slow (it loads every label): not on the main thread.
   */
  fun launchableApps(): List<LaunchableApp>

  /** Every permission LiMo needs, read now. */
  fun permissions(): LimoPermissions

  /**
   * LiMo's own activity came to the front (true, from its `onResume`) or
   * left it (false, from its `onPause`). Its screens are then LiMo at once,
   * even with usage access off.
   */
  fun limoScreenShown(shown: Boolean)

  /**
   * LiMo is about to open the emergency dialler (the lock's emergency call):
   * call it just before. Where Android did not name the emergency dialler,
   * the first screen to come to the front after it, if it has no icon and
   * comes within ten seconds, is learned as it, so it is never covered.
   * Only that first screen: whatever comes first ends the wait. Usage is
   * read at once, a few milliseconds.
   */
  fun emergencyDiallerOpening()
}

/** Told when something the facts depend on changed. Called on the main thread. */
fun interface WatchListener {
  fun changed(what: WatchChange)
}

/** What changed. The host calls [PhoneWatch.factsNow] for all of it. */
enum class WatchChange {
  /** The app in front, or the kind of screen in front. */
  FRONT,

  /** The screen went on or off, or Android's lock screen went away. */
  SCREEN,

  /** A call rang, started or ended. */
  CALL,

  /** The clock was set, or the 12 or 24 hour setting changed. */
  CLOCK,

  /** The time zone changed. */
  TIME_ZONE,

  /** A permission changed, or the accessibility service connected or stopped. */
  PERMISSIONS,

  /** An app was installed, removed or changed, or the default dialler changed. */
  APPS,
}

/** The facts, and what the engine does not take. */
data class WatchLook(
  /** For the engine. */
  val facts: PhoneFacts,
  /**
   * Other apps still on the screen beside or over the one in front: the
   * other half of split screen, a picture in picture window, an app under a
   * see through dialog. As far as Android's usage events tell, which is from
   * Android 10 on; empty before that and while usage access is off.
   */
  val alsoVisible: List<Front>,
  /** Every permission LiMo needs. */
  val permissions: LimoPermissions,
  /** Why the front is unknown, when it is. */
  val unknownBecause: UnknownBecause?,
)

/** Why the watch cannot say what is in front. */
enum class UnknownBecause {
  /** Usage access is switched off. */
  USAGE_ACCESS_OFF,

  /** Android refused to give the usage events. */
  USAGE_UNREADABLE,

  /** Usage shows no app yet: just after the phone started. */
  NOTHING_SEEN_YET,
}

/** Every permission LiMo needs, for setup and for the guardian, in the order setup asks for them. */
data class LimoPermissions(
  /** LiMo may show notifications (the time left). */
  val notifications: Boolean,
  /** Android's Phone permission (`READ_PHONE_STATE`) is granted: only ever to know that a call rings or is on. */
  val phone: Boolean,
  /** Usage access. */
  val usageAccess: Boolean,
  /** LiMo's accessibility service is switched on in Settings. */
  val accessibility: Boolean,
  /** The accessibility service is connected now, so covers can be drawn. */
  val accessibilityRunning: Boolean,
  /** LiMo is left out of battery optimisation. */
  val batteryExempt: Boolean,
  /** LiMo's device admin is active. */
  val deviceAdmin: Boolean,
)

/** An app a child can open. */
data class LaunchableApp(
  /** Its package name: what the rules name. */
  val packageName: String,
  /** Its label, the name under its icon; null when it has none. */
  val label: String?,
  /** It is the home app, a dialler or the emergency dialler: its time never counts. */
  val neverCounted: Boolean,
)
