package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.CallState
import ai.lucidloom.limo.child.ffi.Front
import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.ffi.PhoneFacts
import ai.lucidloom.limo.child.ffi.PhonePermissions

/**
 * The watch's decisions, with no Android in them: every fact read through
 * [PhoneSource], each with its fallback, the app in front ([FrontWatch]),
 * the roles, the labels and the permissions, and **the one place a
 * [PhoneFacts] is assembled** ([look]).
 *
 * Not thread safe: [Watch] calls it under one lock.
 *
 * **Fallbacks.** Every reading is tried; one that throws gives the last good
 * reading of the same fact in this process, or, with none, the default in
 * this table, and a log line (at most one a minute per fact) naming the fact
 * and the kind of failure, never its message.
 *
 * | Fact | Default with no good reading |
 * | --- | --- |
 * | wall clock, elapsed count | 0 (these calls do not fail) |
 * | boot count | 0 |
 * | time zone | UTC |
 * | 24 hours | false (12 hours) |
 * | automatic time | false: a clock set by hand buys nothing |
 * | screen on | true |
 * | Android's lock screen | false |
 * | call | idle, from the audio mode and the telephony state, either alone |
 * | every permission | false |
 * | the front | unknown, why in [WatchLook.unknownBecause] |
 * | a label | none |
 * | the roles | as last asked (none at first) |
 */
internal class WatchCore(private val phone: PhoneSource, private val log: WatchLog) {
  private class Remembered<T : Any>(val fact: String, val default: T) {
    var good: T? = null
  }

  private val front = FrontWatch(phone.ownPackage, log)
  private val labels = LabelCache()
  private val failedAt = HashMap<String, Long>()

  private val wall = Remembered("wall_clock", 0L)
  private val elapsed = Remembered("elapsed", 0L)
  private val boot = Remembered("boot_count", 0)
  private val offset = Remembered("utc_offset", 0)
  private val hours24 = Remembered("hours_24", false)
  private val autoTime = Remembered("auto_time", false)
  private val interactive = Remembered("interactive", true)
  private val keyguard = Remembered("keyguard_locked", false)
  private val usageAccess = Remembered("usage_access", false)
  private val accessibility = Remembered("accessibility", false)
  private val notifications = Remembered("notifications", false)
  private val phoneGranted = Remembered("phone_permission", false)
  private val batteryExempt = Remembered("battery_exempt", false)
  private val deviceAdmin = Remembered("device_admin", false)
  private val home = Remembered("home", emptySet<String>())
  private val diallers = Remembered("diallers", emptySet<String>())
  private val inCall = Remembered("in_call_screens", emptySet<String>())
  private val emergency = Remembered("emergency_dialler", emptySet<String>())
  private val keyboards = Remembered("keyboards", emptySet<String>())
  private var callGood: CallState? = null

  private val startedAtMs = elapsedNow()
  private var roles = PhoneRoles()
  private var rolesAtMs: Long? = null
  private var launchableAtMs: Long? = null
  private var usageReadAtMs: Long? = null
  private var permissionsSeen: LimoPermissions? = null
  private var permissionsAtMs: Long? = null
  private var serviceRunning = false
  private var lastPick: Pick? = null

  /**
   * **The facts now: the one place they are assembled.** Reads every fact,
   * reads usage when the last read is half a second old or more, and says
   * what is in front.
   */
  fun look(inReach: Boolean, prodded: Boolean): WatchLook {
    val now = elapsedNow()
    val wallMs = remembered(wall, now) { phone.wallMs() }
    refreshRoles(now)
    val call = callNow(now)
    val read = usageReadAtMs
    if (read == null || now - read >= WatchTimes.USAGE_FRESH_MS) readUsage(now, wallMs)
    val permissions = permissionsNow(now)
    val pick = front.pick(roles)
    if (pick != lastPick) logFront(pick)
    lastPick = pick
    val facts = PhoneFacts(
      wallSecs = wallMs.floorDiv(1_000L),
      elapsedMs = now,
      bootCount = remembered(boot, now) { phone.bootCount() }.coerceAtLeast(0),
      utcOffsetSecs = remembered(offset, now) { phone.utcOffsetSecs(wallMs) },
      hours24 = remembered(hours24, now) { phone.hours24() },
      autoTime = remembered(autoTime, now) { phone.autoTime() },
      interactive = remembered(interactive, now) { phone.interactive() },
      keyguardLocked = remembered(keyguard, now) { phone.keyguardLocked() },
      call = call,
      front = toFront(pick),
      inReach = inReach,
      prodded = prodded,
      permissions = enginePermissions(permissions, now),
    )
    val beside = front.alsoVisible(roles, phone.stopsReported).map(::toFront)
    return WatchLook(facts, beside, permissions, pick.unknownBecause)
  }

