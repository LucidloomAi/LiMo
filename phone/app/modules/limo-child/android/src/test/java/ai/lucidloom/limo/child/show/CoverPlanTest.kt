package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.LockWhy
import ai.lucidloom.limo.child.ffi.NothingToDo
import ai.lucidloom.limo.child.ffi.RequestAnswer
import ai.lucidloom.limo.child.ffi.Way
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The lock's and the block screen's layout decisions, from the engine's
 * orders: which rows, which notes, which tiles, which pinned buttons, which
 * words, for each kind of order. No Android.
 */
class CoverPlanTest {
  private val apps = FakeApps()

  private fun lockPlan(c: ai.lucidloom.limo.child.ffi.LockCover = lockCover(), name: String? = "Mira") =
    Plans.of(lock(c), name, apps) as CoverPlan.Lock

  @Test
  fun aLockWithTheTimeUsedUpOffersTheThreeWaysOutAsTheEngineWordsThem() {
    val p = lockPlan()
    assertEquals(Picture.HOURGLASS, p.picture)
    assertEquals("Locked", p.stamp)
    assertEquals("Mira · this phone", p.eyebrow)
    // The engine's plain apostrophes, as the drawings set them.
    assertEquals("Today’s time is used up.", p.title)
    assertEquals("You have used all of today’s screen time.", p.message)
    assertEquals("Tomorrow, 7:00 am", p.untilWords)
    assertEquals(listOf(Way.ASK, Way.CODE, Way.PIN), p.rows.map { it.way })
    assertEquals(listOf(Picture.HAND, Picture.CODE, Picture.PIN), p.rows.map { it.picture })
    assertEquals("A guardian who is away can read you 8 digits", p.rows[1].sub)
    assertTrue(p.rows.none { it.off })
    assertNull(p.request)
    assertNull(p.waitWords)
    assertNull(p.nothing)
    assertEquals(listOf(Pinned.EMERGENCY), p.pinned)
    assertNull(p.backToCall)
  }

  @Test
  fun theTilesAreTheDiallerThenTheAllowedAppsEachOnceOnlyThoseAndroidCanOpenNeverLiMo() {
    val p = lockPlan(lockCover(allowed = listOf(MESSAGES, CLOCK_APP, GONE, MESSAGES, LIMO, DIALLER, MAPS, "")))
    assertEquals(listOf(DIALLER, MESSAGES, CLOCK_APP, MAPS), p.tiles.map { it.packageName })
    assertEquals(listOf("Phone", "Messages", "Clock", "Maps"), p.tiles.map { it.label })
    // No dialler known, no allowed apps: no tiles, so no "Still open" at all.
    val none = Plans.of(lock(lockCover(allowed = emptyList())), "Mira", FakeApps(dialler = null)) as CoverPlan.Lock
    assertTrue(none.tiles.isEmpty())
  }

  @Test
  fun bedtimeOffersOnlyThePinAndTheMoon() {
    val p = lockPlan(lockCover(why = LockWhy.BLOCKED_HOURS, ways = listOf(row(Way.PIN))))
    assertEquals(Picture.MOON, p.picture)
    assertEquals("Screen time is off for now.", p.title)
    assertEquals(listOf(Way.PIN), p.rows.map { it.way })
  }

  @Test
  fun withNoWayOutTheLockSaysTheEnginesWordsInstead() {
    val p = lockPlan(lockCover(why = LockWhy.GUARDIAN_LOCK, ways = emptyList(), nothing = NothingToDo("A guardian can unlock it.", "From LiMo on their phone, or in LiMo on the family PC.")))
    assertTrue(p.rows.isEmpty())
    assertEquals(NothingPlan("A guardian can unlock it.", "From LiMo on their phone, or in LiMo on the family PC."), p.nothing)
    assertEquals(Picture.LOCK, p.picture)
    // Words for none are only used when there is none.
    val with = lockPlan(lockCover(nothing = NothingToDo("x", "y")))
    assertNull(with.nothing)
  }

  @Test
  fun theWaitGreysTheCodeAndThePinAndSaysWhenTheNextTryIs() {
    val p = lockPlan(lockCover(ways = listOf(row(Way.ASK), row(Way.CODE, off = true), row(Way.PIN, off = true)), waitWords = "Too many wrong tries. The next try is at 9:47 pm."))
    assertEquals(listOf(false, true, true), p.rows.map { it.off })
    assertEquals("Wait until 9:47 pm", p.rows[2].sub)
    assertEquals("Too many wrong tries. The next try is at 9:47 pm.", p.waitWords)
  }

  @Test
  fun aRequestThatWaitsIsANoteWithItsStampAndAskingIsNotOfferedAgain() {
    val waiting = lockPlan(lockCover(ways = listOf(row(Way.CODE), row(Way.PIN)), request = RequestAnswer.WAITING))
    assertEquals(RequestPlan("You asked for 30 min at 9:32 pm", "Waiting", Tone.INK), waiting.request)
    assertEquals(listOf(Way.CODE, Way.PIN), waiting.rows.map { it.way })
    assertEquals(Tone.TEAL, lockPlan(lockCover(request = RequestAnswer.GRANTED)).request!!.tone)
    assertEquals("Given 15 min", lockPlan(lockCover(request = RequestAnswer.GRANTED)).request!!.stamp)
    assertEquals(Tone.CORAL, lockPlan(lockCover(request = RequestAnswer.REFUSED)).request!!.tone)
  }

