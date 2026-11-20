package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.CallState
import ai.lucidloom.limo.child.ffi.FrontKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Which app is in front, through real orders of events: window events
 * proposing, Android's usage record confirming. Proof two's notes from the
 * Galaxy (17 Nov 2026) are the first story: after the cover's "Go home", the
 * blocked app sent one more window event after the home screen's, and the
 * cover came back over the home screen in 12 of 14 tries.
 */
class FrontTest {
  @Test
  fun anAppIsInFrontOnceItsActivityResumedAndItsWindowSaidSo() {
    val rig = Rig()
    assertTrue(rig.open(LAUNCHER))
    assertEquals(FrontKind.HOME, rig.front().kind)
    assertTrue(rig.open(GAME))
    val front = rig.front()
    assertEquals(FrontKind.APP, front.kind)
    assertEquals(GAME, front.packageName)
    assertEquals("Rocket Racers", front.label)
    // Confirmed at once: nothing waits.
    assertNull(rig.core.nextLookInMs())
    // The same app's windows again (a dialog of its own, a new page): nothing new.
    val reads = rig.phone.usageReads
    assertFalse(rig.core.windowEvent(GAME))
    assertEquals(reads, rig.phone.usageReads)
  }

  @Test
  fun aLateWindowEventAfterHomeNeverPutsTheBlockedAppBackInFront() {
    // Fourteen tries, as on the Galaxy, the late event at another moment
    // each time, the app stopped in some and only paused in others.
    repeat(14) { attempt ->
      val rig = Rig()
      rig.open(LAUNCHER)
      rig.open(VIDEO)
      assertEquals(VIDEO, rig.front().packageName)

      // The cover's "Go home": the video app's activity pauses, the home
      // screen's resumes, and the home screen's window tells the service.
      rig.phone.pass(40)
      rig.phone.paused(VIDEO)
      rig.phone.pass(15)
      rig.phone.resumed(LAUNCHER)
      rig.phone.pass(10)
      assertTrue(rig.core.windowEvent(LAUNCHER))
      assertEquals(FrontKind.HOME, rig.front().kind)

      // Then the video app's late window event.
      rig.phone.pass(5L + attempt * 25L)
      if (attempt % 2 == 0) rig.phone.stopped(VIDEO)
      assertFalse(rig.core.windowEvent(VIDEO))
      assertEquals("try ${attempt + 1}", FrontKind.HOME, rig.front().kind)

      // Usage is read again at 100, 300 and 1,000 ms; the event is never
      // confirmed and is dropped; the home screen stays in front throughout.
      assertEquals(3, rig.settle())
      assertEquals("try ${attempt + 1}", FrontKind.HOME, rig.front().kind)
      assertNull(rig.core.nextLookInMs())
      assertTrue(rig.log.lines.any { it == "front window_event package=$VIDEO not_confirmed front=$LAUNCHER" })
    }
  }