  /**
   * A look made of what was last read, with no reading at all: for the
   * host when something unforeseen went wrong in [look].
   */
  fun lastLook(inReach: Boolean, prodded: Boolean): WatchLook {
    val permissions = permissionsSeen ?: LimoPermissions(
      notifications = false,
      phone = false,
      usageAccess = false,
      accessibility = false,
      accessibilityRunning = serviceRunning,
      batteryExempt = false,
      deviceAdmin = false,
    )
    val facts = PhoneFacts(
      wallSecs = (wall.good ?: wall.default).floorDiv(1_000L),
      elapsedMs = elapsed.good ?: elapsed.default,
      bootCount = (boot.good ?: boot.default).coerceAtLeast(0),
      utcOffsetSecs = offset.good ?: offset.default,
      hours24 = hours24.good ?: hours24.default,
      autoTime = autoTime.good ?: autoTime.default,
      interactive = interactive.good ?: interactive.default,
      keyguardLocked = keyguard.good ?: keyguard.default,
      call = callGood ?: CallState.IDLE,
      front = Front(FrontKind.UNKNOWN),
      inReach = inReach,
      prodded = prodded,
      permissions = toEngine(permissions, accessibility = permissions.accessibility && permissions.accessibilityRunning),
    )
    return WatchLook(facts, emptyList(), permissions, UnknownBecause.USAGE_UNREADABLE)
  }

  /** A window event from the accessibility service. True when what is in front changed. */
  fun windowEvent(packageName: String?): Boolean {
    val now = elapsedNow()
    refreshRoles(now)
    if (front.proposal(packageName, now, roles)) {
      callNow(now)
      readUsage(now, remembered(wall, now) { phone.wallMs() })
    }
    return pickChanged()
  }

  /** Usage read again to settle a window event. True when what is in front changed. */
  fun recheck(): Boolean {
    val now = elapsedNow()
    callNow(now)
    readUsage(now, remembered(wall, now) { phone.wallMs() })
    return pickChanged()
  }

  /** In how many milliseconds [recheck] is wanted; null when nothing waits. */
  fun nextLookInMs(): Long? = front.nextLookInMs(elapsedNow())

  /** A call rang, started or ended. True when what is in front changed with it. */
  fun callChanged(): Boolean {
    callNow(elapsedNow())
    return pickChanged()
  }

  /**
   * The clock was set. Android moves its stored usage events with the clock,
   * so the record is read again from the start. True when what is in front
   * changed.
   */
  fun clockSet(): Boolean {
    val now = elapsedNow()
    front.track.forget()
    readUsage(now, remembered(wall, now) { phone.wallMs() })
    return pickChanged()
  }

  /** LiMo's own activity came to the front or left it. True when what is in front changed. */
  fun limoScreen(shown: Boolean): Boolean {
    front.limoScreen(shown)
    return pickChanged()
  }

  /** LiMo is about to open the emergency dialler. */
  fun emergencyOpening() {
    front.expectEmergency(elapsedNow())
  }

  /** The accessibility service connected (true) or stopped (false). */
  fun serviceConnected(running: Boolean) {
    serviceRunning = running
    permissionsAtMs = null
  }

  /** A permission may have changed: read them again at the next look. */
  fun permissionsChanged() {
    permissionsAtMs = null
  }

  /** An app was installed, removed or changed: ask the roles again, and forget the labels. */
  fun appsChanged() {
    rolesAtMs = null
    launchableAtMs = null
    labels.clear()
  }