  @Test
  fun duringACallBackToTheCallIsPinnedAboveEmergencyCallOnTheLockAndTheBlockScreen() {
    val p = lockPlan(lockCover(callOn = true))
    assertEquals(listOf(Pinned.BACK_TO_CALL, Pinned.EMERGENCY), p.pinned)
    assertEquals("Back to the call", p.backToCall)
    val b = Plans.of(block(callOn = true), "Mira", apps) as CoverPlan.Block
    assertEquals(listOf(Pinned.BACK_TO_CALL, Pinned.EMERGENCY), b.pinned)
    val plain = Plans.of(block(), "Mira", apps) as CoverPlan.Block
    assertEquals(listOf(Pinned.EMERGENCY), plain.pinned)
    assertNull(plain.backToCall)
  }

  @Test
  fun theBlockScreenNamesTheAppAsTheEngineGivesIt() {
    val b = Plans.of(block(), "Mira", apps) as CoverPlan.Block
    assertEquals("Game Launcher is not allowed.", b.title)
    assertEquals("It is blocked by your family’s rules.", b.message)
    assertEquals("org.example.gamelauncher", b.packageName)
    val nameless = Plans.of(block(label = null), "Mira", apps) as CoverPlan.Block
    assertEquals("This app is not allowed.", nameless.title)
  }

  @Test
  fun theLabelIsTheChildsNameAndThisPhoneOrThisPhoneAloneAndNothingElseChanges() {
    val named = lockPlan(name = "Mira")
    val unnamed = lockPlan(name = null)
    val blank = lockPlan(name = "  ")
    assertEquals("Mira · this phone", named.eyebrow)
    assertEquals("this phone", unnamed.eyebrow)
    assertEquals("this phone", blank.eyebrow)
    assertEquals(named.copy(eyebrow = unnamed.eyebrow), unnamed)
    val b1 = Plans.of(block(), "Asha", apps) as CoverPlan.Block
    val b2 = Plans.of(block(), null, apps) as CoverPlan.Block
    assertEquals("Asha · this phone", b1.eyebrow)
    assertEquals(b1.copy(eyebrow = "this phone"), b2)
  }

  @Test
  fun nothingOrderedIsNothingDrawnAndTheSameOrderIsTheSamePlan() {
    assertEquals(CoverPlan.None, Plans.of(NOTHING, "Mira", apps))
    assertEquals(Plans.of(lock(), "Mira", apps), Plans.of(lock(), "Mira", apps))
    assertFalse(Plans.of(lock(), "Mira", apps) == Plans.of(lock(lockCover(callOn = true)), "Mira", apps))
  }

  @Test
  fun everyReasonHasAPictureOfTheSet() {
    val expected = mapOf(
      LockWhy.BLOCKED_HOURS to "moon",
      LockWhy.NO_TIME_TODAY to "calendar",
      LockWhy.TIME_USED_UP to "hourglass",
      LockWhy.WAITING_FOR_TIME to "sync",
      LockWhy.AWAY_TIME_USED_UP to "away",
      LockWhy.HELD_BY_ANOTHER_DEVICE to "devices",
      LockWhy.GUARDIAN_LOCK to "lock",
      LockWhy.CANNOT_CHECK to "alert",
      LockWhy.APP_BLOCKED to "block",
    )
    assertEquals(LockWhy.entries.toSet(), expected.keys)
    for ((why, icon) in expected) {
      assertEquals(icon, Plans.picture(why).icon)
      assertTrue("$icon is in the design's set", java.io.File(REPO, "design/icons/svg/$icon.svg").isFile)
    }
    for (p in Picture.entries) assertTrue(java.io.File(REPO, "design/icons/svg/${p.icon}.svg").isFile)
  }

  @Test
  fun wordsTheDrawingsSet() {
    assertEquals("Opens tomorrow, 7:00 am", Drawn.opens("Tomorrow, 7:00 am"))
    assertEquals("Opens when a guardian unlocks it", Drawn.opens("When a guardian unlocks it"))
    assertEquals("28 / 200", Drawn.count("To finish my science project", 200))
    // A child's emoji is one character, as the rules core counts them.
    assertEquals("1 / 200", Drawn.count("😀", 200))
    assertEquals("Today’s", Typo.quotes("Today's"))
    assertEquals("'quoted' and family’s", Typo.quotes("'quoted' and family's"))
    val ist = ZoneId.of("Asia/Kolkata")
    // Wednesday 18 November 2026, 9:42 pm in India.
    val at = 1_795_018_320_000L
    assertEquals("9:42 pm", ClockWords.time(at, ist, hours24 = false))
    assertEquals("21:42", ClockWords.time(at, ist, hours24 = true))
    assertEquals("Wed 18 Nov", ClockWords.date(at, ist))
    assertEquals("12:05 am", ClockWords.time(1_794_940_500_000L, ist, hours24 = false))
    // The end of today, for a guardian's "Today": midnight on the phone's clock.
    assertEquals(1_795_026_600L, ClockWords.endOfDay(at / 1000, ist))
  }
}
