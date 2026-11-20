package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.CallState
import ai.lucidloom.limo.child.ffi.FrontKind
import kotlin.math.abs

/** What is in front, as the watch decided it: a kind, the package behind it, and why it is unknown when it is. */
internal data class Pick(
  val kind: FrontKind,
  val packageName: String?,
  val unknownBecause: UnknownBecause? = null,
)

/** Whether usage could be read at the last try. */
internal enum class UsageState { NOT_READ, ON, OFF, UNREADABLE }

/**
 * **Which app is in front: the state machine.** Accessibility events
 * propose, usage events confirm.
 *
 * - A window event (the accessibility service's, window state changes only)
 *   proposes its package. Events from LiMo itself (its covers are its own
 *   windows), from a keyboard and from the framework's overlays propose
 *   nothing. A proposal is never taken as it is: it asks for usage to be
 *   read now, and again after 100, 300 and 1,000 ms until usage shows that
 *   package's activity resumed. One not confirmed within a second is
 *   dropped: a late event, a floating window, a dialog.
 * - The front is the app whose activity resumed last in Android's usage
 *   events ([UsageTrack]). So proof two's late window event, sent by a
 *   blocked app after the home screen came up, never puts it back in front.
 * - What kind of screen that package is: [kindOf].
 * - **Usage access off**: the front is unknown, never a guess, with one
 *   exception, because a lock must never stand between a child and a call:
 *   a window event from the dialler or the emergency dialler still says so.
 *   That exception can only take a cover away, never put one up. The same
 *   holds while usage has shown nothing yet.
 * - LiMo's own screens say so themselves ([limoScreen]), from their activity,
 *   which is a fact, not a guess, and holds with usage access off too.
 *
 * Pure: no Android here. [WatchCore] reads Android and feeds it.
 */
internal class FrontWatch(private val ownPackage: String, private val log: WatchLog) {
  private class Pending(val packageName: String, val sinceMs: Long)

  /** LiMo opened the emergency dialler at [sinceMs]; [forgets] is the record's count of fresh starts then. */
  private class EmergencyWait(val sinceMs: Long, val forgets: Long)

  val track = UsageTrack()

  var usage = UsageState.NOT_READ
    private set

  private var pending: Pending? = null
  private var latestProposal: String? = null
  private var ownScreenSince: Long? = null
  private val learnedCall = LinkedHashSet<String>()
  private val learnedEmergency = LinkedHashSet<String>()
  private var call = CallState.IDLE
  private var callChangedAtMs: Long? = null
  private var frontChangedAtMs: Long? = null
  private var emergencyWait: EmergencyWait? = null

  /**
   * A window event from [packageName] at [nowMs] (elapsed). True when usage
   * should be read now to confirm it.
   */
  fun proposal(packageName: String?, nowMs: Long, roles: PhoneRoles): Boolean {
    val pkg = packageName?.trim().orEmpty()
    if (pkg.isEmpty() || pkg == ownPackage || Overlays.isOverlay(pkg) || pkg in roles.keyboards) return false
    latestProposal = pkg
    // Already in front: nothing new to confirm. A pending proposal of another
    // app is left as it is, so its rechecks still run.
    if (usage == UsageState.ON && pkg == track.front) return false
    // With usage off this read only finds out whether it is back on: a
    // proposal waits for nothing then (usageOff drops it).
    pending = Pending(pkg, nowMs)
    return true
  }

  /** Usage was read: [events] for [window], at [nowMs] (elapsed). */
  fun usageTaken(events: List<UsageEvent>, window: UsageTrack.Window, nowMs: Long, roles: PhoneRoles) {
    usage = UsageState.ON
    val moved = track.take(events, window, Overlays::isOverlay)
    awaitEmergency(nowMs, roles)
    if (moved) {
      frontChangedAtMs = nowMs
      learnCallScreen(roles)
    }
    settle(nowMs)
    val since = ownScreenSince
    // LiMo said its screen was shown, and another app resumed since: its
    // word that the screen left was missed.
    if (since != null && track.resumes > since && track.front != ownPackage) ownScreenSince = null
  }