  @Test
  fun homeComesUpEvenWhenAndroidWritesItsResumeLateAndTheLateEventArrivesFirst() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(VIDEO)
    // "Go home", with Android slow to write the home screen's resume.
    rig.phone.pass(40)
    rig.phone.paused(VIDEO)
    rig.phone.pass(15)
    rig.phone.resumed(LAUNCHER, late = true)
    rig.phone.pass(10)
    assertFalse(rig.core.windowEvent(LAUNCHER))
    // Not confirmed yet: the front stays what usage says, never what the event says.
    assertEquals(VIDEO, rig.front().packageName)
    assertEquals(100L, rig.core.nextLookInMs())
    // The late event of the video app: it is in front by usage, so nothing
    // new, and the home screen's rechecks go on.
    rig.phone.pass(20)
    assertFalse(rig.core.windowEvent(VIDEO))
    assertEquals(80L, rig.core.nextLookInMs())
    // Android writes the resume; the next recheck finds it.
    rig.phone.catchUp()
    rig.phone.pass(80)
    assertTrue(rig.core.recheck())
    assertEquals(FrontKind.HOME, rig.front().kind)
    assertNull(rig.core.nextLookInMs())
  }

  @Test
  fun aWindowEventUsageHasNotWrittenYetIsConfirmedByARecheck() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.phone.pass(30)
    rig.phone.resumed(GAME, late = true)
    rig.phone.pass(20)
    assertFalse(rig.core.windowEvent(GAME))
    assertEquals(FrontKind.HOME, rig.front().kind)
    assertEquals(100L, rig.core.nextLookInMs())
    rig.phone.pass(100)
    assertFalse(rig.core.recheck())
    assertEquals(200L, rig.core.nextLookInMs())
    rig.phone.catchUp()
    rig.phone.pass(200)
    assertTrue(rig.core.recheck())
    assertEquals(GAME, rig.front().packageName)
    assertNull(rig.core.nextLookInMs())
  }

  @Test
  fun aWindowThatIsNoActivityNeverComesToTheFront() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(GAME)
    // A chat app's floating bubble over the game: window events, no activity.
    rig.phone.pass(500)
    assertFalse(rig.core.windowEvent(CHAT))
    rig.phone.pass(200)
    assertFalse(rig.core.windowEvent(CHAT))
    assertEquals(3, rig.settle())
    assertEquals(GAME, rig.front().packageName)
    assertTrue(rig.log.lines.contains("front window_event package=$CHAT not_confirmed front=$GAME"))
  }

  @Test
  fun theShadeSystemDialogsKeyboardsAndLimosCoversProposeNothing() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(GAME)
    val reads = rig.phone.usageReads
    for (pkg in listOf(SYSTEM_UI, "android", "com.android.permissioncontroller", KEYBOARD, LIMO, null, "", "  ")) {
      assertFalse(rig.core.windowEvent(pkg))
    }
    assertEquals(reads, rig.phone.usageReads)
    assertNull(rig.core.nextLookInMs())
    // The recent apps of older phones are an activity of System UI's, and a
    // permission dialog is the permission controller's: the app under them
    // stays in front.
    rig.phone.pass(50)
    rig.phone.paused(GAME)
    rig.phone.resumed(SYSTEM_UI)
    rig.phone.pass(800)
    rig.phone.resumed("com.android.permissioncontroller")
    rig.phone.pass(800)
    assertEquals(GAME, rig.front().packageName)
  }

  @Test
  fun limoIsLimoAndItsCoverIsNothing() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(GAME)
    // A cover over the game is a window of LiMo's own, not an activity.
    assertFalse(rig.core.windowEvent(LIMO))
    assertEquals(GAME, rig.front().packageName)
    // LiMo's own screen is an activity: usage shows it.
    rig.phone.pass(30)
    rig.phone.resumed(LIMO)
    rig.phone.pass(600)
    val front = rig.front()
    assertEquals(FrontKind.LIMO, front.kind)
    assertNull(front.packageName)
    assertNull(front.label)
  }

  @Test
  fun limosScreenSaysSoItselfEvenWithUsageAccessOff() {
    val rig = Rig()
    rig.phone.usageOn = false
    assertEquals(FrontKind.UNKNOWN, rig.front().kind)
    assertTrue(rig.core.limoScreen(true))
    assertEquals(FrontKind.LIMO, rig.front().kind)
    assertTrue(rig.core.limoScreen(false))
    assertEquals(FrontKind.UNKNOWN, rig.front().kind)

    // With usage on, a missed word that LiMo's screen left is put right as
    // soon as another app resumes.
    rig.phone.usageOn = true
    rig.open(LAUNCHER)
    assertTrue(rig.core.limoScreen(true))
    assertEquals(FrontKind.LIMO, rig.front().kind)
    rig.open(GAME)
    assertEquals(GAME, rig.front().packageName)
  }

  @Test
  fun theDiallerAndACallsOwnScreenAreTheDiallerAndTheCallScreenIsLearned() {
    // A maker whose call screen is another package than the dialler, which
    // Android's telecom service does not name.
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(PHONE_APP)
    assertEquals(FrontKind.DIALLER, rig.front().kind)
    assertNull(rig.front().packageName)

    // A call is dialled and its own screen comes up: a package with no icon.
    rig.phone.audioMode = 2
    rig.core.callChanged()
    rig.open(CALL_SCREEN)
    val onCall = rig.facts()
    assertEquals(CallState.ONGOING, onCall.call)
    assertEquals(FrontKind.DIALLER, onCall.front.kind)
    assertTrue(rig.log.lines.contains("call now ongoing"))
    assertTrue(rig.log.lines.contains("front learned call_screen package=$CALL_SCREEN"))

    // During the call a game is a game: a call left open is no way around the limits.
    rig.open(GAME)
    assertEquals(FrontKind.APP, rig.front().kind)

    // The call ends. Its screen is still the dialler after it, so a call
    // that rings silently later is never covered.
    rig.phone.audioMode = 0
    rig.core.callChanged()
    rig.open(CALL_SCREEN)
    val after = rig.facts()
    assertEquals(CallState.IDLE, after.call)
    assertEquals(FrontKind.DIALLER, after.front.kind)
  }

  @Test
  fun aScreenWithNoIconIsTheCallsOnlyDuringACallAndIsLearnedOnlyAsTheCallStarts() {
    val rig = Rig()
    rig.open(LAUNCHER)
    // No call: a package with no icon is an app, with no label to show.
    rig.open(HELPER)
    assertEquals(FrontKind.APP, rig.front().kind)
    assertNull(rig.front().label)

    // A call started a minute ago: during it, the same package is taken as
    // the call's screen, but it did not come up as the call started, so it
    // is not learned.
    rig.open(LAUNCHER)
    rig.phone.audioMode = 2
    rig.core.callChanged()
    rig.phone.pass(60_000)
    rig.open(HELPER)
    assertEquals(FrontKind.DIALLER, rig.front().kind)
    rig.phone.audioMode = 0
    rig.core.callChanged()
    assertEquals(FrontKind.APP, rig.front().kind)
    assertFalse(rig.log.lines.any { it.contains("learned") })
  }

  @Test
  fun anInCallScreenAndroidNamesIsTheDiallerCallOrNoCall() {
    val rig = Rig(TestPhone().apply { inCall = setOf(CALL_SCREEN) })
    rig.open(LAUNCHER)
    rig.open(CALL_SCREEN)
    assertEquals(FrontKind.DIALLER, rig.front().kind)
    assertEquals(CallState.IDLE, rig.facts().call)
  }

  @Test
  fun theEmergencyDiallerIsKnownFromAndroidOrLearnedWhenLimoOpensIt() {
    val named = Rig(TestPhone().apply { emergency = setOf(SOS) })
    named.open(LAUNCHER)
    named.open(SOS)
    assertEquals(FrontKind.EMERGENCY_DIALLER, named.front().kind)

    // Android names none. The lock's emergency call: LiMo says it is opening it.
    val learned = Rig()
    learned.open(LAUNCHER)
    learned.open(LIMO)
    learned.core.emergencyOpening()
    learned.open(SOS)
    assertEquals(FrontKind.EMERGENCY_DIALLER, learned.front().kind)
    assertTrue(learned.log.lines.contains("front learned emergency_dialler package=$SOS"))
    learned.open(LAUNCHER)
    learned.open(SOS)
    assertEquals(FrontKind.EMERGENCY_DIALLER, learned.front().kind)

    // An app with an icon is never learned as the emergency dialler, even
    // the first to come up after LiMo opened it.
    val game = Rig()
    game.open(LAUNCHER)
    game.core.emergencyOpening()
    game.open(GAME)
    assertEquals(FrontKind.APP, game.front().kind)
    // Nor one that comes up after the ten seconds.
    val late = Rig()
    late.open(LAUNCHER)
    late.core.emergencyOpening()
    late.phone.pass(11_000)
    late.open(SOS)
    assertEquals(FrontKind.APP, late.front().kind)
  }

  @Test
  fun theFirstRunsEmergencyCallThenSettingsSearchLearnsNothing() {
    // The first run on a phone, 20 Nov 2026, a Galaxy on Android 13, in the
    // order and at the times its log showed. Android's roles name the
    // emergency dialler. A game is locked; Emergency call on the cover.
    val rig = Rig(TestPhone().apply {
      emergency = setOf(SOS)
      apps = apps + (SETTINGS to "Settings")
      labels[SETTINGS] = "Settings"
    })
    rig.open(LAUNCHER)
    rig.open(GAME)
    rig.core.emergencyOpening()
    // About 60 ms later the emergency dialler is in front, known from the roles.
    rig.open(SOS)
    assertEquals(FrontKind.EMERGENCY_DIALLER, rig.front().kind)
    assertTrue(rig.log.lines.contains("front now emergency_dialler package=$SOS"))
    assertTrue(rig.log.lines.contains("front emergency_wait ended first=$SOS"))
    // Under a second later the home screen; three seconds on, Settings.
    rig.phone.pass(800)
    rig.open(LAUNCHER)
    assertEquals(FrontKind.HOME, rig.front().kind)
    rig.phone.pass(3_070)
    rig.open(SETTINGS)
    assertEquals(FrontKind.APP, rig.front().kind)
    // Then Settings' search, a package with no icon, 8.7 seconds after the
    // tap: an app, never the emergency dialler.
    rig.phone.pass(4_600)
    rig.open(SEARCH)
    val front = rig.front()
    assertEquals(FrontKind.APP, front.kind)
    assertEquals(SEARCH, front.packageName)
    assertFalse(rig.log.lines.any { it.startsWith("front learned") })
    rig.open(LAUNCHER)
    rig.open(SEARCH)
    assertEquals(FrontKind.APP, rig.front().kind)
  }

  @Test
  fun anEmergencyDiallerTheRolesDoNotNameIsLearnedWhenItIsTheFirstScreenAfterTheTap() {
    // From LiMo's own screen of the ways out, tapped at once. LiMo's window
    // events propose nothing, so usage has not been read since its screen
    // resumed; the read at the tap takes it, and the emergency dialler is
    // still the first screen after the tap.
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(GAME)
    rig.open(LIMO)
    rig.core.emergencyOpening()
    rig.open(SOS)
    assertEquals(FrontKind.EMERGENCY_DIALLER, rig.front().kind)
    assertTrue(rig.log.lines.contains("front learned emergency_dialler package=$SOS"))
    // A second package with no icon, well inside the ten seconds: an app.
    rig.phone.pass(2_000)
    rig.open(HELPER)
    assertEquals(FrontKind.APP, rig.front().kind)
    assertEquals(1, rig.log.lines.count { it.startsWith("front learned") })
    rig.open(SOS)
    assertEquals(FrontKind.EMERGENCY_DIALLER, rig.front().kind)

    // Left before the watch read the record: the emergency dialler's window
    // event missed, its resume read together with the home screen's. It was
    // still the first screen after the tap.
    val quick = Rig()
    quick.open(LAUNCHER)
    quick.open(GAME)
    quick.core.emergencyOpening()
    quick.phone.pass(10)
    quick.phone.paused(GAME)
    quick.phone.stopped(GAME)
    quick.phone.pass(30)
    quick.phone.resumed(SOS)
    quick.phone.pass(400)
    quick.phone.paused(SOS)
    quick.phone.resumed(LAUNCHER)
    quick.phone.pass(20)
    assertTrue(quick.core.windowEvent(LAUNCHER))
    assertEquals(FrontKind.HOME, quick.front().kind)
    assertTrue(quick.log.lines.contains("front learned emergency_dialler package=$SOS"))

    // LiMo started again (what it learned is kept in memory only) with the
    // emergency dialler still in front, so covered. Emergency call on the
    // cover brings it up again: the same package, the first screen after
    // the tap, learned.
    val again = Rig()
    again.open(LAUNCHER)
    again.open(SOS)
    assertEquals(FrontKind.APP, again.front().kind)
    again.core.emergencyOpening()
    again.phone.pass(10)
    again.phone.paused(SOS)
    again.phone.pass(30)
    again.phone.resumed(SOS)
    again.phone.pass(500)
    assertEquals(FrontKind.EMERGENCY_DIALLER, again.front().kind)
    assertTrue(again.log.lines.contains("front learned emergency_dialler package=$SOS"))
  }

  @Test
  fun onlyTheFirstScreenAfterTheTapMayBeLearned() {
    // Whatever comes up first ends the wait: the dialler (Emergency call's
    // last way), the home screen, LiMo, an app with an icon. A package with
    // no icon after it, well inside the ten seconds, is an app.
    for (first in listOf(PHONE_APP, LAUNCHER, LIMO, GAME)) {
      val rig = Rig()
      rig.open(LAUNCHER)
      rig.open(VIDEO)
      rig.core.emergencyOpening()
      rig.open(first)
      // LiMo's own window events propose nothing: its resume is read with the facts.
      rig.phone.pass(600)
      rig.look()
      assertTrue(first, rig.log.lines.contains("front emergency_wait ended first=$first"))
      rig.open(HELPER)
      assertEquals(first, FrontKind.APP, rig.front().kind)
      assertFalse(first, rig.log.lines.any { it.startsWith("front learned") })
    }
  }

  @Test
  fun nothingIsLearnedWhereAndroidNamesNoDiallerOrUsageIsOffAtTheTap() {
    // A tablet with no calls: Android names no dialler and no emergency
    // dialler, Emergency call opens nothing and the cover stays, so the
    // first screen after it would be one the child chose.
    val tablet = Rig(TestPhone().apply { diallers = emptySet() })
    tablet.open(LAUNCHER)
    tablet.open(GAME)
    tablet.core.emergencyOpening()
    assertTrue(tablet.log.lines.contains("front emergency_wait not_started because=no_dialler"))
    tablet.open(HELPER)
    assertEquals(FrontKind.APP, tablet.front().kind)

    // Usage access off at the tap: once it is back, the record cannot say
    // what came first after the tap.
    val off = Rig()
    off.open(LAUNCHER)
    off.open(GAME)
    off.phone.usageOn = false
    off.core.emergencyOpening()
    assertTrue(off.log.lines.contains("front emergency_wait not_started because=no_usage"))
    off.phone.usageOn = true
    off.open(HELPER)
    assertEquals(FrontKind.APP, off.front().kind)

    for (rig in listOf(tablet, off)) assertFalse(rig.log.lines.any { it.startsWith("front learned") })
  }

  @Test
  fun theWaitEndsAfterTenSecondsWhenTheClockIsSetOrWhenUsageIsRefused() {
    // Ten seconds with nothing new in front.
    val late = Rig()
    late.open(LAUNCHER)
    late.open(GAME)
    late.core.emergencyOpening()
    late.phone.pass(11_000)
    late.open(HELPER)
    assertEquals(FrontKind.APP, late.front().kind)
    assertTrue(late.log.lines.contains("front emergency_wait ended because=ten_seconds"))

    // The clock set an hour on: Android moves its record with it and the
    // record is read again from the start, so a time before the tap and one
    // after it can no longer be told apart.
    val clock = Rig()
    clock.open(LAUNCHER)
    clock.open(GAME)
    clock.core.emergencyOpening()
    clock.phone.pass(1_000)
    clock.phone.wall += 3_600_000L
    clock.phone.record.replaceAll { it.copy(atWallMs = it.atWallMs + 3_600_000L) }
    clock.core.clockSet()
    assertTrue(clock.log.lines.contains("front emergency_wait ended because=clock_set"))
    clock.open(HELPER)
    assertEquals(FrontKind.APP, clock.front().kind)

    // A read of usage refused while the watch waits.
    val refused = Rig()
    refused.open(LAUNCHER)
    refused.open(GAME)
    refused.core.emergencyOpening()
    refused.phone.failing += "usage_events"
    refused.phone.pass(600)
    refused.look()
    assertTrue(refused.log.lines.contains("front emergency_wait ended because=no_usage"))
    refused.phone.failing -= "usage_events"
    refused.open(HELPER)
    assertEquals(FrontKind.APP, refused.front().kind)

    for (rig in listOf(late, clock, refused)) assertFalse(rig.log.lines.any { it.startsWith("front learned") })
  }

  @Test
  fun withUsageAccessOffTheFrontIsUnknownNeverAGuessButADiallerStillSaysSo() {
    val rig = Rig(TestPhone().apply { emergency = setOf(SOS) })
    rig.open(LAUNCHER)
    rig.open(GAME)
    assertEquals(GAME, rig.front().packageName)

    rig.phone.usageOn = false
    rig.phone.pass(600)
    val look = rig.look()
    assertEquals(FrontKind.UNKNOWN, look.facts.front.kind)
    assertEquals(UnknownBecause.USAGE_ACCESS_OFF, look.unknownBecause)
    assertFalse(look.facts.permissions.usageAccess)
    assertFalse(look.permissions.usageAccess)
    assertTrue(look.alsoVisible.isEmpty())

    // A window event alone is never taken for the home screen or an app.
    assertFalse(rig.core.windowEvent(LAUNCHER))
    assertEquals(FrontKind.UNKNOWN, rig.front().kind)
    assertFalse(rig.core.windowEvent(GAME))
    assertEquals(FrontKind.UNKNOWN, rig.front().kind)
    // But one from the dialler or the emergency dialler still says so: a
    // lock must never stand between a child and a call.
    assertTrue(rig.core.windowEvent(PHONE_APP))
    assertEquals(FrontKind.DIALLER, rig.front().kind)
    // LiMo's own windows are its covers: they change nothing.
    assertFalse(rig.core.windowEvent(LIMO))
    assertEquals(FrontKind.DIALLER, rig.front().kind)
    assertTrue(rig.core.windowEvent(GAME))
    assertEquals(FrontKind.UNKNOWN, rig.front().kind)
    assertTrue(rig.core.windowEvent(SOS))
    assertEquals(FrontKind.EMERGENCY_DIALLER, rig.front().kind)
    // Nothing waits for usage while it is off.
    assertNull(rig.core.nextLookInMs())

    // Switched on again: the next window event finds it at once.
    rig.phone.usageOn = true
    rig.open(VIDEO)
    assertEquals(VIDEO, rig.front().packageName)
  }

  @Test
  fun splitScreenPutsTheLastResumedInFrontAndTheOtherBesideIt() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(VIDEO)
    // The game opens in the other half; the video stays resumed beside it.
    rig.phone.pass(30)
    rig.phone.paused(LAUNCHER)
    rig.phone.stopped(LAUNCHER)
    rig.phone.resumed(GAME)
    rig.phone.pass(20)
    assertTrue(rig.core.windowEvent(GAME))
    val look = rig.look()
    assertEquals(GAME, look.facts.front.packageName)
    assertEquals(1, look.alsoVisible.size)
    assertEquals(FrontKind.APP, look.alsoVisible[0].kind)
    assertEquals(VIDEO, look.alsoVisible[0].packageName)
    assertEquals("Tube Time", look.alsoVisible[0].label)
  }

  @Test
  fun pictureInPictureLeavesHomeInFrontWithTheVideoBesideIt() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(VIDEO)
    // Home: the video goes into its small window, paused and not stopped.
    rig.phone.pass(40)
    rig.phone.paused(VIDEO)
    rig.phone.resumed(LAUNCHER)
    rig.phone.pass(10)
    rig.core.windowEvent(LAUNCHER)
    rig.phone.pass(30)
    rig.core.windowEvent(VIDEO)
    rig.settle()
    val look = rig.look()
    assertEquals(FrontKind.HOME, look.facts.front.kind)
    assertEquals(listOf(VIDEO), look.alsoVisible.map { it.packageName })

    // Android 9 and older report no stops: only resumed apps can be told.
    rig.phone.stopsReported = false
    assertTrue(rig.look().alsoVisible.isEmpty())
  }

  @Test
  fun anAppWithNoLabelIsAnAppWithoutOne() {
    val rig = Rig()
    rig.phone.labels["org.example.nolabel"] = "org.example.nolabel"
    rig.phone.labels["org.example.blank"] = "   "
    rig.phone.labels["org.example.padded"] = "  Star Painter  "
    rig.open(LAUNCHER)
    for (pkg in listOf("org.example.nolabel", "org.example.blank", "org.example.unseen")) {
      rig.open(pkg)
      val front = rig.front()
      assertEquals(FrontKind.APP, front.kind)
      assertEquals(pkg, front.packageName)
      assertNull(pkg, front.label)
    }
    rig.open("org.example.padded")
    assertEquals("Star Painter", rig.front().label)
    // A label that cannot be read is no label, never a failure.
    rig.phone.failing += "label"
    rig.open(VIDEO)
    assertEquals(VIDEO, rig.front().packageName)
    assertNull(rig.front().label)
  }

  @Test
  fun aClockSetBackStartsTheRecordAgainAndTheFrontStaysRight() {
    for (told in listOf(true, false)) {
      val rig = Rig()
      rig.open(LAUNCHER)
      rig.open(GAME)
      // The clock is set an hour back. Android moves its stored events with it.
      rig.phone.wall -= 3_600_000L
      rig.phone.record.replaceAll { it.copy(atWallMs = it.atWallMs - 3_600_000L) }
      // Told by ClockReceiver, or not told at all (the broadcast missed):
      // either way the record is read again from the start.
      if (told) assertFalse(rig.core.clockSet())
      assertEquals(GAME, rig.front().packageName)
      rig.open(VIDEO)
      assertEquals(VIDEO, rig.front().packageName)
      rig.open(LAUNCHER)
      assertEquals(FrontKind.HOME, rig.front().kind)
    }
  }

  @Test
  fun aRestartOfThePhoneInTheRecordForgetsWhatCameBefore() {
    val rig = Rig()
    rig.phone.resumed(GAME)
    rig.phone.pass(60_000)
    rig.phone.started()
    rig.phone.pass(5_000)
    // Started, Android's lock screen up, nothing resumed since.
    val look = rig.look()
    assertEquals(FrontKind.UNKNOWN, look.facts.front.kind)
    assertEquals(UnknownBecause.NOTHING_SEEN_YET, look.unknownBecause)
    rig.open(LAUNCHER)
    assertEquals(FrontKind.HOME, rig.front().kind)
  }

  @Test
  fun eventsReadTwiceNearTheEdgeAreTakenOnceAndOnesWrittenLateAreStillTaken() {
    val track = UsageTrack(lookbackMs = 60_000, overlapMs = 2_000)
    val overlay = Overlays::isOverlay
    var window = track.window(100_000)
    assertEquals(UsageTrack.Window(40_000, 100_001), window)
    assertTrue(track.take(listOf(UsageEvent(GAME, UsageKind.RESUMED, 99_000)), window, overlay))
    assertEquals(1L, track.resumes)

    window = track.window(101_000)
    assertEquals(UsageTrack.Window(98_001, 101_001), window)
    val again = listOf(
      UsageEvent(GAME, UsageKind.RESUMED, 99_000), // read before: taken once
      UsageEvent(LAUNCHER, UsageKind.RESUMED, 98_500), // written late, older than the front: does not take it
      UsageEvent(VIDEO, UsageKind.RESUMED, 99_500), // written late, newer: taken
    )
    assertTrue(track.take(again, window, overlay))
    assertEquals(VIDEO, track.front)
    assertEquals(2L, track.resumes)

    // Nothing new: nothing moves.
    window = track.window(102_000)
    assertFalse(track.take(again, window, overlay))
    assertEquals(2L, track.resumes)

    // The clock went back past the last read: the record is read again from the lookback.
    window = track.window(50_000)
    assertEquals(UsageTrack.Window(-10_000, 50_001), window)
    assertNull(track.front)
  }
}
