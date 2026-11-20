package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.module.R
import android.content.Context
import android.graphics.drawable.Drawable
import android.os.Build
import android.text.Layout
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Button
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout

/** How a button is filled: the card, marigold for the main one, ink, or teal for the way back to a call. */
internal enum class ButtonKind { PLAIN, GO, INK, TEAL }

/**
 * The parts the covers and the ways out are made of, as native views, each
 * measured from the drawings ([Look]). Every view group here leaves its
 * children unclipped, so the hard shadows can fall outside the boxes as a
 * browser draws them.
 */
internal class Parts(val context: Context, val assets: ShowAssets) {
  val dp: Dp = Dp.of(context)

  fun px(v: Float): Int = dp.px(v)

  // Layout

  fun open(group: ViewGroup): ViewGroup {
    group.clipChildren = false
    group.clipToPadding = false
    return group
  }

  fun column(): LinearLayout = LinearLayout(context).also {
    it.orientation = LinearLayout.VERTICAL
    open(it)
  }

  fun line(): LinearLayout = LinearLayout(context).also {
    it.orientation = LinearLayout.HORIZONTAL
    open(it)
  }

  /** Layout of a child in a [LinearLayout], with margins in dp. */
  fun at(
    width: Int = ViewGroup.LayoutParams.MATCH_PARENT,
    height: Int = ViewGroup.LayoutParams.WRAP_CONTENT,
    top: Float = 0f,
    start: Float = 0f,
    end: Float = 0f,
    bottom: Float = 0f,
    weight: Float = 0f,
    gravity: Int = -1,
  ): LinearLayout.LayoutParams = LinearLayout.LayoutParams(width, height, weight).also {
    it.setMargins(px(start), px(top), px(end), px(bottom))
    it.marginStart = px(start)
    it.marginEnd = px(end)
    it.gravity = gravity
  }

  fun size(v: Float): Int = px(v)

  /** A space that takes what is left in a column. */
  fun spring(): View = View(context).also { it.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO }

  // Text

  /**
   * A text. [balance] is CSS `text-wrap: balance` (lines of even length, as
   * the reason's title and the plain line at the end); the drawings' reading
   * text is set `pretty` (no word alone on its last line).
   */
  fun text(type: Look.Type, words: CharSequence, balance: Boolean = false): PaperText =
    PaperText(context, assets, type).also {
      it.say(words)
      it.breakStrategy = if (balance) Layout.BREAK_STRATEGY_BALANCED else Layout.BREAK_STRATEGY_HIGH_QUALITY
      it.pretty = !balance && PRETTY.any { p -> p.copy(color = type.color) == type }
    }

  fun heading(type: Look.Type, words: CharSequence, balance: Boolean = false): PaperText = text(type, words, balance).also {
    if (Build.VERSION.SDK_INT >= 28) it.isAccessibilityHeading = true
  }

  /** A mono label in capitals, with a hint in reading type beside it ("WHY you can leave this empty"). */
  fun label(words: String, hint: String? = null): View {
    val l = text(Look.LABEL, words)
    if (hint == null) return l
    return line().also { row ->
      row.isBaselineAligned = true
      row.addView(l, at(ViewGroup.LayoutParams.WRAP_CONTENT))
      row.addView(text(Look.LABEL_HINT, hint), at(ViewGroup.LayoutParams.WRAP_CONTENT, start = Look.LABEL_HINT_GAP))
    }
  }

  // Boxes

  /**
   * Padding inside a box with a border. CSS puts the border outside the
   * padding, and the box's drawable draws it inside the view, so the
   * content starts after both.
   */
  fun inside(v: View, start: Float, top: Float, end: Float, bottom: Float, edge: Float = Look.BORDER_WIDTH) {
    v.setPaddingRelative(px(start + edge), px(top + edge), px(end + edge), px(bottom + edge))
  }

  fun box(
    fill: Int? = Look.CARD,
    edge: Int? = Look.INK,
    radius: Float = Look.RADIUS_MD,
    shadow: Float = 0f,
    dashed: Boolean = false,
    edgeWidth: Float = Look.BORDER_WIDTH,
  ) = BoxDrawable(dp, fill, edge, edgeWidth, radius, shadow, dashed)

  /** A card: lighter paper, an ink border, corners of 14 and the hard shadow of 4. */
  fun card(): LinearLayout = column().also {
    it.background = box(radius = Look.RADIUS_LG, shadow = Look.CARD_SHADOW)
    inside(it, Look.CARD_PAD_H, Look.CARD_PAD_V, Look.CARD_PAD_H, Look.CARD_PAD_V)
  }

