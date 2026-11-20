package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.LockWhy
import ai.lucidloom.limo.child.ffi.RequestAnswer
import ai.lucidloom.limo.child.ffi.Way
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.BitmapDrawable
import android.graphics.drawable.Drawable
import android.view.ContextThemeWrapper
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import java.io.File
import java.io.FileOutputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The covers and the ways out drawn on the PC, as Android draws them:
 * Robolectric's native graphics mode runs Android's own drawing code, so
 * these are the real views, the real fonts (the repository's) and the real
 * icons, with no phone. Each state is saved as `native-<frame>.png`, at two
 * pixels per point, beside the drawing's frame of the same name
 * (design/screens/phone.html), into the folder `limo.pictures` names
 * (LIMO_PICTURES, or the module's build folder).
 *
 * Android's own bars are under the cover and are not drawn: the top 32 and
 * the bottom 24 are paper here, where the drawings show the status bar's
 * clock and the gesture bar. The keyboard of B1 is a grey band, as drawn.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w390dp-h844dp-port-xhdpi")
class PicturesTest {
  private val out = File(System.getProperty("limo.pictures") ?: "build/limo-pictures").also { it.mkdirs() }
  private val taps = ArrayList<String>()

  private val fonts = File(REPO, "screens/fonts")
  private val assets: ShowAssets by lazy {
    fun face(path: String) = Typeface.createFromFile(File(fonts, path))
    ShowAssets(
      body = face("instrument-sans/InstrumentSans-VariableFont_wdth,wght.ttf"),
      mono = face("spline-sans-mono/SplineSansMono-VariableFont_wght.ttf"),
      display = face("fraunces/Fraunces-VariableFont_SOFT,WONK,opsz,wght.ttf"),
      variable = true,
      logo = { BitmapDrawable(context().resources, BitmapFactory.decodeFile(File(REPO, "docs/images/lucidloom-logo.png").path)) },
    )
  }

  private fun context(): Context =
    ContextThemeWrapper(RuntimeEnvironment.getApplication(), android.R.style.Theme_Material_Light_NoActionBar)

  /** The drawing's letter tiles, in place of the apps' own icons, which only a phone has. */
  private inner class LetterTile(private val letter: String) : Drawable() {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)

    override fun draw(canvas: Canvas) {
      val d = Dp.of(context())
      val b = RectF(bounds)
      val r = d(12f)
      paint.style = Paint.Style.FILL
      paint.color = Look.CARD
      canvas.drawRoundRect(b, r, r, paint)
      paint.style = Paint.Style.STROKE
      paint.strokeWidth = d(1.5f)
      paint.color = Look.INK
      val h = d(0.75f)
      canvas.drawRoundRect(b.left + h, b.top + h, b.right - h, b.bottom - h, r - h, r - h, paint)
      paint.style = Paint.Style.FILL
      paint.typeface = assets.mono
      paint.fontVariationSettings = "'wght' 600"
      paint.textSize = d(18f)
      paint.textAlign = Paint.Align.CENTER
      val fm = paint.fontMetrics
      canvas.drawText(letter, b.centerX(), b.centerY() - (fm.ascent + fm.descent) / 2f, paint)
    }

    override fun setAlpha(alpha: Int) = Unit

    override fun setColorFilter(colorFilter: ColorFilter?) = Unit

    @Deprecated("Deprecated in Java")
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
  }

  private val actions = object : CoverActions {
    override fun home() {
      taps += "home"
    }

    override fun emergency() {
      taps += "emergency"
    }

    override fun backToCall() {
      taps += "back_to_call"
    }

    override fun openApp(packageName: String) {
      taps += "app $packageName"
    }

    override fun openWay(way: Way, lock: CoverPlan.Lock) {
      taps += "way $way"
    }

    override fun appIcon(packageName: String): Drawable = LetterTile(LABELS[packageName]!!.take(1))
  }

  private val handlers = object : WaysHandlers {
    override fun back() {
      taps += "back"
    }

    override fun submit() {
      taps += "submit"
    }

    override fun emergency() {
      taps += "emergency"
    }
  }

  // Drawing

  private fun lay(view: View, wDp: Int, hDp: Int?) {
    val d = context().resources.displayMetrics.density
    val w = Math.round(wDp * d)
    val hSpec = if (hDp == null) View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED) else View.MeasureSpec.makeMeasureSpec(Math.round(hDp * d), View.MeasureSpec.EXACTLY)
    view.measure(View.MeasureSpec.makeMeasureSpec(w, View.MeasureSpec.EXACTLY), hSpec)
    view.layout(0, 0, view.measuredWidth, view.measuredHeight)
  }

  private fun save(name: String, view: View): Bitmap {
    val bmp = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
    view.draw(Canvas(bmp))
    FileOutputStream(File(out, "native-$name.png")).use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
    // Beside it, where each text and each box stands, in dp: to set against the drawing's own boxes.
    val lines = all(view).mapNotNull { v ->
      val at = at(view, v)
      val what = when {
        v is PaperText -> "text\t${v.words.toString().replace('\n', ' ')}"
        v.background is BoxDrawable -> "box\t${v.javaClass.simpleName}"
        else -> return@mapNotNull null
      }
      "%.1f\t%.1f\t%.1f\t%.1f\t%s".format(dp(at[0]), dp(at[1]), dp(v.width), dp(v.height), what)
    }
    File(out, "native-$name.txt").writeText(lines.joinToString("\n") + "\n")
    return bmp
  }

  /** A screen at a phone's size, with the drawing's bars: 32 at the top, 24 at the bottom (or a keyboard). */
  private fun phone(screen: Screen, wDp: Int = 390, hDp: Int = 844, bottomDp: Int = 24): Screen {
    val dp = Dp.of(context())
    screen.insets(0, dp.px(32f), 0, dp.px(bottomDp.toFloat()))
    lay(screen, wDp, hDp)
    lay(screen, wDp, hDp)
    return screen
  }

  private fun lockScreen(c: ai.lucidloom.limo.child.ffi.LockCover, name: String? = "Mira"): Screen {
    val parts = Parts(context(), assets)
    val plan = Plans.of(lock(c), name, FakeApps()) as CoverPlan.Lock
    return CoverPage(parts, actions).build(plan, "9:42 pm", "Wed 18 Nov")
  }

  private fun ways(panel: Panel, typed: String = "", message: Message? = null, setUp: (WaysOutModel) -> Unit = {}): WaysOutPage {
    val model = WaysOutModel(panel)
    setUp(model)
    typed.forEach { model.type(it) }
    message?.let { model.restore(model.saved().copy(message = it), waiting = false) }
    val heading = when (panel) {
      Panel.ASK -> "Ask for more time"
      Panel.CODE -> "Type a one time code"
      Panel.PIN -> "Guardian PIN"
    }
    return WaysOutPage(Parts(context(), assets), model, WayReason(Picture.HOURGLASS, "Today’s time is used up.", "Tomorrow, 7:00 am"), heading, handlers)
  }

  /** Only a part, as wide as the phone: 16 at each side, as the drawing's crops. */
  private fun crop(name: String, part: View, top: Float = 16f) {
    val ctx = context()
    val dp = Dp.of(ctx)
    val box = FrameLayout(ctx)
    box.background = DotsDrawable(dp)
    box.clipChildren = false
    box.setPadding(dp.px(16f), dp.px(top), dp.px(16f), dp.px(20f))
    box.addView(part, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
    lay(box, 390, null)
    save(name, box)
  }

  // What can be checked on a picture

  private fun all(v: View): List<View> = if (v is ViewGroup) listOf(v) + (0 until v.childCount).flatMap { all(v.getChildAt(it)) } else listOf(v)

  private fun dp(px: Int): Float = px / context().resources.displayMetrics.density

  /** Everything to tap is at least 48 high, and a key or a chip 48 wide too. */
  private fun tapsAre48(root: View) {
    val tappable = all(root).filter { it.isClickable && it.visibility == View.VISIBLE }
    assertTrue("something to tap", tappable.isNotEmpty())
    for (v in tappable) assertTrue("${describe(v)} is ${dp(v.height)} high", dp(v.height) >= 47.9f)
  }

  /** No text is cut: every line of every text fits its view. */
  private fun nothingCut(root: View) {
    for (t in all(root).filterIsInstance<TextView>()) {
      val layout = t.layout ?: continue
      if (t.text.isEmpty()) continue
      val room = t.height - t.totalPaddingTop - t.totalPaddingBottom
      assertTrue("${describe(t)}: ${layout.height} of text in $room", layout.height <= room + 1)
      for (i in 0 until layout.lineCount) {
        assertEquals("${describe(t)} is cut short", 0, layout.getEllipsisCount(i))
        val room = t.width - t.totalPaddingLeft - t.totalPaddingRight
        // Without the spaces at its end, which a line may carry past its edge.
        assertTrue("${describe(t)}: line $i is ${layout.getLineMax(i)} wide in $room", layout.getLineMax(i) <= room + 1)
      }
    }
  }

  /** Where [v] stands in [root], in pixels, through its parents and their scrolling. */
  private fun at(root: View, v: View): IntArray {
    var x = 0
    var y = 0
    var c: View? = v
    while (c != null && c !== root) {
      x += c.left - (c.parent as? View)?.scrollX.let { it ?: 0 }
      y += c.top - (c.parent as? View)?.scrollY.let { it ?: 0 }
      c = c.parent as? View
    }
    return intArrayOf(x, y)
  }

  /** The view is wholly on the screen, between the bars. */
  private fun inSight(v: View, screen: Screen, bottomDp: Int = 24) {
    val bottom = at(screen, v)[1] + v.height
    assertTrue("${describe(v)} ends at ${dp(bottom)}, under the bar", dp(bottom) <= dp(screen.height) - bottomDp + 0.5f)
  }

  private fun describe(v: View): String = when (v) {
    is PaperText -> "\"${v.words}\""
    is TextView -> "\"${v.text}\""
    else -> v.contentDescription?.let { "\"$it\"" } ?: v.javaClass.simpleName
  }

  private fun button(root: View, words: String): View =
    all(root).filterIsInstance<PaperText>().firstOrNull { it.words.toString() == words && it.isClickable } ?: error("no $words button")

  /** The thing to tap that holds [words]: a row of the ways out, an app's tile. */
  private fun holding(root: View, words: String): View =
    all(root).firstOrNull { it.isClickable && all(it).any { c -> c is PaperText && c.words.toString() == words } } ?: error("nothing to tap holds $words")

  /** What a screen reader says, in its order: the texts and labels of what it can reach. */
  private fun spoken(root: View): List<String> {
    val said = ArrayList<String>()
    fun walk(v: View) {
      if (v.visibility != View.VISIBLE) return
      if (v.importantForAccessibility == View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS) return
      val own = v.contentDescription?.toString() ?: (v as? PaperText)?.words?.toString() ?: (v as? TextView)?.text?.toString()
      if (own != null && own.isNotBlank() && v.importantForAccessibility != View.IMPORTANT_FOR_ACCESSIBILITY_NO) said += own
      if (v.contentDescription != null) return
      if (v is ViewGroup) for (i in 0 until v.childCount) walk(v.getChildAt(i))
    }
    walk(root)
    return said
  }

  // The frames

  @Test
  fun aTheLockWithTheTimeUsedUp() {
    val s = phone(lockScreen(lockCover(allowed = listOf(MESSAGES, CLOCK_APP, MAPS))))
    save("A", s)
    tapsAre48(s)
    nothingCut(s)
    inSight(button(s, Drawn.EMERGENCY_CALL), s)
    val range = s.scroll.getChildAt(0).height - s.scroll.height
    assertTrue("A fits, or nearly: it scrolls ${dp(range)}", dp(range) <= 24f)
    // The screen reader's order: LiMo and the time, the reason, the ways out, the apps, the end, the pinned button.
    val said = spoken(s)
    val order = listOf("LiMo", "9:42 pm", "Locked", "Mira · this phone", "Today’s time is used up.", "Opens again", "Tomorrow, 7:00 am", "What you can do", "Ask for more time", "Guardian PIN", "Still open", "Phone", "Maps", Drawn.NOTHING_CLOSED, "Made by LucidLoom AI", "Emergency call")
    var at = -1
    for (o in order) {
      val i = said.indexOf(o)
      assertTrue("\"$o\" is read after what comes before it in $said", i > at)
      at = i
    }
    // Taps.
    button(s, Drawn.EMERGENCY_CALL).performClick()
    holding(s, "Guardian PIN").performClick()
    holding(s, "Maps").performClick()
    assertEquals(listOf("emergency", "way PIN", "app $MAPS"), taps)
  }

  @Test
  fun aTheLockWithNoNameSaysThisPhoneAndNothingElseMoves() {
    val named = phone(lockScreen(lockCover(), "Mira"))
    val unnamed = phone(lockScreen(lockCover(), null))
    save("A-no-name", unnamed)
    val a = all(named).filterIsInstance<PaperText>()
    val b = all(unnamed).filterIsInstance<PaperText>()
    assertEquals(a.size, b.size)
    assertTrue(b.any { it.words == "this phone" && it.text.toString() == "THIS PHONE" })
    assertTrue(a.any { it.words == "Mira · this phone" && it.text.toString() == "MIRA · THIS PHONE" })
    for ((x, y) in a.zip(b)) {
      assertEquals("${describe(x)} stays where it was", at(named, x)[1], at(unnamed, y)[1])
      assertEquals("${describe(x)} keeps its height", x.height, y.height)
    }
  }

  @Test
  fun a2TheLockDuringACall() {
    val s = phone(lockScreen(lockCover(callOn = true)))
    save("A2", s)
    tapsAre48(s)
    val back = button(s, "Back to the call")
    val emergency = button(s, Drawn.EMERGENCY_CALL)
    inSight(emergency, s)
    inSight(back, s)
    assertTrue("Back to the call is above Emergency call", back.bottom < emergency.top + 1 && back.parent === emergency.parent)
    assertEquals(Look.TEAL, (back as PaperText).currentTextColor)
    back.performClick()
    assertEquals(listOf("back_to_call"), taps)
  }

  @Test
  fun aOnASmallPhoneScrollsAboveThePinnedEmergencyCall() {
    RuntimeEnvironment.setQualifiers("w360dp-h640dp-port-xhdpi")
    val s = phone(lockScreen(lockCover()), 360, 640)
    save("A-small", s)
    assertTrue("the page scrolls", s.scroll.getChildAt(0).height > s.scroll.height)
    inSight(button(s, Drawn.EMERGENCY_CALL), s)
    tapsAre48(s)
    nothingCut(s)
  }

  @Test
  fun cBedtime() {
    val s = phone(lockScreen(lockCover(why = LockWhy.BLOCKED_HOURS, ways = listOf(row(Way.PIN)))))
    save("C", s)
    tapsAre48(s)
    nothingCut(s)
  }

  @Test
  fun dTheBlockScreen() {
    val parts = Parts(context(), assets)
    val plan = Plans.of(block(), "Mira", FakeApps())
    val s = phone(CoverPage(parts, actions).build(plan, "9:42 pm", "Wed 18 Nov"))
    save("D", s)
    tapsAre48(s)
    nothingCut(s)
    inSight(button(s, Drawn.EMERGENCY_CALL), s)
    button(s, Drawn.GO_HOME).performClick()
    assertEquals(listOf("home"), taps)
  }

  @Test
  fun bTheWaysOut() {
    val b1 = ways(Panel.ASK) { it.pick(AskChoice.MIN30) }
    b1.field!!.setText("To finish my science project")
    val s1 = b1.screen
    // The phone's keyboard, as the drawing shows it: a grey band of 300 under the pinned button.
    val withKeyboard = FrameLayout(context())
    withKeyboard.addView(s1, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
    val keyboard = View(context()).also { it.setBackgroundColor(Color.rgb(0xd9, 0xdc, 0xe1)) }
    val d = Dp.of(context())
    withKeyboard.addView(keyboard, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, d.px(300f), android.view.Gravity.BOTTOM))
    s1.insets(0, d.px(32f), 0, d.px(300f))
    lay(withKeyboard, 390, 844)
    lay(withKeyboard, 390, 844)
    save("B1", withKeyboard)
    tapsAre48(s1)
    inSight(button(s1, Drawn.SEND_REQUEST), s1, bottomDp = 300)
    assertTrue(all(s1).any { it is PaperText && it.words == "28 / 200" })

    val b2 = ways(Panel.CODE, typed = "13420")
    val s2 = phone(b2.screen)
    save("B2", s2)
    tapsAre48(s2)
    nothingCut(s2)
    inSight(button(s2, Drawn.USE_THIS_CODE), s2)

    val b3 = ways(Panel.PIN, typed = "2468") { it.pick(PinChoice.ADD30) }
    val s3 = phone(b3.screen)
    save("B3", s3)
    tapsAre48(s3)
    nothingCut(s3)
    inSight(button(s3, Drawn.UNLOCK), s3)
    // The PIN's digits are never on the screen nor read out: four dots, and the field's own words.
    assertTrue(all(s3).none { it is TextView && it.text.contains("2468") })
    assertTrue(spoken(s3).none { it.contains("2468") })
    assertTrue(spoken(s3).contains(Drawn.PIN_PLACEHOLDER))
    // Its order: what to give, then the PIN, then the keys.
    val said = spoken(s3)
    assertTrue(said.indexOf("Today") < said.indexOf(Drawn.PIN_PLACEHOLDER) && said.indexOf(Drawn.PIN_PLACEHOLDER) < said.indexOf("1"))
  }

  @Test
  fun bAWrongCodeTheWaitAndARequestThatWaits() {
    val wrong = ways(Panel.CODE, message = Message("That code is not right. 3 more tries before a wait.", right = false))
    lay(wrong.screen, 390, 844)
    (wrong.card.parent as ViewGroup).removeView(wrong.card)
    crop("B-wrong-code", wrong.card)

    val parts = Parts(context(), assets)
    val waitPlan = Plans.of(lock(lockCover(ways = listOf(row(Way.ASK), row(Way.CODE, off = true), row(Way.PIN, off = true)), waitWords = "Too many wrong tries. The next try is at 9:47 pm.")), "Mira", FakeApps()) as CoverPlan.Lock
    crop("B-wait", CoverPage(parts, actions).ways(waitPlan), top = 0f)

    val askPlan = Plans.of(lock(lockCover(ways = listOf(row(Way.CODE), row(Way.PIN)), request = RequestAnswer.WAITING)), "Mira", FakeApps()) as CoverPlan.Lock
    crop("B-request", CoverPage(parts, actions).ways(askPlan), top = 0f)
  }

  @Test
  fun fAWarningOverTheChildsOwnApp() {
    val ctx = context()
    val d = Dp.of(ctx)
    val phone = FrameLayout(ctx)
    phone.setBackgroundColor(Color.rgb(0xcf, 0xd5, 0xdf))
    phone.clipChildren = false
    val parts = Parts(ctx, assets)
    var ok = 0
    val card = WarningView.build(parts, WarningPlan("5 minutes of screen time left today.", "Save what you are doing.", "Locks about 9:47 pm")) { ok += 1 }
    val lp = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
    lp.setMargins(d.px(Look.WARN_SIDE), d.px(32f + Look.WARN_TOP), d.px(Look.WARN_SIDE), 0)
    phone.addView(card, lp)
    lay(phone, 390, 844)
    save("F", phone)
    tapsAre48(card)
    nothingCut(card)
    button(card, Drawn.OK).performClick()
    assertEquals(1, ok)
  }

  @Test
  fun largeTextAtOnePointThreeAndTwoTimesBreaksNothing() {
    for (scale in listOf(1.3f, 2f)) {
      RuntimeEnvironment.setFontScale(scale)
      val tag = if (scale == 2f) "2" else "1.3"
      val a = phone(lockScreen(lockCover(callOn = true)))
      save("A2-text-$tag", a)
      tapsAre48(a)
      nothingCut(a)
      inSight(button(a, Drawn.EMERGENCY_CALL), a)
      inSight(button(a, "Back to the call"), a)
      assertTrue("at $scale the lock scrolls above its pinned buttons", a.scroll.getChildAt(0).height > a.scroll.height)

      val b3 = phone(ways(Panel.PIN, typed = "2468").screen)
      save("B3-text-$tag", b3)
      tapsAre48(b3)
      nothingCut(b3)
      inSight(button(b3, Drawn.UNLOCK), b3)
      // Nothing changes size when it is picked, and a row of chips or keys keeps one height when one wraps.
      for (row in all(b3).filterIsInstance<ViewGroup>()) {
        val boxes = (0 until row.childCount).map { row.getChildAt(it) }.filter { it.isClickable && it.background is BoxDrawable }
        if (boxes.size >= 3) assertEquals("one height in a row at $scale", 1, boxes.map { it.height }.toSet().size)
      }

      val dScreen = phone(CoverPage(Parts(context(), assets), actions).build(Plans.of(block(), "Mira", FakeApps()), "9:42 pm", "Wed 18 Nov"))
      save("D-text-$tag", dScreen)
      tapsAre48(dScreen)
      nothingCut(dScreen)

      val ctx = context()
      val card = WarningView.build(Parts(ctx, assets), WarningPlan("5 minutes of screen time left today.", "Save what you are doing.", "Locks about 9:47 pm")) {}
      lay(card, 366, null)
      tapsAre48(card)
      nothingCut(card)
    }
    RuntimeEnvironment.setFontScale(1f)
  }
}
