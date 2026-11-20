package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.Front
import ai.lucidloom.limo.child.ffi.PhoneFacts

// The watch's tests run on the PC's JVM with no Android at all: the phone is
// TestPhone, a PhoneSource whose every reading the test sets, and which can
// be told to refuse any of them. Android's usage record is a list the test
// writes as Android would, with events it can hold back to write late.

internal const val LIMO = "ai.lucidloom.limo.child"
internal const val LAUNCHER = "org.example.launcher"
internal const val PHONE_APP = "org.example.dialler"
internal const val CALL_SCREEN = "org.example.incall"
internal const val SOS = "org.example.sos"
internal const val KEYBOARD = "org.example.keyboard"
internal const val GAME = "org.example.game"
internal const val VIDEO = "org.example.video"
internal const val CLOCK = "org.example.clock"
internal const val CHAT = "org.example.chat"
internal const val HELPER = "org.example.helper"
internal const val SYSTEM_UI = "com.android.systemui"

/** Wednesday 18 November 2026, 16:00 in India, in milliseconds. */
internal const val WEDNESDAY_4_PM_MS = 1_794_997_800_000L

/** Every reading's name, as the watch logs it and as [TestPhone.failing] takes it. */
internal val ALL_READINGS = listOf(
  "wall_clock", "elapsed", "boot_count", "utc_offset", "hours_24", "auto_time", "interactive",
  "keyguard_locked", "audio_mode", "telephony_call_state", "usage_access", "accessibility",
  "notifications", "phone_permission", "battery_exempt", "device_admin", "usage_events", "home", "diallers",
  "in_call_screens", "emergency_dialler", "keyboards", "launchable", "label",
)

/** A phone as the watch reads it. Each field is what Android says now. */
internal class TestPhone : PhoneSource {
  override val ownPackage = LIMO
  override var stopsReported = true

  var wall = WEDNESDAY_4_PM_MS
  var elapsed = 3_600_000L
  var boot = 7
  var offset = 19_800
  var hours24 = false
  var autoTime = true
  var screenOn = true
  var keyguard = false
  var audioMode = 0
  var telephony: Int? = null
  var usageOn = true
  var accessibilityOn = true
  var notifications = true
  var phoneGranted = true
  var battery = true
  var admin = false
  var home = setOf(LAUNCHER)
  var diallers = setOf(PHONE_APP)
  var inCall = emptySet<String>()
  var emergency = emptySet<String>()
  var keyboards = setOf(KEYBOARD)

  /** Apps with an icon, with the label each has. */
  var apps: Map<String, String?> = linkedMapOf(
    GAME to "Rocket Racers",
    VIDEO to "Tube Time",
    PHONE_APP to "Phone",
    CLOCK to "Clock",
    CHAT to "Chatter",
    LIMO to "LiMo",
  )

  /** What the package manager gives as a label; a package missing here is one Android does not know (or LiMo cannot see). */
  val labels = HashMap<String, String?>(apps)

  /** Readings to refuse, by name ([ALL_READINGS]). */
  val failing = HashSet<String>()

  /** Android's usage record, as queryEvents finds it. */
  val record = ArrayList<UsageEvent>()

  /** Events that happened but Android has not yet written where queryEvents finds them. */
  val heldBack = ArrayList<UsageEvent>()

  var usageReads = 0
  var labelReads = 0

  fun pass(ms: Long) {
    wall += ms
    elapsed += ms
  }

  fun resumed(pkg: String, late: Boolean = false) = happen(pkg, UsageKind.RESUMED, late)

  fun paused(pkg: String) = happen(pkg, UsageKind.PAUSED, false)

  fun stopped(pkg: String) = happen(pkg, UsageKind.STOPPED, false)

  fun started() = happen("android", UsageKind.STARTUP, false)

  /** Android writes what it held back. */
  fun catchUp() {
    record.addAll(heldBack)
    heldBack.clear()
  }

  private fun happen(pkg: String, kind: UsageKind, late: Boolean) {
    val event = UsageEvent(pkg, kind, wall)
    if (late) heldBack.add(event) else record.add(event)
  }