  /** Usage access is off: Android gives LiMo no usage events. */
  fun usageOff() {
    usage = UsageState.OFF
    pending = null
    endEmergencyWait("no_usage")
  }

  /** Usage could not be read: the call failed. */
  fun usageUnreadable() {
    usage = UsageState.UNREADABLE
    pending = null
    endEmergencyWait("no_usage")
  }

  /** LiMo's own activity came to the front (true) or left it (false). */
  fun limoScreen(shown: Boolean) {
    ownScreenSince = if (shown) track.resumes else null
  }

  /**
   * LiMo is about to open the emergency dialler, at [nowMs], and usage was
   * read just now, so whatever came to the front before the tap is taken.
   * The wait for the first screen after it starts here ([awaitEmergency]),
   * unless usage could not be read (the next read could not tell what came
   * first), or Android names no dialler and no emergency dialler (a phone
   * or tablet with no calls, where Emergency call opens nothing, so the
   * first screen after it would be whatever the child brings up).
   */
  fun expectEmergency(nowMs: Long, roles: PhoneRoles) {
    emergencyWait = null
    val why = when {
      usage != UsageState.ON -> "no_usage"
      roles.diallers.isEmpty() && roles.emergency.isEmpty() -> "no_dialler"
      else -> null
    }
    if (why != null) {
      log.line("front emergency_wait not_started because=$why")
      return
    }
    emergencyWait = EmergencyWait(nowMs, track.forgets)
  }

  /** The call state now, at [nowMs]. */
  fun callState(state: CallState, nowMs: Long, roles: PhoneRoles) {
    if (state == call) return
    call = state
    callChangedAtMs = nowMs
    log.line("call now ${state.name.lowercase()}")
    learnCallScreen(roles)
  }

  /** In how many milliseconds usage should be read again to settle a window event; null when none waits. */
  fun nextLookInMs(nowMs: Long): Long? {
    val waiting = pending ?: return null
    val age = nowMs - waiting.sinceMs
    for (at in WatchTimes.RECHECKS_MS) if (age < at) return at - age
    return 0
  }

  /** What is in front now. */
  fun pick(roles: PhoneRoles): Pick {
    if (ownScreenSince != null) return Pick(FrontKind.LIMO, ownPackage)
    val why = when (usage) {
      UsageState.ON -> null
      UsageState.OFF -> UnknownBecause.USAGE_ACCESS_OFF
      UsageState.UNREADABLE -> UnknownBecause.USAGE_UNREADABLE
      UsageState.NOT_READ -> UnknownBecause.NOTHING_SEEN_YET
    }
    val front = if (why == null) track.front else null
    if (front != null) return Pick(kindOf(front, roles), front)
    // Unknown, except that a window event from a dialler still says so.
    val proposed = latestProposal
    if (proposed != null) {
      val kind = kindOf(proposed, roles)
      if (kind == FrontKind.DIALLER || kind == FrontKind.EMERGENCY_DIALLER) return Pick(kind, proposed)
    }
    return Pick(FrontKind.UNKNOWN, null, why ?: UnknownBecause.NOTHING_SEEN_YET)
  }

  /** Other apps still on the screen beside or over the front, each with its kind. */
  fun alsoVisible(roles: PhoneRoles, stopsReported: Boolean): List<Pick> {
    if (usage != UsageState.ON) return emptyList()
    val front = track.front
    return track.visible(stopsReported)
      .filter { it != front && it != ownPackage && !Overlays.isOverlay(it) }
      .sorted()
      .map { Pick(kindOf(it, roles), it) }
  }

