package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.CallState
import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.ffi.PhonePermissions
import android.app.usage.UsageEvents
import android.media.AudioManager
import android.telephony.TelephonyManager
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The facts, read in one place, each from its reading and each with its own
 * fallback when Android refuses it; the call state; the permissions; the
 * apps a guardian picks from; and what the log may say.
 */
class FactsTest {
  @Test
  fun everyFactComesFromItsReadingInOnePlace() {
    val rig = Rig()
    rig.phone.hours24 = true
    rig.phone.autoTime = false
    rig.open(LAUNCHER)
    rig.open(GAME)
    // Whole seconds, rounded down.
    rig.phone.wall = WEDNESDAY_4_PM_MS + 999
    val look = rig.core.look(inReach = true, prodded = true)
    val facts = look.facts
    assertEquals(WEDNESDAY_4_PM_MS / 1_000, facts.wallSecs)
    assertEquals(rig.phone.elapsed, facts.elapsedMs)
    assertEquals(7, facts.bootCount)
    assertEquals(19_800, facts.utcOffsetSecs)
    assertTrue(facts.hours24)
    assertFalse(facts.autoTime)
    assertTrue(facts.interactive)
    assertFalse(facts.keyguardLocked)
    assertEquals(CallState.IDLE, facts.call)
    assertEquals(FrontKind.APP, facts.front.kind)
    assertEquals(GAME, facts.front.packageName)
    assertEquals("Rocket Racers", facts.front.label)
    // The link's two, passed through.
    assertTrue(facts.inReach)
    assertTrue(facts.prodded)
    // All six for the engine, in setup order: protection is the device admin, still off here.
    assertEquals(
      PhonePermissions(
        notifications = true,
        phone = true,
        usageAccess = true,
        accessibility = true,
        battery = true,
        protection = false,
      ),
      facts.permissions,
    )
    assertEquals(
      LimoPermissions(
        notifications = true,
        phone = true,
        usageAccess = true,
        accessibility = true,
        accessibilityRunning = false,
        batteryExempt = true,
        deviceAdmin = false,
      ),
      look.permissions,
    )
    assertNull(look.unknownBecause)
    assertTrue(look.alsoVisible.isEmpty())

    // The screen off, Android's lock screen up, another zone: read at the next call.
    rig.phone.screenOn = false
    rig.phone.keyguard = true
    rig.phone.offset = 3_600
    val later = rig.core.look(inReach = false, prodded = false).facts
    assertFalse(later.interactive)
    assertTrue(later.keyguardLocked)
    assertEquals(3_600, later.utcOffsetSecs)
    assertFalse(later.inReach)
    assertFalse(later.prodded)
  }

  @Test
  fun eachFactFallsBackToItsLastGoodReadingThenToItsDefault() {
    val rig = Rig()
    rig.phone.hours24 = true
    rig.phone.autoTime = true
    rig.phone.audioMode = 2
    val good = rig.facts()
    assertEquals(CallState.ONGOING, good.call)

    // Android now says otherwise, and refuses to say it.
    val refused = listOf("boot_count", "utc_offset", "hours_24", "auto_time", "interactive", "keyguard_locked", "audio_mode")
    rig.phone.failing += refused
    rig.phone.boot = 8
    rig.phone.offset = 0
    rig.phone.hours24 = false
    rig.phone.autoTime = false
    rig.phone.screenOn = false
    rig.phone.keyguard = true
    rig.phone.audioMode = 0
    val kept = rig.facts()
    assertEquals(good.bootCount, kept.bootCount)
    assertEquals(good.utcOffsetSecs, kept.utcOffsetSecs)
    assertEquals(good.hours24, kept.hours24)
    assertEquals(good.autoTime, kept.autoTime)
    assertEquals(good.interactive, kept.interactive)
    assertEquals(good.keyguardLocked, kept.keyguardLocked)
    assertEquals(CallState.ONGOING, kept.call)

    // A new process with nothing good read yet: each fact's default.
    val fresh = Rig(rig.phone).facts()
    assertEquals(0, fresh.bootCount)
    assertEquals(0, fresh.utcOffsetSecs)
    assertFalse(fresh.hours24)
    // A clock set by hand buys nothing: with automatic time unknown, it is taken as off.
    assertFalse(fresh.autoTime)
    assertTrue(fresh.interactive)
    assertFalse(fresh.keyguardLocked)
    assertEquals(CallState.IDLE, fresh.call)
  }