  /** The stamp of the reason card: mono capitals in a coral outline, tilted. */
  fun stamp(words: String): PaperText = text(Look.STAMP, words).also {
    it.background = box(fill = null, edge = Look.CORAL, radius = Look.RADIUS_SM, edgeWidth = Look.STAMP_BORDER)
    inside(it, Look.STAMP_PAD_START, Look.STAMP_PAD_TOP, Look.STAMP_PAD_END, Look.STAMP_PAD_BOTTOM, Look.STAMP_BORDER)
    it.rotation = Look.STAMP_TILT
  }

  /** A small straight stamp: "WAITING", "GIVEN 15 MIN", "NOT THIS TIME". */
  fun smallStamp(words: String, tone: Tone): PaperText {
    val color = when (tone) {
      Tone.INK -> Look.INK
      Tone.TEAL -> Look.TEAL
      Tone.CORAL -> Look.CORAL
    }
    return text(Look.SMALL_STAMP.copy(color = color), words).also {
      it.background = box(fill = null, edge = color, radius = Look.SMALL_STAMP_RADIUS)
      inside(it, Look.SMALL_STAMP_PAD_H, Look.SMALL_STAMP_PAD_TOP, Look.SMALL_STAMP_PAD_H, Look.SMALL_STAMP_PAD_BOTTOM)
      it.maxLines = 1
    }
  }

  fun dash(): View = DashLine(context, dp)

  // Pictures

  fun icon(picture: Picture): ImageView = ImageView(context).also {
    it.setImageResource(drawable(picture))
    it.scaleType = ImageView.ScaleType.FIT_CENTER
    it.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
  }

  /** An icon on a tile of the quieter paper, so a pale drawing still has an edge on a card. */
  fun tile(picture: Picture, size: Float = Look.TILE, radius: Float = Look.TILE_RADIUS): FrameLayout = FrameLayout(context).also {
    open(it)
    it.background = box(fill = Look.PAPER2, edge = null, radius = radius)
    it.addView(icon(picture), FrameLayout.LayoutParams(px(Look.TILE_ICON), px(Look.TILE_ICON), Gravity.CENTER))
    it.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
  }

  fun stroke(mark: StrokeDrawable.Mark, color: Int = Look.INK): ImageView = ImageView(context).also {
    it.setImageDrawable(StrokeDrawable(dp, mark, color))
    it.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
  }

  // Taps