  /** A refusal whose message holds a label and a name, which the log must never repeat. */
  private fun check(name: String) {
    if (name in failing) throw SecurityException("Mira's Secret Diary refused $name")
  }

  override fun wallMs(): Long = wall.also { check("wall_clock") }
  override fun elapsedMs(): Long = elapsed.also { check("elapsed") }
  override fun bootCount(): Int = boot.also { check("boot_count") }
  override fun utcOffsetSecs(wallMs: Long): Int = offset.also { check("utc_offset") }
  override fun hours24(): Boolean = hours24.also { check("hours_24") }
  override fun autoTime(): Boolean = autoTime.also { check("auto_time") }
  override fun interactive(): Boolean = screenOn.also { check("interactive") }
  override fun keyguardLocked(): Boolean = keyguard.also { check("keyguard_locked") }
  override fun audioMode(): Int = audioMode.also { check("audio_mode") }
  override fun telephonyCallState(): Int? = telephony.also { check("telephony_call_state") }
  override fun usageAccess(): Boolean = usageOn.also { check("usage_access") }
  override fun accessibilityEnabled(): Boolean = accessibilityOn.also { check("accessibility") }
  override fun notificationsEnabled(): Boolean = notifications.also { check("notifications") }
  override fun phoneGranted(): Boolean = phoneGranted.also { check("phone_permission") }
  override fun batteryExempt(): Boolean = battery.also { check("battery_exempt") }
  override fun deviceAdmin(): Boolean = admin.also { check("device_admin") }

  override fun usageEvents(fromWallMs: Long, toWallMs: Long): List<UsageEvent>? {
    check("usage_events")
    usageReads += 1
    if (!usageOn) return null
    return record.filter { it.atWallMs >= fromWallMs && it.atWallMs < toWallMs }.sortedBy { it.atWallMs }
  }

  override fun homePackages(): Set<String> = home.also { check("home") }
  override fun diallerPackages(): Set<String> = diallers.also { check("diallers") }
  override fun inCallScreens(): Set<String> = inCall.also { check("in_call_screens") }
  override fun emergencyPackages(): Set<String> = emergency.also { check("emergency_dialler") }
  override fun keyboardPackages(): Set<String> = keyboards.also { check("keyboards") }
  override fun launchablePackages(): Set<String> = apps.keys.also { check("launchable") }
  override fun launchableApps(): Map<String, String?> = apps.also { check("launchable") }

  override fun label(packageName: String): String? {
    check("label")
    labelReads += 1
    return labels[packageName]
  }
}

/** Every line the watch logs. */
internal class LogLines : WatchLog {
  val lines = ArrayList<String>()

  override fun line(text: String) {
    lines.add(text)
  }
}

/** The watch's core on a [TestPhone], driven as the service, the handler and the host drive it. */
internal class Rig(val phone: TestPhone = TestPhone()) {
  val log = LogLines()
  val core = WatchCore(phone, log)

  /** The package whose activity is up, as this rig opened it. */
  private var up: String? = null

  fun look(): WatchLook = core.look(inReach = false, prodded = false)

  fun facts(): PhoneFacts = look().facts

  fun front(): Front = facts().front

  /**
   * An app comes up as Android does it: the activity that was up pauses and
   * stops, the new one resumes, and its window tells the service. Returns
   * whether the watch said the front changed.
   */
  fun open(pkg: String): Boolean {
    up?.let {
      phone.pass(10)
      phone.paused(it)
      phone.stopped(it)
    }
    phone.pass(30)
    phone.resumed(pkg)
    up = pkg
    phone.pass(20)
    return core.windowEvent(pkg)
  }

  /** The handler's rechecks, as [Watch] runs them, until nothing waits. Returns how many ran. */
  fun settle(): Int {
    var runs = 0
    while (true) {
      val wait = core.nextLookInMs() ?: return runs
      phone.pass(wait)
      core.recheck()
      runs += 1
      check(runs < 10) { "the rechecks never ended" }
    }
  }
}
