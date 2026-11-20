package ai.lucidloom.limo.child.show

import android.annotation.SuppressLint
import android.content.Context
import android.os.Build
import android.os.Bundle
import android.view.Gravity
import android.view.MotionEvent
import android.view.ViewConfiguration
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.LinearLayout

/**
 * F: the warning card that floats at the top of the screen over the
 * child's own app: the bell, the engine's message and hint, a dashed line,
 * when the phone locks, and OK. No maker's mark: it floats over the child's
 * own app. OK puts it away, and so does a swipe up or to either side
 * ([Swipe]); nothing on it shows that it can be swiped, since the drawing
 * has no such mark.
 */
internal object WarningView {
  /** How the card was put away, for the log. */
  enum class Gone { OK, SWIPE }

  fun build(parts: Parts, w: WarningPlan, gone: (Gone) -> Unit): LinearLayout {
    val swipe = Swipe(
      slop = ViewConfiguration.get(parts.context).scaledTouchSlop.toFloat(),
      away = parts.dp(Look.SWIPE_AWAY),
      speed = parts.dp(Look.SWIPE_SPEED),
    )
    val card = SwipeCard(parts.context, swipe) { gone(Gone.SWIPE) }
    parts.open(card)
    card.orientation = LinearLayout.VERTICAL
    card.background = parts.box(radius = Look.RADIUS_LG, shadow = Look.CARD_SHADOW)
    parts.inside(card, Look.WARN_PAD_H, Look.WARN_PAD_TOP, Look.WARN_PAD_H, Look.WARN_PAD_BOTTOM)

    val top = parts.line()
    top.addView(parts.tile(Picture.BELL, Look.WARN_TILE, Look.WARN_TILE_RADIUS), parts.at(parts.px(Look.WARN_TILE), parts.px(Look.WARN_TILE)))
    val words = parts.column()
    words.addView(parts.heading(Look.WARN_HEADING, w.message), parts.at(top = Look.WARN_HEADING_TOP))
    words.addView(parts.text(Look.WARN_HINT, w.hint), parts.at(top = Look.WARN_HINT_TOP))
    top.addView(words, parts.at(0, weight = 1f, start = Look.WARN_GAP))
    card.addView(top)

    card.addView(parts.dash(), parts.at(top = Look.WARN_FOOT_TOP))
    val foot = parts.line()
    foot.gravity = Gravity.CENTER_VERTICAL
    foot.setPadding(0, parts.px(Look.WARN_FOOT_PAD), 0, 0)
    foot.addView(parts.text(Look.LABEL, w.whenWords), parts.at(0, weight = 1f))
    val ok = parts.button(Drawn.OK) { gone(Gone.OK) }
    parts.inside(ok, Look.WARN_OK_PAD_H, 0f, Look.WARN_OK_PAD_H, 0f)
    foot.addView(ok, parts.at(ViewGroup.LayoutParams.WRAP_CONTENT, start = Look.NOTE_GAP))
    card.addView(foot)
    // A screen reader says the warning when it comes up.
    if (Build.VERSION.SDK_INT >= 28) card.accessibilityPaneTitle = w.message
    return card
  }
}

/**
 * The warning card's own view: it sees every touch on it first and gives
 * each to [swipe]. Until a touch is a swipe, it goes on to what is under the
 * finger as usual (OK works, the body does nothing); once it is a swipe,
 * the card takes the rest of it, and [away] is called the moment the swipe
 * is far or quick enough. A screen reader, which cannot swipe on it, is
 * offered the same as "dismiss".
 */
@SuppressLint("ViewConstructor")
internal class SwipeCard(context: Context, private val swipe: Swipe, private val away: () -> Unit) : LinearLayout(context) {
  private var taken = false
  private var gone = false

  @SuppressLint("ClickableViewAccessibility")
  override fun dispatchTouchEvent(e: MotionEvent): Boolean {
    if (gone) return true
    val now = when (e.actionMasked) {
      MotionEvent.ACTION_DOWN -> {
        taken = false
        swipe.down(e.x, e.y, e.eventTime)
      }
      MotionEvent.ACTION_MOVE -> swipe.move(e.x, e.y)
      MotionEvent.ACTION_UP -> swipe.up(e.x, e.y, e.eventTime)
      MotionEvent.ACTION_CANCEL -> {
        swipe.cancel()
        Swipe.Now.OVER
      }
      else -> swipe.now
    }
    if (now == Swipe.Now.SWIPE || now == Swipe.Now.AWAY) {
      if (!taken) {
        // What was under the finger (OK) lets go of the touch.
        taken = true
        val cancel = MotionEvent.obtain(e)
        cancel.action = MotionEvent.ACTION_CANCEL
        super.dispatchTouchEvent(cancel)
        cancel.recycle()
      }
      if (now == Swipe.Now.AWAY) {
        gone = true
        away()
      }
      return true
    }
    if (taken) return true
    val handled = super.dispatchTouchEvent(e)
    // A touch that begins on the card's body stays the card's, so it can become a swipe.
    return handled || e.actionMasked == MotionEvent.ACTION_DOWN
  }

  override fun onInitializeAccessibilityNodeInfo(info: AccessibilityNodeInfo) {
    super.onInitializeAccessibilityNodeInfo(info)
    info.isDismissable = true
    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_DISMISS)
  }

  override fun performAccessibilityAction(action: Int, arguments: Bundle?): Boolean {
    if (action == AccessibilityNodeInfo.ACTION_DISMISS && !gone) {
      gone = true
      away()
      return true
    }
    return super.performAccessibilityAction(action, arguments)
  }
}
