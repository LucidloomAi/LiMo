package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.Cover
import ai.lucidloom.limo.child.ffi.Way
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The shower as a level, with no Android: the same order twice changes
 * nothing, a changed order changes the window, an order ended takes the
 * cover down at once; stepping aside for a screen the child opens from the
 * cover; LiMo's own screen never covered; the warning's ten seconds.
 */
class ShowerCoreTest {
  private val cover = FakeCover()
  private val card = FakeCard()
  private val timer = FakeTimer()
  private val core = ShowerCore(cover, card, timer)
  private val apps = FakeApps()

  private fun show(c: Cover) = core.show(Plans.of(c, "Mira", apps))

  private val warning = WarningPlan("5 minutes of screen time left today.", "Save what you are doing.", "Locks about 9:47 pm")

  @Test
  fun theSameOrderEverySecondDrawsOnceAndOnlyMovesTheClock() {
    show(lock())
    repeat(60) {
      timer.pass(1_000)
      show(lock())
    }
    assertEquals(1, cover.drawn.size)
    assertEquals(60, cover.refreshed)
    assertEquals(0, cover.removed)
    assertTrue(cover.up)
  }

  @Test
  fun anOrderEndedTakesTheCoverDownAtOnceAndNothingIsNeverDrawn() {
    show(NOTHING)
    show(NOTHING)
    assertTrue(cover.drawn.isEmpty())
    show(block())
    assertEquals(1, cover.drawn.size)
    show(NOTHING)
    assertEquals(1, cover.removed)
    assertFalse(cover.up)
    assertEquals(CoverPlan.None, core.drawn)
    // Ended twice: taken down once.
    show(NOTHING)
    assertEquals(1, cover.removed)
  }

  @Test
  fun aChangedOrderChangesTheWindowThatIsUp() {
    show(lock())
    show(lock(lockCover(callOn = true)))
    show(block())
    show(lock(lockCover(ways = listOf(row(Way.CODE), row(Way.PIN)))))
    assertEquals(4, cover.drawn.size)
    assertEquals(0, cover.removed)
    assertTrue(cover.drawn[1] is CoverPlan.Lock && (cover.drawn[1] as CoverPlan.Lock).backToCall == "Back to the call")
    assertTrue(cover.drawn[2] is CoverPlan.Block)
  }

  @Test
  fun aCoverAndroidTookAwayOrCouldNotDrawIsDrawnAgainAtTheNextOrder() {
    cover.refuse = true
    show(lock())
    assertEquals(CoverPlan.None, core.drawn)
    cover.refuse = false
    show(lock())
    assertEquals(1, cover.drawn.size)
    cover.lost()
    show(lock())
    assertEquals(2, cover.drawn.size)
    assertTrue(cover.up)
  }

  @Test
  fun aTapThatOpensAnotherScreenStepsAsideUntilTheEngineSeesIt() {
    show(lock())
    // Emergency call: the cover goes at once, so the emergency dialler is never under it.
    core.stepAside()
    assertFalse(cover.up)
    assertTrue(core.steppingAside)
    // The engine has not seen the dialler yet: the same lock is ordered, and the cover stays aside.
    timer.pass(400)
    show(lock())
    assertFalse(cover.up)
    // The engine sees it: nothing to cover there.
    timer.pass(400)
    show(NOTHING)
    assertFalse(core.steppingAside)
    assertFalse(cover.up)
    // Back to the game: the lock again, at once.
    show(lock())
    assertTrue(cover.up)
  }

  @Test
  fun aCoverStandsAsideAtMostThreeSecondsWhenNothingNewIsOrdered() {
    show(block())
    core.stepAside()
    assertFalse(cover.up)
    timer.pass(2_000)
    show(block())
    assertFalse(cover.up)
    // The timer brings it back even with no new order...
    timer.pass(1_000)
    assertTrue(cover.up)
    assertFalse(core.steppingAside)
    // ...and an order after it keeps it.
    show(block())
    assertTrue(cover.up)
    // Nothing ordered: nothing to step aside from.
    show(NOTHING)
    core.stepAside()
    assertFalse(core.steppingAside)
  }

  @Test
  fun limosOwnScreenIsNeverCoveredAndWhenItClosesTheOrdersLead() {
    show(lock())
    core.stepAside()
    core.limoScreen(up = true)
    assertFalse(cover.up)
    // Even an order of the lock draws nothing over LiMo's own screen.
    show(lock())
    assertFalse(cover.up)
    // The engine sees LiMo in front: nothing ordered.
    show(NOTHING)
    // Back: nothing is drawn from before; the engine's order of nothing stands...
    core.limoScreen(up = false)
    assertFalse(cover.up)
    // ...until it sees the app under the lock again.
    show(lock())
    assertTrue(cover.up)
  }

  @Test
  fun aRingingCallAsTheWaysOutCloseIsNeverCovered() {
    show(lock())
    core.limoScreen(up = true)
    show(NOTHING)
    core.limoScreen(up = false)
    // A call rings: the engine orders nothing, and nothing is drawn, now or later.
    show(NOTHING)
    timer.pass(10_000)
    assertFalse(cover.up)
    assertEquals(1, cover.drawn.size)
  }

  @Test
  fun anOrderEndedWhileSteppingAsideIsFollowedAndTheTimerDrawsNothing() {
    show(lock())
    core.stepAside()
    // The emergency dialler: the engine orders nothing.
    show(NOTHING)
    timer.pass(5_000)
    assertFalse(cover.up)
    assertFalse(core.steppingAside)
  }

  @Test
  fun aWarningFloatsForTenSecondsOrUntilOk() {
    core.warn(warning)
    assertEquals(listOf(warning), card.floated)
    timer.pass(9_999)
    assertTrue("not before ten seconds", card.up)
    timer.pass(1)
    assertFalse("gone at ten seconds", card.up)
    core.warn(warning)
    core.dismissWarning()
    assertFalse(card.up)
    // The timer of a warning gone by OK or a swipe takes nothing away later.
    core.warn(warning)
    timer.pass(4_000)
    assertTrue(card.up)
    timer.pass(6_000)
    assertFalse(card.up)
  }

  @Test
  fun aNewWarningTakesThePlaceOfTheOldAndACoverTakesItAway() {
    core.warn(warning)
    timer.pass(6_000)
    val next = warning.copy(message = "1 minute of screen time left today.")
    core.warn(next)
    timer.pass(6_000)
    assertTrue("the second keeps its own ten seconds", card.up)
    timer.pass(4_000)
    assertFalse(card.up)
    core.warn(next)
    show(lock())
    assertFalse(card.up)
    // Under a cover a warning is not floated.
    core.warn(warning)
    assertEquals(3, card.floated.size)
  }

  @Test
  fun withNoServiceConnectedNoCardIsFloated() {
    card.refuse = true
    core.warn(warning)
    assertFalse(card.up)
    timer.pass(60_000)
    assertEquals(0, card.removed)
  }
}
