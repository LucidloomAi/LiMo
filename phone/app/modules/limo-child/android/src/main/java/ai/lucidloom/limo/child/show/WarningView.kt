package ai.lucidloom.limo.child.show

import android.os.Build
import android.view.Gravity
import android.view.ViewGroup
import android.widget.LinearLayout

/**
 * F: the warning card that floats at the top of the screen over the
 * child's own app: the bell, the engine's message and hint, a dashed line,
 * when the phone locks, and OK. No maker's mark: it floats over the child's
 * own app.
 */
internal object WarningView {
  fun build(parts: Parts, w: WarningPlan, onOk: () -> Unit): LinearLayout = parts.column().also { card ->
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
    val ok = parts.button(Drawn.OK) { onOk() }
    parts.inside(ok, Look.WARN_OK_PAD_H, 0f, Look.WARN_OK_PAD_H, 0f)
    foot.addView(ok, parts.at(ViewGroup.LayoutParams.WRAP_CONTENT, start = Look.NOTE_GAP))
    card.addView(foot)
    // A screen reader says the warning when it comes up.
    if (Build.VERSION.SDK_INT >= 28) card.accessibilityPaneTitle = w.message
  }
}
