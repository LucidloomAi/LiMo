package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.Way
import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Rect
import android.graphics.drawable.Drawable
import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView

/**
 * A screen that fills the display: the dotted paper behind everything
 * (under Android's bars too), a page that scrolls, and a strip pinned to
 * the bottom that the page scrolls above. What LiMo draws keeps clear of
 * the bars, the cut out and the keyboard by the [insets] it is given. On a
 * screen wider than [Look.PAGE_MOST] (a phone on its side, a tablet, an
 * unfolded phone) the page and the strip keep that width in the middle.
 */
@SuppressLint("ViewConstructor")
internal class Screen(context: Context, private val dp: Dp) : FrameLayout(context) {
  val scroll: ScrollView = ScrollView(context)

  /** The page, inside the scroll: padding 4 at the top, 16 at the sides, 6 at the bottom. */
  val page: LinearLayout = LinearLayout(context)

  /** What is pinned to the bottom: Emergency call on a lock, the main button on a way out. */
  val strip: LinearLayout = LinearLayout(context)

  private val body = LinearLayout(context)
  private val insets = Rect()

  init {
    background = DotsDrawable(dp)
    body.orientation = LinearLayout.VERTICAL
    page.orientation = LinearLayout.VERTICAL
    page.clipChildren = false
    page.clipToPadding = false
    page.setPadding(dp.px(Look.SIDE), dp.px(Look.PAGE_TOP), dp.px(Look.SIDE), dp.px(Look.PAGE_BOTTOM))
    scroll.isFillViewport = true
    scroll.isVerticalScrollBarEnabled = false
    scroll.addView(page, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
    strip.orientation = LinearLayout.VERTICAL
    strip.clipChildren = false
    strip.clipToPadding = false
    strip.setPadding(dp.px(Look.SIDE), dp.px(Look.PIN_TOP), dp.px(Look.SIDE), dp.px(Look.PIN_BOTTOM))
    body.addView(scroll, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
    body.addView(strip, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
    addView(body, LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
  }

  /** Android's bars, the cut out and the keyboard, in pixels. */
  fun insets(left: Int, top: Int, right: Int, bottom: Int) {
    if (insets.left == left && insets.top == top && insets.right == right && insets.bottom == bottom) return
    insets.set(left, top, right, bottom)
    requestLayout()
  }

  override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
    val inner = MeasureSpec.getSize(widthMeasureSpec) - insets.left - insets.right
    val extra = maxOf(0, (inner - dp.px(Look.PAGE_MOST)) / 2)
    body.setPadding(insets.left + extra, insets.top, insets.right + extra, insets.bottom)
    super.onMeasure(widthMeasureSpec, heightMeasureSpec)
  }

  /** Adds the pinned buttons, 10 apart. */
  fun pin(views: List<View>) {
    strip.removeAllViews()
    views.forEachIndexed { i, v ->
      val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
      if (i > 0) lp.topMargin = dp.px(Look.PIN_GAP)
      strip.addView(v, lp)
    }
  }
}

/** What a tap on a cover does: Android's part, made up in tests. */
internal interface CoverActions {
  /** The block screen's one button: the home screen, by the accessibility service's own action. */
  fun home()

  /** Android's emergency dialler; it never dials by itself. */
  fun emergency()

  /** The call's own screen. */
  fun backToCall()

  /** An app that stays open under the lock. */
  fun openApp(packageName: String)

  /** A way out, as a screen of its own. */
  fun openWay(way: Way, lock: CoverPlan.Lock)

  /** An app's own icon, from Android. */
  fun appIcon(packageName: String): Drawable?
}

/** Builds the lock (A, A2, C) and the block screen (D) from their plans. */
internal class CoverPage(private val parts: Parts, private val act: CoverActions) {
  /** The top row of the screen built last, so its clock can be moved on. */
  var top: Parts.Top? = null
    private set

  fun build(plan: CoverPlan, time: String, date: String): Screen = when (plan) {
    is CoverPlan.Lock -> lock(plan, time, date)
    is CoverPlan.Block -> block(plan, time, date)
    CoverPlan.None -> Screen(parts.context, parts.dp)
  }.also { s ->
    // A screen reader says the cover's title when it comes up.
    val title = when (plan) {
      is CoverPlan.Lock -> plan.title
      is CoverPlan.Block -> plan.title
      CoverPlan.None -> null
    }
    if (title != null && Build.VERSION.SDK_INT >= 28) s.accessibilityPaneTitle = title
  }

  private fun lock(p: CoverPlan.Lock, time: String, date: String): Screen {
    val s = Screen(parts.context, parts.dp)
    val page = s.page
    val top = parts.Top(time, date).also { this.top = it }
    page.addView(top.view)

    val card = parts.card()
    card.addView(parts.whyTop(p.picture, p.stamp))
    card.addView(parts.text(Look.LABEL, p.eyebrow), parts.at(top = Look.EYEBROW_TOP))
    card.addView(parts.heading(Look.TITLE, p.title, balance = true), parts.at(top = Look.TITLE_TOP))
    card.addView(parts.text(Look.DETAIL, p.message), parts.at(top = Look.DETAIL_TOP))
    p.untilWords?.let { until ->
      card.addView(parts.dash(), parts.at(top = Look.UNTIL_TOP))
      card.addView(parts.text(Look.LABEL, Drawn.OPENS_AGAIN), parts.at(top = Look.UNTIL_PAD))
      card.addView(parts.text(Look.UNTIL, until), parts.at(top = Look.UNTIL_WORDS_TOP))
    }
    page.addView(card, parts.at(top = Look.TOP_TO_CARD))
    page.addView(ways(p))
    if (p.tiles.isNotEmpty()) {
      page.addView(parts.text(Look.LABEL, Drawn.STILL_OPEN), parts.section())
      page.addView(parts.apps(p.tiles, act::appIcon, act::openApp))
    }
    page.addView(parts.spring(), parts.at(height = 0, weight = 1f))
    page.addView(parts.tail(Drawn.NOTHING_CLOSED))
    s.pin(pinned(p.pinned, p.backToCall))
    return s
  }

  /**
   * What you can do: the label, the child's request that waits, the wait
   * after wrong tries, and a row for each way out (or the words when none is
   * offered).
   */
  fun ways(p: CoverPlan.Lock): LinearLayout = parts.column().also { box ->
    box.addView(parts.text(Look.LABEL, Drawn.WHAT_YOU_CAN_DO), parts.section())
    p.request?.let { r ->
      val stamp = parts.smallStamp(r.stamp, r.tone)
      box.addView(parts.note(Picture.HAND, parts.text(Look.NOTE_TEXT, r.text), stamp), parts.at(bottom = Look.NOTE_BOTTOM))
    }
    p.waitWords?.let { w ->
      box.addView(parts.note(Picture.ALERT, parts.text(Look.NOTE_TEXT, w), wait = true), parts.at(bottom = Look.NOTE_BOTTOM))
    }
    if (p.rows.isNotEmpty()) {
      p.rows.forEachIndexed { i, r ->
        box.addView(parts.wayRow(r) { act.openWay(r.way, p) }, parts.at(top = if (i == 0) 0f else Look.ROW_GAP))
      }
    } else {
      p.nothing?.let { n ->
        val words = parts.column().also { c ->
          c.addView(parts.text(Look.ROW_TITLE, n.bold))
          c.addView(parts.text(Look.ROW_SUB, n.line), parts.at(top = Look.ROW_SUB_TOP))
        }
        box.addView(parts.note(null, words))
      }
    }
  }

  private fun block(p: CoverPlan.Block, time: String, date: String): Screen {
    val s = Screen(parts.context, parts.dp)
    val page = s.page
    val top = parts.Top(time, date).also { this.top = it }
    page.addView(top.view)

    val card = parts.card()
    card.addView(parts.whyTop(Picture.BLOCK, Drawn.BLOCKED))
    card.addView(parts.text(Look.LABEL, p.eyebrow), parts.at(top = Look.EYEBROW_TOP))
    card.addView(parts.heading(Look.TITLE, p.title, balance = true), parts.at(top = Look.TITLE_TOP))
    card.addView(parts.text(Look.DETAIL, p.message), parts.at(top = Look.DETAIL_TOP))
    card.addView(parts.button(Drawn.GO_HOME, ButtonKind.INK) { act.home() }, parts.at(top = Look.BLOCK_BUTTON_TOP))

    // The card in the middle of what is left, as the drawing centres it.
    val mid = parts.column()
    mid.setPadding(0, parts.px(Look.MID_TOP), 0, parts.px(Look.MID_BOTTOM))
    mid.addView(parts.spring(), parts.at(height = 0, weight = 1f))
    mid.addView(card)
    mid.addView(parts.spring(), parts.at(height = 0, weight = 1f))
    page.addView(mid, parts.at(height = 0, weight = 1f))
    page.addView(parts.tail(Drawn.NOTHING_ELSE_CLOSED))
    s.pin(pinned(p.pinned, p.backToCall))
    return s
  }

  /** Back to the call in teal, then Emergency call; or Emergency call alone. */
  private fun pinned(pins: List<Pinned>, backToCall: String?): List<View> = pins.map { pin ->
    when (pin) {
      Pinned.BACK_TO_CALL -> parts.button(backToCall.orEmpty(), ButtonKind.TEAL) { act.backToCall() }
      Pinned.EMERGENCY -> parts.button(Drawn.EMERGENCY_CALL) { act.emergency() }
    }
  }
}