  /** Every permission, read now. */
  fun permissions(): LimoPermissions = permissionsNow(elapsedNow(), force = true)

  /**
   * The apps a child can open, from [raw] (package to label, as
   * [PhoneSource.launchableApps] gives them): LiMo left out, labels
   * cleaned, sorted by label with the unlabelled last.
   */
  fun shapeApps(raw: Map<String, String?>): List<LaunchableApp> {
    refreshRoles(elapsedNow())
    return raw
      .filterKeys { it != phone.ownPackage }
      .map { (pkg, label) ->
        LaunchableApp(
          packageName = pkg,
          label = cleanLabel(label, pkg),
          neverCounted = pkg in roles.home || pkg in roles.diallers || pkg in roles.emergency,
        )
      }
      .sortedWith(compareBy<LaunchableApp>({ it.label == null }, { it.label?.lowercase() }, { it.packageName }))
  }

  private fun pickChanged(): Boolean {
    val pick = front.pick(roles)
    val changed = pick != lastPick
    if (changed) logFront(pick)
    lastPick = pick
    return changed
  }

  private fun logFront(pick: Pick) {
    val kind = pick.kind.name.lowercase()
    val pkg = pick.packageName?.let { " package=$it" }.orEmpty()
    val why = pick.unknownBecause?.let { " because=${it.name.lowercase()}" }.orEmpty()
    log.line("front now $kind$pkg$why")
  }

  private fun toFront(pick: Pick): Front {
    val pkg = pick.packageName
    return when {
      pick.kind != FrontKind.APP -> Front(pick.kind)
      pkg == null -> Front(FrontKind.UNKNOWN)
      else -> Front(FrontKind.APP, pkg, label(pkg))
    }
  }

  private fun label(pkg: String): String? =
    labels.get(pkg) { cleanLabel(optional("label") { phone.label(it) }, it) }

  private fun readUsage(now: Long, wallMs: Long) {
    usageReadAtMs = now
    val window = front.track.window(wallMs)
    val events = try {
      phone.usageEvents(window.fromWallMs, window.toWallMs)
    } catch (e: Exception) {
      failed("usage_events", e)
      front.usageUnreadable()
      return
    } catch (e: LinkageError) {
      failed("usage_events", e)
      front.usageUnreadable()
      return
    }
    if (events == null) front.usageOff() else front.usageTaken(events, window, now, roles)
    // Usage told whether access is on: when that differs from the
    // permission last read, read the permissions again.
    val seen = permissionsSeen
    if (seen != null && seen.usageAccess != (events != null)) permissionsAtMs = null
  }

  private fun callNow(now: Long): CallState {
    val audio = optional("audio_mode") { Calls.fromAudioMode(phone.audioMode()) }
    val telephony = if (telephonyAllowed(now)) {
      optional("telephony_call_state") { Calls.fromTelephony(phone.telephonyCallState()) }
    } else {
      null
    }
    val state = Calls.both(audio, telephony) ?: callGood ?: CallState.IDLE
    if (audio != null || telephony != null) callGood = state
    front.callState(state, now, roles)
    return state
  }

  /**
   * The telephony call state may be asked for: on Android 8 to 11 always,
   * on Android 12 and later only while the Phone permission is granted (as
   * the permissions were last read, or read now when that is five seconds
   * old or more). Without it the audio mode alone says whether a call rings.
   */
  private fun telephonyAllowed(now: Long): Boolean {
    if (!phone.callStateNeedsPhone) return true
    val seen = permissionsSeen
    val at = permissionsAtMs
    if (seen != null && at != null && now - at < WatchTimes.PERMISSIONS_EVERY_MS) return seen.phone
    return remembered(phoneGranted, now) { phone.phoneGranted() }
  }