  @Test
  fun nothingAndroidRefusesStopsTheFacts() {
    val phone = TestPhone()
    phone.failing += ALL_READINGS
    val rig = Rig(phone)
    val look = rig.look()
    val facts = look.facts
    assertEquals(0L, facts.wallSecs)
    assertEquals(0L, facts.elapsedMs)
    assertEquals(0, facts.bootCount)
    assertEquals(CallState.IDLE, facts.call)
    assertEquals(FrontKind.UNKNOWN, facts.front.kind)
    assertEquals(UnknownBecause.USAGE_UNREADABLE, look.unknownBecause)
    assertEquals(PhonePermissions(false, false, false, false, false, false), facts.permissions)
    assertEquals(LimoPermissions(false, false, false, false, false, false, false), look.permissions)
    // Nor do the service's events, the rechecks or the clock.
    assertFalse(rig.core.windowEvent(GAME))
    rig.core.recheck()
    rig.core.clockSet()
    rig.core.callChanged()
    assertTrue(rig.core.shapeApps(emptyMap()).isEmpty())

    // Each refusal was logged once, by the fact's name.
    for (fact in listOf("boot_count", "usage_events", "audio_mode", "home", "launchable", "usage_access")) {
      assertEquals(fact, 1, rig.log.lines.count { it.startsWith("fact $fact ") })
    }

    // Android answers again: the facts come back.
    phone.failing.clear()
    rig.open(LAUNCHER)
    assertEquals(FrontKind.HOME, rig.front().kind)
    assertEquals(7, rig.facts().bootCount)
  }

  @Test
  fun aRefusalIsLoggedOnceAMinuteWithoutItsMessageOrALabel() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(GAME)
    rig.phone.failing += "boot_count"
    rig.facts()
    rig.phone.pass(1_000)
    rig.facts()
    assertEquals(listOf("fact boot_count unreadable error=SecurityException fallback_used"), rig.log.lines.filter { it.startsWith("fact ") })
    rig.phone.pass(60_000)
    rig.facts()
    assertEquals(2, rig.log.lines.count { it.startsWith("fact boot_count ") })