  /**
   * **What kind of screen a package is**, in this order:
   *
   * 1. LiMo's own package: LiMo.
   * 2. The home app: the home screen.
   * 3. The default dialler, the system's dialler, a preinstalled in call
   *    screen, or a screen learned to be a call's: the dialler.
   * 4. The emergency dialler Android names, or one learned when LiMo opened
   *    it: the emergency dialler.
   * 5. During a call, a package with no icon to open it: the call's own
   *    screen (on some makers it is another package than the dialler), so
   *    the dialler. An app with an icon is an app, call or no call: a call
   *    left open is no way around the limits.
   * 6. Anything else: an app.
   */
  fun kindOf(packageName: String, roles: PhoneRoles): FrontKind = when {
    packageName == ownPackage -> FrontKind.LIMO
    packageName in roles.home -> FrontKind.HOME
    packageName in roles.diallers || packageName in learnedCall -> FrontKind.DIALLER
    packageName in roles.emergency || packageName in learnedEmergency -> FrontKind.EMERGENCY_DIALLER
    call != CallState.IDLE && hasNoIcon(packageName, roles) -> FrontKind.DIALLER
    else -> FrontKind.APP
  }

  /** True only when Android said which apps have an icon and this one has none. */
  private fun hasNoIcon(packageName: String, roles: PhoneRoles): Boolean =
    roles.launchable?.let { packageName !in it } == true

  /** A package that is none of the known kinds and has no icon: one that may be learned. */
  private fun unexplained(packageName: String, roles: PhoneRoles): Boolean =
    packageName != ownPackage &&
      packageName !in roles.home &&
      packageName !in roles.diallers &&
      packageName !in roles.emergency &&
      packageName !in learnedCall &&
      packageName !in learnedEmergency &&
      hasNoIcon(packageName, roles)

  /**
   * **The emergency dialler, learned where Android does not name it**: the
   * first activity to come to the front after LiMo opened it, and nothing
   * after. Whatever comes first ends the wait: it is learned only when it is
   * none of the known kinds and has no icon; the emergency dialler Android
   * names, the dialler, the home screen, LiMo or an app with an icon end it
   * with nothing learned (the child went somewhere else). So do ten seconds
   * with nothing come up, the record read again from the start (the clock
   * was set, and times before and after it cannot be compared) and a read
   * of usage that failed ([usageOff], [usageUnreadable]).
   */
  private fun awaitEmergency(nowMs: Long, roles: PhoneRoles) {
    val wait = emergencyWait ?: return
    if (track.forgets != wait.forgets) return endEmergencyWait("clock_set")
    if (nowMs - wait.sinceMs > WatchTimes.EMERGENCY_WINDOW_MS) return endEmergencyWait("ten_seconds")
    val first = track.lastTaken.firstOrNull() ?: return
    emergencyWait = null
    if (unexplained(first, roles)) {
      remember(learnedEmergency, first)
      log.line("front learned emergency_dialler package=$first")
    } else {
      log.line("front emergency_wait ended first=$first")
    }
  }

  private fun endEmergencyWait(because: String) {
    if (emergencyWait == null) return
    emergencyWait = null
    log.line("front emergency_wait ended because=$because")
  }

  /**
   * Learns the screen of a call when Android did not name it: a package
   * with no icon that came to the front within five seconds of a call
   * starting or being answered is that call's screen from then on, so it
   * stays the dialler when a call rings silently. An app with an icon is
   * never learned.
   */
  private fun learnCallScreen(roles: PhoneRoles) {
    val pkg = track.front ?: return
    val changedAt = frontChangedAtMs ?: return
    if (!unexplained(pkg, roles)) return
    val callAt = callChangedAtMs ?: return
    if (call != CallState.IDLE && abs(changedAt - callAt) <= WatchTimes.CALL_SCREEN_WINDOW_MS) {
      remember(learnedCall, pkg)
      log.line("front learned call_screen package=$pkg")
    }
  }

  private fun remember(set: LinkedHashSet<String>, pkg: String) {
    set.add(pkg)
    while (set.size > WatchTimes.MAX_LEARNED) set.remove(set.first())
  }

  private fun settle(nowMs: Long) {
    val waiting = pending ?: return
    if (track.front == waiting.packageName) {
      pending = null
    } else if (nowMs - waiting.sinceMs >= WatchTimes.CONFIRM_GRACE_MS) {
      pending = null
      log.line("front window_event package=${waiting.packageName} not_confirmed front=${track.front ?: "none"}")
    }
  }
}