  private fun permissionsNow(now: Long, force: Boolean = false): LimoPermissions {
    val seen = permissionsSeen
    val at = permissionsAtMs
    if (!force && seen != null && at != null && now - at < WatchTimes.PERMISSIONS_EVERY_MS) {
      return seen.copy(accessibilityRunning = serviceRunning)
    }
    val read = LimoPermissions(
      notifications = remembered(notifications, now) { phone.notificationsEnabled() },
      phone = remembered(phoneGranted, now) { phone.phoneGranted() },
      usageAccess = remembered(usageAccess, now) { phone.usageAccess() },
      accessibility = remembered(accessibility, now) { phone.accessibilityEnabled() },
      accessibilityRunning = serviceRunning,
      batteryExempt = remembered(batteryExempt, now) { phone.batteryExempt() },
      deviceAdmin = remembered(deviceAdmin, now) { phone.deviceAdmin() },
    )
    permissionsSeen = read
    permissionsAtMs = now
    return read
  }

  /**
   * The engine's six permissions. The accessibility service counts as on
   * when it is switched on in Settings and connected; or switched on and the
   * watch started less than a minute ago, since Android connects it a moment
   * after LiMo starts, and a guardian should not be told it was switched off
   * at every restart.
   */
  private fun enginePermissions(p: LimoPermissions, now: Long): PhonePermissions {
    val starting = now - startedAtMs < WatchTimes.ACCESSIBILITY_START_MS
    return toEngine(p, accessibility = p.accessibility && (p.accessibilityRunning || starting))
  }

  /** The watch's permissions as the engine takes them, in setup order, with the engine's [accessibility]. */
  private fun toEngine(p: LimoPermissions, accessibility: Boolean) = PhonePermissions(
    notifications = p.notifications,
    phone = p.phone,
    usageAccess = p.usageAccess,
    accessibility = accessibility,
    battery = p.batteryExempt,
    protection = p.deviceAdmin,
  )

  private fun refreshRoles(now: Long) {
    val at = rolesAtMs
    if (at == null || now - at >= WatchTimes.ROLES_EVERY_MS) {
      roles = roles.copy(
        home = remembered(home, now) { phone.homePackages() },
        diallers = remembered(diallers, now) { phone.diallerPackages() } + remembered(inCall, now) { phone.inCallScreens() },
        emergency = remembered(emergency, now) { phone.emergencyPackages() },
        keyboards = remembered(keyboards, now) { phone.keyboardPackages() },
      )
      rolesAtMs = now
    }
    val launchableAt = launchableAtMs
    if (launchableAt == null || now - launchableAt >= WatchTimes.LAUNCHABLE_EVERY_MS) {
      roles = roles.copy(launchable = optional("launchable") { phone.launchablePackages() } ?: roles.launchable)
      launchableAtMs = now
    }
  }

  private fun elapsedNow(): Long = remembered(elapsed, elapsed.good ?: 0L) { phone.elapsedMs() }

  private inline fun <T : Any> remembered(r: Remembered<T>, now: Long, read: () -> T): T {
    try {
      val value = read()
      r.good = value
      return value
    } catch (e: Exception) {
      failed(r.fact, e, now)
    } catch (e: LinkageError) {
      failed(r.fact, e, now)
    }
    return r.good ?: r.default
  }

  private inline fun <T> optional(fact: String, read: () -> T?): T? {
    try {
      return read()
    } catch (e: Exception) {
      failed(fact, e)
    } catch (e: LinkageError) {
      failed(fact, e)
    }
    return null
  }

  private fun failed(fact: String, e: Throwable, now: Long = elapsed.good ?: 0L) {
    val last = failedAt[fact]
    if (last != null && now - last < WatchTimes.LOG_REPEAT_MS) return
    failedAt[fact] = now
    log.line("fact $fact unreadable error=${e.javaClass.simpleName} fallback_used")
  }
}

/** Labels by package, the newest [WatchTimes.MAX_LABELS], a missing one remembered as missing. */
internal class LabelCache(private val max: Int = WatchTimes.MAX_LABELS) {
  private class Entry(val label: String?)

  private val entries = object : LinkedHashMap<String, Entry>(32, 0.75f, true) {
    override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, Entry>?): Boolean = size > max
  }

  fun get(packageName: String, load: (String) -> String?): String? {
    entries[packageName]?.let { return it.label }
    val label = load(packageName)
    entries[packageName] = Entry(label)
    return label
  }

  fun clear() = entries.clear()

  val size: Int get() = entries.size
}