  /** Makes [view] something to tap, read by a screen reader with [role] (a button unless said). */
  fun tappable(view: View, role: String = Button::class.java.name, onTap: () -> Unit) {
    view.isClickable = true
    view.isFocusable = true
    view.setOnClickListener { onTap() }
    view.accessibilityDelegate = object : View.AccessibilityDelegate() {
      override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(host, info)
        info.className = role
      }
    }
  }

  /** A button, at least 48 high, with the hard shadow of 3. */
  fun button(words: String, kind: ButtonKind = ButtonKind.PLAIN, onTap: () -> Unit): PaperText {
    val (fill, edge, ink) = when (kind) {
      ButtonKind.PLAIN -> Triple(Look.CARD, Look.INK, Look.INK)
      ButtonKind.GO -> Triple(Look.MARIGOLD, Look.INK, Look.INK)
      ButtonKind.INK -> Triple(Look.INK, Look.INK, Look.CARD)
      ButtonKind.TEAL -> Triple(Look.CARD, Look.TEAL, Look.TEAL)
    }
    return text(Look.BUTTON.copy(color = ink), words).also {
      it.background = box(fill = fill, edge = edge, shadow = Look.BUTTON_SHADOW)
      it.gravity = Gravity.CENTER
      it.minHeight = px(Look.BUTTON_MIN)
      inside(it, Look.BUTTON_PAD_H, 0f, Look.BUTTON_PAD_H, 0f)
      tappable(it, onTap = onTap)
    }
  }

  // The parts of a lock

  /** LiMo, and the clock with today's date: the top of a lock and of the block screen. */
  inner class Top(time: String, date: String) {
    val view: LinearLayout = line()
    private val time: PaperText = text(Look.NOW, time).also { it.gravity = Gravity.END }
    private val date: PaperText = text(Look.LABEL, date).also { it.gravity = Gravity.END }

    init {
      view.setPadding(px(Look.TOP_PAD), px(Look.TOP_PAD), px(Look.TOP_PAD), 0)
      val mark = line().also { m ->
        m.gravity = Gravity.CENTER_VERTICAL
        m.setPadding(0, px(Look.TOP_PAD), 0, 0)
        m.addView(icon(Picture.HOURGLASS), at(px(Look.MARK_ICON), px(Look.MARK_ICON)))
        m.addView(text(Look.MARK, Drawn.LIMO), at(ViewGroup.LayoutParams.WRAP_CONTENT, start = Look.MARK_GAP))
      }
      view.addView(mark, at(0, weight = 1f, gravity = Gravity.TOP))
      val now = column().also { n ->
        n.gravity = Gravity.END
        n.addView(this.time, at(ViewGroup.LayoutParams.WRAP_CONTENT, gravity = Gravity.END))
        n.addView(this.date, at(ViewGroup.LayoutParams.WRAP_CONTENT, top = Look.NOW_GAP, gravity = Gravity.END))
      }
      view.addView(now, at(ViewGroup.LayoutParams.WRAP_CONTENT))
    }

    fun set(time: String, date: String) {
      if (this.time.words.toString() != time) this.time.say(time)
      if (this.date.words.toString() != date) this.date.say(date)
    }
  }

  /** The reason card's top: the picture of the reason at 64, and the tilted stamp. */
  fun whyTop(picture: Picture, stamp: String): View = line().also { row ->
    row.addView(icon(picture), at(px(Look.WHY_ICON), px(Look.WHY_ICON)))
    row.addView(spring(), at(0, 1, weight = 1f))
    row.addView(stamp(stamp), at(ViewGroup.LayoutParams.WRAP_CONTENT, top = Look.STAMP_MARGIN_TOP, end = Look.STAMP_MARGIN_END, gravity = Gravity.TOP))
  }

  /** A section's label: "WHAT YOU CAN DO", "STILL OPEN". */
  fun section(): LinearLayout.LayoutParams = at(top = Look.SECTION_TOP, start = Look.SECTION_SIDE, end = Look.SECTION_SIDE, bottom = Look.SECTION_BOTTOM)

  /** One way out: its tile, its title and line, and an arrow; dashed and pale while it must wait. */
  fun wayRow(r: RowPlan, onTap: () -> Unit): LinearLayout = line().also { row ->
    row.gravity = Gravity.CENTER_VERTICAL
    row.minimumHeight = px(Look.ROW_MIN)
    inside(row, Look.ROW_PAD_START, Look.ROW_PAD_V, Look.ROW_PAD_END, Look.ROW_PAD_V)
    row.background = if (r.off) box(dashed = true) else box(shadow = Look.BUTTON_SHADOW)
    row.addView(tile(r.picture), at(px(Look.TILE), px(Look.TILE)))
    val words = column().also { c ->
      c.addView(text(Look.ROW_TITLE, r.title))
      c.addView(text(Look.ROW_SUB, r.sub), at(top = Look.ROW_SUB_TOP))
    }
    row.addView(words, at(0, weight = 1f, start = Look.ROW_INNER_GAP))
    if (!r.off) {
      row.addView(stroke(StrokeDrawable.Mark.RIGHT), at(px(Look.CHEVRON), px(Look.CHEVRON), start = Look.ROW_INNER_GAP))
      tappable(row, onTap = onTap)
    } else {
      row.alpha = Look.OFF_ALPHA
      row.isEnabled = false
      row.isFocusable = true
    }
  }

  /**
   * A note above the rows: the child's request that waits (dashed, with its
   * stamp), or the wait after wrong tries (coral), or what the lock says
   * when it offers no way out.
   */
  fun note(picture: Picture?, words: View, stamp: View? = null, wait: Boolean = false): LinearLayout = line().also { box ->
    box.gravity = Gravity.CENTER_VERTICAL
    inside(box, Look.NOTE_PAD_START, Look.NOTE_PAD_TOP, Look.NOTE_PAD_END, Look.NOTE_PAD_BOTTOM)
    box.background = if (wait) {
      box(fill = Look.WAIT_WASH, edge = Look.CORAL)
    } else {
      box(fill = null, edge = Look.REQUEST_EDGE, dashed = true)
    }
    if (picture != null) box.addView(icon(picture), at(px(Look.NOTE_ICON), px(Look.NOTE_ICON), end = Look.NOTE_GAP))
    box.addView(words, at(0, weight = 1f))
    if (stamp != null) box.addView(stamp, at(ViewGroup.LayoutParams.WRAP_CONTENT, start = Look.NOTE_GAP))
    box.isFocusable = true
  }

  /** The apps that stay open: each its own icon and name from Android, four to a row; a tap opens it. */
  fun apps(tiles: List<TilePlan>, iconOf: (String) -> Drawable?, onTap: (String) -> Unit): LinearLayout = column().also { grid ->
    tiles.chunked(Look.APPS_PER_ROW).forEachIndexed { i, chunk ->
      val row = line()
      for (t in chunk) {
        val cell = column().also { c ->
          c.gravity = Gravity.CENTER_HORIZONTAL
          c.minimumHeight = px(Look.TAP_MIN)
          val img = ImageView(context)
          img.setImageDrawable(iconOf(t.packageName))
          img.scaleType = ImageView.ScaleType.FIT_CENTER
          img.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
          c.addView(img, at(px(Look.APP_ICON), px(Look.APP_ICON), gravity = Gravity.CENTER_HORIZONTAL))
          val name = text(Look.APP_NAME, t.label).also { n ->
            n.gravity = Gravity.CENTER_HORIZONTAL
            n.maxLines = 2
            n.ellipsize = android.text.TextUtils.TruncateAt.END
          }
          c.addView(name, at(top = Look.APP_GAP))
          tappable(c) { onTap(t.packageName) }
        }
        row.addView(cell, at(0, weight = 1f))
      }
      // An empty place keeps the columns in step on a last row that is not full.
      repeat(Look.APPS_PER_ROW - chunk.size) { row.addView(spring(), at(0, 1, weight = 1f)) }
      grid.addView(row, at(top = if (i == 0) 0f else Look.APP_ROW_GAP))
    }
  }

  /** The end of the scrolling part: the plain line, then the maker's mark (not a link). */
  fun tail(foot: String, top: Float = Look.TAIL_TOP): LinearLayout = column().also { t ->
    t.setPadding(0, px(top), 0, 0)
    val line = text(Look.FOOT, foot, balance = true).also { it.gravity = Gravity.CENTER_HORIZONTAL }
    t.addView(line, at(start = Look.FOOT_SIDE, end = Look.FOOT_SIDE))
    t.addView(maker(), at(ViewGroup.LayoutParams.WRAP_CONTENT, top = Look.MAKER_TOP, gravity = Gravity.CENTER_HORIZONTAL))
  }

  /** "MADE BY", the LucidLoom AI logo in its own colours at 22, and the name. */
  fun maker(): LinearLayout = line().also { m ->
    m.gravity = Gravity.CENTER_VERTICAL
    m.addView(text(Look.MAKER_WORDS, Drawn.MADE_BY), at(ViewGroup.LayoutParams.WRAP_CONTENT))
    val logo = assets.logo()
    if (logo != null) {
      val img = ImageView(context)
      img.setImageDrawable(logo)
      img.scaleType = ImageView.ScaleType.FIT_CENTER
      m.addView(img, at(px(Look.MAKER_LOGO), px(Look.MAKER_LOGO), start = Look.MAKER_GAP + Look.MAKER_LOGO_START))
    }
    m.addView(text(Look.MAKER_NAME, Drawn.MAKER), at(ViewGroup.LayoutParams.WRAP_CONTENT, start = Look.MAKER_GAP))
    m.contentDescription = "${Drawn.MADE_BY} ${Drawn.MAKER}"
    m.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
    for (i in 0 until m.childCount) m.getChildAt(i).importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
  }

  private companion object {
    /** The kinds of text the drawings set `text-wrap: pretty`. */
    val PRETTY = listOf(Look.DETAIL, Look.ROW_SUB, Look.NOTE_TEXT, Look.NOTE, Look.SMALL, Look.MESSAGE)
  }

  private fun drawable(p: Picture): Int = when (p) {
    Picture.HOURGLASS -> R.drawable.limo_icon_hourglass
    Picture.MOON -> R.drawable.limo_icon_moon
    Picture.CALENDAR -> R.drawable.limo_icon_calendar
    Picture.SYNC -> R.drawable.limo_icon_sync
    Picture.AWAY -> R.drawable.limo_icon_away
    Picture.DEVICES -> R.drawable.limo_icon_devices
    Picture.LOCK -> R.drawable.limo_icon_lock
    Picture.ALERT -> R.drawable.limo_icon_alert
    Picture.BLOCK -> R.drawable.limo_icon_block
    Picture.HAND -> R.drawable.limo_icon_hand
    Picture.CODE -> R.drawable.limo_icon_code
    Picture.PIN -> R.drawable.limo_icon_pin
    Picture.BELL -> R.drawable.limo_icon_bell
  }
}
