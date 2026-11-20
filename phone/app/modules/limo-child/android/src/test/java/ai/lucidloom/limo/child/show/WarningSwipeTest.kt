package ai.lucidloom.limo.child.show

import android.content.Context
import android.graphics.Typeface
import android.view.ContextThemeWrapper
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The warning card (F) as a person handles it: real touches on the real
 * card, built as the shower builds it and wired to a real [ShowerCore], so
 * that what puts it away is what puts it away on a phone. It goes after ten
 * seconds and not before; OK puts it away; a swipe up, or to either side,
 * pushes it away at once; a touch that only rests on the card, a tap on its
 * body, a slow short drag and a swipe down do not.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w390dp-h844dp-port-xhdpi")
class WarningSwipeTest {
  private val context: Context = ContextThemeWrapper(RuntimeEnvironment.getApplication(), android.R.style.Theme_Material_Light_NoActionBar)
  private val dp = Dp.of(context)
  private val parts: Parts by lazy {
    val fonts = File(REPO, "screens/fonts")
    fun face(path: String) = Typeface.createFromFile(File(fonts, path))
    Parts(
      context,
      ShowAssets(
        body = face("instrument-sans/InstrumentSans-VariableFont_wdth,wght.ttf"),
        mono = face("spline-sans-mono/SplineSansMono-VariableFont_wght.ttf"),
        display = face("fraunces/Fraunces-VariableFont_SOFT,WONK,opsz,wght.ttf"),
        variable = true,
        logo = { null },
      ),
    )
  }

  private val warning = WarningPlan("5 minutes of screen time left today.", "Save what you are doing.", "Locks about 9:47 pm")
  private val gone = ArrayList<WarningView.Gone>()

  /** The card's window, made up: the real card, as CoverShower builds it, laid out 366 wide as on a phone 390 wide. */
  private inner class ViewCard : CardSurface {
    var view: ViewGroup? = null
    var removed = 0

    override val up: Boolean get() = view != null

    override fun float(warning: WarningPlan): Boolean {
      val card = WarningView.build(parts, warning) { how ->
        gone += how
        core.dismissWarning()
      }
      card.measure(View.MeasureSpec.makeMeasureSpec(dp.px(366f), View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED))
      card.layout(0, 0, card.measuredWidth, card.measuredHeight)
      view = card
      return true
    }

    override fun remove() {
      if (view != null) removed += 1
      view = null
    }
  }

  private val timer = FakeTimer()
  private val card = ViewCard()
  private val core = ShowerCore(FakeCover(), card, timer)

  /** A touch through these points (dp in the card, ms from its start): down at the first, moves between, up at the last. */
  private fun touch(vararg points: Triple<Float, Float, Long>) {
    val v = card.view ?: error("no card")
    val t0 = 1_000L
    points.forEachIndexed { i, (x, y, t) ->
      val action = when (i) {
        0 -> MotionEvent.ACTION_DOWN
        points.lastIndex -> MotionEvent.ACTION_UP
        else -> MotionEvent.ACTION_MOVE
      }
      val e = MotionEvent.obtain(t0, t0 + t, action, dp(x), dp(y), 0)
      v.dispatchTouchEvent(e)
      e.recycle()
    }
  }

  private fun p(x: Float, y: Float, t: Long) = Triple(x, y, t)

  /** The body of the card: beside the bell, on the message. Nothing there is a button. */
  private val bodyX = 150f
  private val bodyY = 30f

  private fun ok(): View {
    fun find(v: View): View? = when {
      v is PaperText && v.words.toString() == Drawn.OK -> v
      v is ViewGroup -> (0 until v.childCount).firstNotNullOfOrNull { find(v.getChildAt(it)) }
      else -> null
    }
    return find(card.view!!) ?: error("no OK")
  }

  /** Where the middle of [v] is in the card, in dp. */
  private fun middle(v: View): Pair<Float, Float> {
    var x = v.width / 2f
    var y = v.height / 2f
    var c: View = v
    while (c !== card.view) {
      x += c.left
      y += c.top
      c = c.parent as View
    }
    return x / dp.density to y / dp.density
  }

  @Test
  fun itGoesAfterTenSecondsAndNotBefore() {
    core.warn(warning)
    assertTrue(card.up)
    timer.pass(9_999)
    assertTrue("still there at 9.999 seconds", card.up)
    timer.pass(1)
    assertFalse("gone at ten seconds", card.up)
    assertTrue("by itself, not by a tap", gone.isEmpty())
  }

  @Test
  fun okPutsItAwayAtOnce() {
    core.warn(warning)
    ok().performClick()
    assertFalse(card.up)
    assertEquals(listOf(WarningView.Gone.OK), gone)
    // Its timer is gone with it: nothing is taken away later.
    timer.pass(20_000)
    assertEquals(1, card.removed)
  }

