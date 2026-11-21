package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.CallState
import ai.lucidloom.limo.child.ffi.FrontKind

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

  /**
   * A wait for the first screen to come up after LiMo opened the emergency
   * dialler, or after a call rang, was dialled or was answered, begun at
   * [sinceMs]; [forgets] is the record's count of fresh starts then.
   */
  private class Wait(val sinceMs: Long, val forgets: Long)

  val track = UsageTrack()

  var usage = UsageState.NOT_READ
    private set

  private var pending: Pending? = null
  private var latestProposal: String? = null

  /**
   * The package that last sent a window event, LiMo's own, keyboards' and
   * the framework's overlays left out: what is in front as far as window
   * events tell, for the guard while usage access is off.
   */
  val lastWindow: String? get() = latestProposal
  private var ownScreenSince: Long? = null

  /** LiMo's own screens say one of them is shown ([limoScreen]): a fact, whatever usage shows. */
  val limoShown: Boolean get() = ownScreenSince != null
  private val learnedCall = LinkedHashSet<String>()
  private val learnedEmergency = LinkedHashSet<String>()
  private var call = CallState.IDLE
  private var emergencyWait: Wait? = null
  private var callWait: Wait? = null

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
    track.take(events, window, Overlays::isOverlay)
    awaitEmergency(nowMs, roles)
    awaitCallScreen(nowMs, roles)
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
    endCallWait("no_usage")
  }

  /** Usage could not be read: the call failed. */
  fun usageUnreadable() {
    usage = UsageState.UNREADABLE
    pending = null
    endEmergencyWait("no_usage")
    endCallWait("no_usage")
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
    emergencyWait = Wait(nowMs, track.forgets)
  }

  /**
   * The call state now. True when a call began to ring, was dialled or was
   * answered (any change to ringing or on): the caller then reads usage at
   * once and calls [expectCallScreen]. A call ending ends the wait for its
   * screen.
   */
  fun callState(state: CallState): Boolean {
    if (state == call) return false
    call = state
    log.line("call now ${state.name.lowercase()}")
    if (state == CallState.IDLE) {
      endCallWait("call_ended")
      return false
    }
    return true
  }

  /**
   * A call rang, was dialled or was answered, at [nowMs], and usage was
   * read just now, so whatever came to the front before it is taken. The
   * wait for the first screen after it starts here ([awaitCallScreen]),
   * unless usage could not be read (the next read could not tell what came
   * first).
   */
  fun expectCallScreen(nowMs: Long) {
    callWait = null
    if (usage != UsageState.ON) {
      log.line("front call_wait not_started because=no_usage")
      return
    }
    callWait = Wait(nowMs, track.forgets)
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
    overBy(wait, nowMs, WatchTimes.EMERGENCY_WINDOW_MS, "ten_seconds")?.let { return endEmergencyWait(it) }
    val first = track.lastTaken.firstOrNull() ?: return
    emergencyWait = null
    firstScreen(first, roles, learnedEmergency, "emergency_dialler", "emergency_wait")
  }

  private fun endEmergencyWait(because: String) {
    if (emergencyWait == null) return
    emergencyWait = null
    log.line("front emergency_wait ended because=$because")
  }

  /**
   * **A call's own screen, learned where Android does not name it**: the
   * first activity to come to the front after a call rang, was dialled or
   * was answered, and nothing after, so it stays the dialler after the call
   * too and a call that later rings silently is never covered. Whatever
   * comes first ends the wait, as for the emergency dialler: it is learned
   * only when it is none of the known kinds and has no icon. Five seconds
   * with nothing come up end it too, and so do the call ending, the record
   * read again from the start and a read of usage that failed. What came up
   * before the call is never learned: usage is read when the call is seen,
   * before the wait starts.
   */
  private fun awaitCallScreen(nowMs: Long, roles: PhoneRoles) {
    val wait = callWait ?: return
    overBy(wait, nowMs, WatchTimes.CALL_SCREEN_WINDOW_MS, "five_seconds")?.let { return endCallWait(it) }
    val first = track.lastTaken.firstOrNull() ?: return
    callWait = null
    firstScreen(first, roles, learnedCall, "call_screen", "call_wait")
  }

  private fun endCallWait(because: String) {
    if (callWait == null) return
    callWait = null
    log.line("front call_wait ended because=$because")
  }

  /** Why [wait] is over at this read with nothing taken for it, or null while it goes on. */
  private fun overBy(wait: Wait, nowMs: Long, windowMs: Long, late: String): String? = when {
    track.forgets != wait.forgets -> "clock_set"
    nowMs - wait.sinceMs > windowMs -> late
    else -> null
  }

  /** The first screen after a wait began: learned into [into] when unexplained, logged either way. */
  private fun firstScreen(first: String, roles: PhoneRoles, into: LinkedHashSet<String>, kind: String, waitName: String) {
    if (unexplained(first, roles)) {
      remember(into, first)
      log.line("front learned $kind package=$first")
    } else {
      log.line("front $waitName ended first=$first")
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
