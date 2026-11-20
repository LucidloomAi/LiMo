package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.watch.Rig
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * LiMo's main activity says whether it is LiMo's own screen in front, to the
 * watch's real core, with usage access off, where nothing else tells the
 * watch: in front it is LiMo, never counted and never covered; leaving, in
 * split screen, or after the ways out screen opened over it, it is not.
 */
class OwnScreenTest {
  /** The watch's core with usage access off, and the main activity's words to it. */
  private class Phone {
    val rig = Rig().also { it.phone.usageOn = false }
    val lines = mutableListOf<String>()
    val main = MainScreen(tell = { rig.core.limoScreen(it) }, log = { lines += it })

    fun front(): FrontKind = rig.front().kind

    /** `show/WaysOutActivity` says the same for itself, in its own resume and pause. */
    fun waysOut(shown: Boolean) {
      rig.core.limoScreen(shown)
    }
  }

  @Test
  fun inFrontTheMainActivityIsLiMoWithUsageAccessOffAndLeavingItSaysSo() {
    val p = Phone()
    // Setup, before its usage access step: without LiMo's word the front is unknown, and counted.
    assertEquals(FrontKind.UNKNOWN, p.front())
    p.main.inOrder(foreground = true, split = false)
    assertEquals(FrontKind.LIMO, p.front())
    // Android's settings opened from a step: LiMo left the front.
    p.main.inOrder(foreground = false, split = false)
    assertEquals(FrontKind.UNKNOWN, p.front())
    // Back, and on Today.
    p.main.inOrder(foreground = true, split = false)
    assertEquals(FrontKind.LIMO, p.front())
  }

  @Test
  fun theWaysOutScreenOpenedOverTodayIsLiMoAndTodayAgainAfterIt() {
    val p = Phone()
    p.main.inOrder(foreground = true, split = false)
    // As Android orders it: Today pauses, then the ways out screen resumes.
    p.main.inOrder(foreground = false, split = false)
    p.waysOut(true)
    assertEquals(FrontKind.LIMO, p.front())
    // Closed: the ways out screen pauses, then Today resumes.
    p.waysOut(false)
    p.main.inOrder(foreground = true, split = false)
    assertEquals(FrontKind.LIMO, p.front())

    // Why the word must be given at once: Today's pause told late, after the
    // ways out screen's resume, would leave that screen unknown, and covered.
    val late = Phone()
    late.main.inOrder(foreground = true, split = false)
    late.waysOut(true)
    late.main.inOrder(foreground = false, split = false)
    assertEquals(FrontKind.UNKNOWN, late.front())
  }

  @Test
  fun inSplitScreenTheMainActivityIsNotSaidToBeInFront() {
    val p = Phone()
    p.main.inOrder(foreground = true, split = true)
    assertEquals(FrontKind.UNKNOWN, p.front())
    // Out of split screen with no pause: its window laid out again.
    p.main.laidOut(split = false)
    assertEquals(FrontKind.LIMO, p.front())
    // Into split screen again with no pause.
    p.main.laidOut(split = true)
    assertEquals(FrontKind.UNKNOWN, p.front())
    p.main.laidOut(split = false)
    assertEquals(FrontKind.LIMO, p.front())
    // Gone from the front, a layout says nothing.
    p.main.inOrder(foreground = false, split = false)
    p.main.laidOut(split = false)
    assertEquals(FrontKind.UNKNOWN, p.front())
    // A layout with nothing changed says nothing, so it never undoes the ways out screen's word.
    p.main.inOrder(foreground = true, split = false)
    p.waysOut(false)
    p.main.laidOut(split = false)
    assertEquals(FrontKind.UNKNOWN, p.front())
  }

  @Test
  fun theFirstResumeHandedOverLateIsTakenOnlyWhenNothingNewerCame() {
    // A cold start: Expo hands the first resume over late, and nothing came in order meanwhile.
    val cold = Phone()
    cold.main.handedOver(foreground = true, split = false)
    assertEquals(FrontKind.LIMO, cold.front())

    // The activity paused in order before the late resume arrived: the pause is newer.
    val paused = Phone()
    paused.main.inOrder(foreground = false, split = false)
    paused.main.handedOver(foreground = true, split = false)
    assertEquals(FrontKind.UNKNOWN, paused.front())

    // A late word in split screen is false too.
    val split = Phone()
    split.main.handedOver(foreground = true, split = true)
    assertEquals(FrontKind.UNKNOWN, split.front())
  }

  @Test
  fun aWatchThatFailsNeverThrowsIntoTheActivity() {
    val lines = mutableListOf<String>()
    val main = MainScreen(tell = { error("the watch failed") }, log = { lines += it })
    main.inOrder(foreground = true, split = false)
    main.laidOut(split = true)
    main.handedOver(foreground = false, split = false)
    assertEquals(listOf("seam failed what=own_screen error=IllegalStateException"), lines.distinct())
    assertTrue(lines.none { it.contains("watch failed") })
  }
}