  @Test
  fun aSwipeUpPushesItAwayTheMomentItIsFarEnough() {
    core.warn(warning)
    touch(p(bodyX, bodyY, 0), p(bodyX, bodyY - 10f, 40), p(bodyX, bodyY - 30f, 80), p(bodyX, bodyY - 50f, 120), p(bodyX, bodyY - 60f, 160))
    assertFalse(card.up)
    assertEquals(listOf(WarningView.Gone.SWIPE), gone)
  }

  @Test
  fun aSwipeToEitherSidePushesItAway() {
    core.warn(warning)
    touch(p(bodyX, bodyY, 0), p(bodyX + 20f, bodyY + 2f, 50), p(bodyX + 60f, bodyY + 4f, 100), p(bodyX + 70f, bodyY + 4f, 150))
    assertFalse("to the right", card.up)
    core.warn(warning)
    touch(p(bodyX, bodyY, 0), p(bodyX - 20f, bodyY - 3f, 50), p(bodyX - 60f, bodyY - 6f, 100), p(bodyX - 70f, bodyY - 6f, 150))
    assertFalse("to the left", card.up)
    assertEquals(listOf(WarningView.Gone.SWIPE, WarningView.Gone.SWIPE), gone)
  }

  @Test
  fun aQuickShortFlickPushesItAwayAndASlowShortDragDoesNot() {
    core.warn(warning)
    // 30 up in 500 ms: too short, too slow.
    touch(p(bodyX, bodyY, 0), p(bodyX, bodyY - 15f, 250), p(bodyX, bodyY - 30f, 500))
    assertTrue(card.up)
    // 30 up in 20 ms: a flick.
    touch(p(bodyX, bodyY, 0), p(bodyX, bodyY - 15f, 10), p(bodyX, bodyY - 30f, 20))
    assertFalse(card.up)
  }

  @Test
  fun aTouchThatOnlyRestsOnTheBodyOrTapsItDoesNot() {
    core.warn(warning)
    touch(p(bodyX, bodyY, 0), p(bodyX, bodyY, 100))
    assertTrue("a tap on the body", card.up)
    // A finger resting a second and a half, trembling less than Android's touch slop.
    touch(p(bodyX, bodyY, 0), p(bodyX + 1f, bodyY - 1f, 500), p(bodyX - 1f, bodyY + 1f, 1_000), p(bodyX, bodyY, 1_500))
    assertTrue("a resting touch", card.up)
    assertTrue(gone.isEmpty())
  }

  @Test
  fun aSwipeDownDoesNotEvenIfItTurnsUpAfter() {
    core.warn(warning)
    touch(p(bodyX, bodyY, 0), p(bodyX, bodyY + 20f, 50), p(bodyX, bodyY + 80f, 100), p(bodyX, bodyY + 80f, 150))
    assertTrue(card.up)
    touch(p(bodyX, bodyY, 0), p(bodyX, bodyY + 20f, 50), p(bodyX, bodyY - 60f, 100), p(bodyX, bodyY - 60f, 150))
    assertTrue(card.up)
    assertTrue(gone.isEmpty())
  }

  @Test
  fun aTapOnOkIsOkAndASwipeThatStartsOnOkIsASwipe() {
    core.warn(warning)
    val (x, y) = middle(ok())
    // A swipe up from OK: the card takes the touch and OK lets go of it.
    touch(p(x, y, 0), p(x, y - 20f, 40), p(x, y - 60f, 80), p(x, y - 70f, 120))
    assertEquals(listOf(WarningView.Gone.SWIPE), gone)
    assertFalse(card.up)
  }

  @Test
  fun aScreenReaderCanDismissIt() {
    core.warn(warning)
    val v = card.view!!
    val info = v.createAccessibilityNodeInfo()
    assertTrue(info.isDismissable)
    assertTrue(info.actionList.contains(AccessibilityNodeInfo.AccessibilityAction.ACTION_DISMISS))
    assertTrue(v.performAccessibilityAction(AccessibilityNodeInfo.ACTION_DISMISS, null))
    assertFalse(card.up)
  }

  @Test
  fun theCardStillHasItsOneButtonAndItsWords() {
    core.warn(warning)
    val all = ArrayList<View>()
    fun walk(v: View) {
      all += v
      if (v is ViewGroup) for (i in 0 until v.childCount) walk(v.getChildAt(i))
    }
    walk(card.view!!)
    assertEquals(listOf(Drawn.OK), all.filter { it.isClickable }.map { (it as PaperText).words.toString() })
    assertEquals(
      listOf("5 minutes of screen time left today.", "Save what you are doing.", "Locks about 9:47 pm", Drawn.OK),
      all.filterIsInstance<PaperText>().map { it.words.toString() },
    )
  }
}