    // Package names may be logged; never a label, a name or a refusal's words.
    rig.phone.failing += "label"
    rig.open(VIDEO)
    rig.open(GAME)
    for (line in rig.log.lines) {
      for (secret in listOf("Rocket Racers", "Tube Time", "Mira", "Secret", "refused")) {
        assertFalse(line, line.contains(secret))
      }
    }
    assertTrue(rig.log.lines.contains("front now app package=$GAME"))
  }

  @Test
  fun theCallStateComesFromTheAudioModeAndOnOlderPhonesTheTelephonyStateToo() {
    // Android's own numbers.
    assertEquals(CallState.IDLE, Calls.fromAudioMode(AudioManager.MODE_NORMAL))
    assertEquals(CallState.RINGING, Calls.fromAudioMode(AudioManager.MODE_RINGTONE))
    assertEquals(CallState.ONGOING, Calls.fromAudioMode(AudioManager.MODE_IN_CALL))
    assertEquals(CallState.RINGING, Calls.fromAudioMode(AudioManager.MODE_CALL_SCREENING))
    assertEquals(CallState.ONGOING, Calls.fromAudioMode(AudioManager.MODE_CALL_REDIRECT))
    // An internet call, such as a game's voice chat, is not a call.
    assertEquals(CallState.IDLE, Calls.fromAudioMode(AudioManager.MODE_IN_COMMUNICATION))
    assertEquals(CallState.IDLE, Calls.fromAudioMode(AudioManager.MODE_COMMUNICATION_REDIRECT))
    assertEquals(CallState.IDLE, Calls.fromAudioMode(AudioManager.MODE_INVALID))
    assertEquals(CallState.IDLE, Calls.fromTelephony(TelephonyManager.CALL_STATE_IDLE))
    assertEquals(CallState.RINGING, Calls.fromTelephony(TelephonyManager.CALL_STATE_RINGING))
    assertEquals(CallState.ONGOING, Calls.fromTelephony(TelephonyManager.CALL_STATE_OFFHOOK))
    // Both in one: a ring in either rings, then a call on in either is on.
    assertNull(Calls.both(null, null))
    assertEquals(CallState.RINGING, Calls.both(CallState.IDLE, CallState.RINGING))
    assertEquals(CallState.RINGING, Calls.both(CallState.ONGOING, CallState.RINGING))
    assertEquals(CallState.ONGOING, Calls.both(CallState.ONGOING, null))
    assertEquals(CallState.ONGOING, Calls.both(null, CallState.ONGOING))
    assertEquals(CallState.IDLE, Calls.both(CallState.IDLE, null))

    // On Android 8 to 11, a call that rings silently leaves the audio mode
    // as it was; the telephony state still says so.
    val rig = Rig()
    rig.phone.telephony = TelephonyManager.CALL_STATE_RINGING
    assertEquals(CallState.RINGING, rig.facts().call)
    // On Android 12 and later there is no telephony state without the phone permission.
    rig.phone.telephony = null
    assertEquals(CallState.IDLE, rig.facts().call)
    rig.phone.audioMode = AudioManager.MODE_RINGTONE
    assertEquals(CallState.RINGING, rig.facts().call)
  }

  @Test
  fun usageEventKindsAreAndroidsNumbers() {
    assertEquals(UsageKind.RESUMED, usageKindOf(UsageEvents.Event.ACTIVITY_RESUMED))
    @Suppress("DEPRECATION")
    assertEquals(UsageKind.RESUMED, usageKindOf(UsageEvents.Event.MOVE_TO_FOREGROUND))
    assertEquals(UsageKind.PAUSED, usageKindOf(UsageEvents.Event.ACTIVITY_PAUSED))
    assertEquals(UsageKind.STOPPED, usageKindOf(UsageEvents.Event.ACTIVITY_STOPPED))
    assertEquals(UsageKind.STARTUP, usageKindOf(UsageEvents.Event.DEVICE_STARTUP))
    for (other in listOf(
      UsageEvents.Event.DEVICE_SHUTDOWN,
      UsageEvents.Event.CONFIGURATION_CHANGE,
      UsageEvents.Event.KEYGUARD_SHOWN,
      UsageEvents.Event.SCREEN_INTERACTIVE,
      UsageEvents.Event.FOREGROUND_SERVICE_START,
    )) {
      assertNull(usageKindOf(other))
    }
  }

  @Test
  fun accessibilityIsOnWhenSwitchedOnAndConnectedOrStillStarting() {
    val rig = Rig()
    // Switched on, not connected yet, the watch just started: on, so a
    // restart never tells the guardian it was switched off.
    var look = rig.look()
    assertTrue(look.facts.permissions.accessibility)
    assertTrue(look.permissions.accessibility)
    assertFalse(look.permissions.accessibilityRunning)
    // A minute on and still not connected: off.
    rig.phone.pass(61_000)
    assertFalse(rig.facts().permissions.accessibility)
    rig.core.serviceConnected(true)
    look = rig.look()
    assertTrue(look.facts.permissions.accessibility)
    assertTrue(look.permissions.accessibilityRunning)
    rig.core.serviceConnected(false)
    assertFalse(rig.facts().permissions.accessibility)
    // Switched off in Settings: off, connected or not.
    rig.core.serviceConnected(true)
    rig.phone.accessibilityOn = false
    rig.core.permissionsChanged()
    assertFalse(rig.facts().permissions.accessibility)
  }

  @Test
  fun thePermissionsAreReadAgainAfterFiveSecondsOrWhenAndroidSaysTheyChanged() {
    val rig = Rig()
    assertTrue(rig.look().permissions.notifications)
    rig.phone.notifications = false
    rig.phone.admin = true
    rig.phone.pass(1_000)
    assertTrue(rig.look().permissions.notifications)
    rig.phone.pass(5_000)
    assertFalse(rig.look().permissions.notifications)
    assertTrue(rig.look().permissions.deviceAdmin)
    rig.phone.battery = false
    rig.core.permissionsChanged()
    assertFalse(rig.look().permissions.batteryExempt)
    // Asked for directly, they are read now.
    rig.phone.battery = true
    assertTrue(rig.core.permissions().batteryExempt)
  }

  @Test
  fun thePhonePermissionIsReadAsAndroidGrantsItAndIsOffWhenItCannotBeRead() {
    val rig = Rig()
    assertTrue(rig.look().permissions.phone)
    assertTrue(rig.facts().permissions.phone)
    // Not granted: off for the watch and for the engine, and nothing else changes.
    rig.phone.phoneGranted = false
    rig.core.permissionsChanged()
    val look = rig.look()
    assertFalse(look.permissions.phone)
    assertFalse(look.facts.permissions.phone)
    assertTrue(look.facts.permissions.notifications && look.facts.permissions.usageAccess && look.facts.permissions.battery)
    // Each of the watch's permissions reaches the engine's own field.
    rig.phone.phoneGranted = true
    rig.phone.notifications = false
    rig.phone.battery = false
    rig.phone.admin = true
    rig.core.permissionsChanged()
    val engine = rig.facts().permissions
    assertEquals(PhonePermissions(notifications = false, phone = true, usageAccess = true, accessibility = true, battery = false, protection = true), engine)
    // A reading Android refuses in a new process, with nothing good read yet: off.
    rig.phone.failing += "phone_permission"
    assertFalse(Rig(rig.phone).look().permissions.phone)
  }

  @Test
  fun theAccessibilitySettingNamesLimoInEitherForm() {
    val service = "ai.lucidloom.limo.child.watch.WatchService"
    assertTrue(AccessibilitySetting.names("org.example.reader/org.example.reader.Reader:$LIMO/$service", LIMO, service))
    assertTrue(AccessibilitySetting.names("$LIMO/.watch.WatchService", LIMO, service))
    assertFalse(AccessibilitySetting.names("$LIMO/.watch.Other", LIMO, service))
    assertFalse(AccessibilitySetting.names("org.example.other/$service", LIMO, service))
    assertFalse(AccessibilitySetting.names("not a service", LIMO, service))
    assertFalse(AccessibilitySetting.names("", LIMO, service))
    assertFalse(AccessibilitySetting.names(null, LIMO, service))
  }

  @Test
  fun launchableAppsAreSortedByLabelWithoutLimoAndSayWhichNeverCount() {
    val rig = Rig()
    rig.phone.apps = linkedMapOf(
      GAME to "Rocket Racers",
      LIMO to "LiMo",
      "org.example.nolabel" to null,
      PHONE_APP to "Phone",
      VIDEO to "tube Time",
      CLOCK to " Clock ",
      "org.example.same" to "org.example.same",
    )
    val apps = rig.core.shapeApps(rig.phone.launchableApps())
    assertEquals(
      listOf(
        LaunchableApp(CLOCK, "Clock", neverCounted = false),
        LaunchableApp(PHONE_APP, "Phone", neverCounted = true),
        LaunchableApp(GAME, "Rocket Racers", neverCounted = false),
        LaunchableApp(VIDEO, "tube Time", neverCounted = false),
        LaunchableApp("org.example.nolabel", null, neverCounted = false),
        LaunchableApp("org.example.same", null, neverCounted = false),
      ),
      apps,
    )
  }

  @Test
  fun labelsAreKeptAndForgottenWhenAppsChange() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(GAME)
    repeat(5) { rig.facts() }
    assertEquals(1, rig.phone.labelReads)
    rig.phone.labels[GAME] = "Rocket Racers 2"
    rig.core.appsChanged()
    assertEquals("Rocket Racers 2", rig.front().label)
    assertEquals(2, rig.phone.labelReads)

    val cache = LabelCache(max = 2)
    for (pkg in listOf(GAME, VIDEO, CLOCK)) cache.get(pkg) { "label of $it" }
    assertEquals(2, cache.size)
  }

  @Test
  fun theRolesAreAskedAgainAfterAMinuteOrWhenAppsChangeAndKeptWhenAndroidRefuses() {
    val rig = Rig()
    rig.open(LAUNCHER)
    assertEquals(FrontKind.HOME, rig.front().kind)
    // Another home app is chosen. Until the roles are asked again it is an app.
    val newHome = "org.example.newhome"
    rig.phone.home = setOf(newHome)
    rig.open(newHome)
    assertEquals(FrontKind.APP, rig.front().kind)
    rig.phone.pass(61_000)
    assertEquals(FrontKind.HOME, rig.front().kind)
    // Android refuses to say: the last answer holds.
    rig.phone.failing += "home"
    rig.phone.pass(61_000)
    assertEquals(FrontKind.HOME, rig.front().kind)
    // A new default dialler, told by Android's broadcast.
    rig.phone.failing.clear()
    rig.phone.diallers = setOf(CHAT)
    rig.core.appsChanged()
    rig.open(CHAT)
    assertEquals(FrontKind.DIALLER, rig.front().kind)
    rig.open(PHONE_APP)
    assertEquals(FrontKind.APP, rig.front().kind)
  }

  @Test
  fun usageThatCannotBeReadIsUnknownUntilItCanBeAgain() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(GAME)
    rig.phone.failing += "usage_events"
    rig.phone.pass(600)
    val look = rig.look()
    assertEquals(FrontKind.UNKNOWN, look.facts.front.kind)
    assertEquals(UnknownBecause.USAGE_UNREADABLE, look.unknownBecause)
    rig.phone.failing.clear()
    rig.phone.pass(600)
    assertEquals(GAME, rig.front().packageName)
  }
}
