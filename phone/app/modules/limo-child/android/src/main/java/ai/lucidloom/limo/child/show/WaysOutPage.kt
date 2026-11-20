package ai.lucidloom.limo.child.show

import android.annotation.SuppressLint
import android.text.Editable
import android.text.InputFilter
import android.text.InputType
import android.text.TextWatcher
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.RadioButton

/** The reason, kept in sight as a strip at the top of a way out: its picture, the lock's title, when it opens. */
internal data class WayReason(val picture: Picture, val title: String, val untilWords: String?)

/** What the taps on a way out's screen ask for. */
internal interface WaysHandlers {
  fun back()

  fun submit()

  fun emergency()
}

/**
 * B1, B2 and B3: a way out as a screen of its own. The reason as a strip at
 * the top of the card, Back, the way out's own title, its fields and keys,
 * and its main button pinned to the bottom (above the keyboard when one is
 * up); Emergency call, the plain line and the maker's mark at the end of
 * the page. It shows the [model] and nothing else: [refresh] after every
 * change.
 */
internal class WaysOutPage(
  private val parts: Parts,
  private val model: WaysOutModel,
  private val reason: WayReason?,
  private val heading: String,
  private val on: WaysHandlers,
) {
  val screen: Screen = Screen(parts.context, parts.dp)

  private val cells = ArrayList<Pair<PaperText, BoxDrawable>>()
  private val askChips = LinkedHashMap<AskChoice, PaperText>()
  private val pinChips = LinkedHashMap<PinChoice, PaperText>()
  private var dots: LinearLayout? = null
  private var placeholder: PaperText? = null
  private var count: PaperText? = null
  private var message: PaperText? = null

  /** The reason's field, on B1: the one place the phone's keyboard is used. */
  var field: EditText? = null
    private set

  /** The way out's card: the strip, Back, the title, the fields and the keys. */
  val card: LinearLayout = parts.card()

  init {
    reason?.let { card.addView(strip(it)) }
    card.addView(back(), parts.at(ViewGroup.LayoutParams.WRAP_CONTENT, top = Look.BACK_TOP))
    card.addView(parts.heading(Look.HEADING, heading), parts.at(top = Look.HEADING_TOP))
    when (model.panel) {
      Panel.ASK -> ask(card)
      Panel.CODE -> code(card)
      Panel.PIN -> pin(card)
    }
    screen.page.addView(card)
    val end = parts.column()
    end.setPadding(0, parts.px(Look.END_TOP), 0, 0)
    end.addView(parts.button(Drawn.EMERGENCY_CALL) { on.emergency() })
    screen.page.addView(end)
    screen.page.addView(parts.spring(), parts.at(height = 0, weight = 1f))
    screen.page.addView(parts.tail(Drawn.NOTHING_CLOSED, top = Look.TAIL_AFTER_EMERGENCY))
    val main = when (model.panel) {
      Panel.ASK -> Drawn.SEND_REQUEST
      Panel.CODE -> Drawn.USE_THIS_CODE
      Panel.PIN -> Drawn.UNLOCK
    }
    screen.pin(listOf(parts.button(main, ButtonKind.GO) { on.submit() }))
    refresh()
  }

  /** Shows the model as it is now. */
  fun refresh() {
    for ((i, cell) in cells.withIndex()) {
      val d = model.digit(i)
      val shown = d?.toString() ?: ""
      if (cell.first.words.toString() != shown) cell.first.say(shown)
      val at = i == model.typed && model.typed < WaysOutModel.MAX_DIGITS
      if (cell.second.band != at) {
        cell.second.band = at
        cell.first.invalidate()
      }
    }
    dots?.let { row ->
      while (row.childCount > model.typed) row.removeViewAt(row.childCount - 1)
      while (row.childCount < model.typed) {
        val dot = View(parts.context)
        dot.background = DotDrawable(parts.dp, Look.PIN_DOT)
        dot.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        row.addView(dot, parts.at(parts.px(Look.PIN_DOT), parts.px(Look.PIN_DOT), start = if (row.childCount == 0) 0f else Look.PIN_DOT_GAP))
      }
      placeholder?.visibility = if (model.typed == 0) View.VISIBLE else View.GONE
    }
    for ((choice, chip) in askChips) chipLook(chip, choice == model.ask)
    for ((choice, chip) in pinChips) chipLook(chip, choice == model.pin)
    count?.let {
      val words = Drawn.count(field?.text ?: "", WaysOutModel.MAX_REASON)
      if (it.words.toString() != words) it.say(words)
    }
    message?.let { m ->
      val msg = model.message
      val color = if (msg?.right == true) Look.TEAL else Look.CORAL
      if (m.type.color != color) m.type = m.type.copy(color = color)
      if (m.words.toString() != (msg?.text ?: "")) m.say(msg?.text ?: "")
    }
  }

  // The parts

  private fun strip(r: WayReason): View = parts.column().also { col ->
    val row = parts.line()
    row.setPadding(0, 0, 0, parts.px(Look.STRIP_PAD_BOTTOM))
    row.addView(parts.icon(r.picture), parts.at(parts.px(Look.STRIP_ICON), parts.px(Look.STRIP_ICON), top = Look.STRIP_ICON_TOP))
    val words = parts.column()
    words.addView(parts.text(Look.STRIP_TITLE, r.title))
    r.untilWords?.let { words.addView(parts.text(Look.STRIP_UNTIL, Drawn.opens(it)), parts.at(top = Look.STRIP_UNTIL_TOP)) }
    row.addView(words, parts.at(0, weight = 1f, start = Look.STRIP_GAP))
    col.addView(row)
    col.addView(parts.dash())
  }

  private fun back(): View = parts.line().also { b ->
    b.gravity = Gravity.CENTER_VERTICAL
    b.minimumHeight = parts.px(Look.BUTTON_MIN)
    parts.inside(b, Look.BACK_PAD_START, 0f, Look.BACK_PAD_END, 0f)
    b.background = parts.box()
    b.addView(parts.stroke(StrokeDrawable.Mark.LEFT), parts.at(parts.px(Look.CHEVRON), parts.px(Look.CHEVRON)))
    b.addView(parts.text(Look.BUTTON, Drawn.BACK), parts.at(ViewGroup.LayoutParams.WRAP_CONTENT, start = Look.BACK_GAP))
    b.contentDescription = Drawn.BACK
    parts.tappable(b) { on.back() }
  }

  private fun ask(card: LinearLayout) {
    card.addView(parts.label(Drawn.HOW_LONG), parts.at(top = Look.LABEL_GAP_TOP))
    card.addView(chips(AskChoice.entries.map { it to it.label }, askChips) { model.pick(it) }, parts.at(top = Look.CHIPS_TOP))
    val why = parts.label(Drawn.WHY, Drawn.WHY_HINT)
    card.addView(why, parts.at(top = Look.LABEL_GAP_TOP))
    val f = reasonField()
    // A screen reader names the field by the label's own words, "Why".
    ((why as? ViewGroup)?.getChildAt(0) ?: why).labelFor = f.id
    card.addView(f, parts.at(height = parts.px(Look.FIELD_HEIGHT), top = Look.FIELD_TOP))
    field = f
    val c = parts.text(Look.COUNT, "").also { it.gravity = Gravity.END }
    count = c
    card.addView(c, parts.at(top = Look.COUNT_TOP))
    card.addView(parts.text(Look.SMALL, Drawn.ASK_SMALL), parts.at(top = Look.SMALL_TOP))
  }

  private fun code(card: LinearLayout) {
    card.addView(parts.text(Look.NOTE, Drawn.CODE_NOTE), parts.at(top = Look.NOTE_TOP))
    val row = parts.line()
    for (i in 0 until WaysOutModel.MAX_DIGITS) {
      val box = parts.box(radius = Look.CELL_RADIUS)
      val cell = parts.text(Look.CELL, "").also {
        it.gravity = Gravity.CENTER
        it.background = box
        it.minHeight = parts.px(Look.CELL_HEIGHT)
        it.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
      }
      val gap = when (i) {
        0 -> 0f
        4 -> Look.CELL_GAP + Look.CELL_MIDDLE_GAP
        else -> Look.CELL_GAP
      }
      row.addView(cell, parts.at(0, weight = 1f, start = gap))
      cells += cell to box
    }
    card.addView(row, parts.at(top = Look.CELLS_TOP))
    card.addView(messageSlot(), parts.at(top = Look.MESSAGE_TOP))
    card.addView(keys(), parts.at(top = Look.KEYS_TOP))
    card.addView(parts.text(Look.SMALL, Drawn.CODE_SMALL), parts.at(top = Look.SMALL_TOP))
  }

  private fun pin(card: LinearLayout) {
    card.addView(parts.text(Look.NOTE, Drawn.PIN_NOTE), parts.at(top = Look.NOTE_TOP))
    card.addView(parts.label(Drawn.ADD_TIME), parts.at(top = Look.LABEL_GAP_TOP))
    card.addView(chips(PinChoice.entries.filter { it.adds }.map { it to it.label }, pinChips) { model.pick(it) }, parts.at(top = Look.CHIPS_TOP))
    card.addView(parts.label(Drawn.KEEP_OPEN), parts.at(top = Look.LABEL_GAP_TOP))
    card.addView(chips(PinChoice.entries.filter { !it.adds }.map { it to it.label }, pinChips) { model.pick(it) }, parts.at(top = Look.CHIPS_TOP))
    val box = parts.line().also { b ->
      b.gravity = Gravity.CENTER_VERTICAL
      b.minimumHeight = parts.px(Look.PIN_BOX_HEIGHT)
      parts.inside(b, Look.PIN_BOX_PAD_H, 0f, Look.PIN_BOX_PAD_H, 0f)
      b.background = parts.box().also { it.band = true }
      val d = parts.line().also { it.gravity = Gravity.CENTER_VERTICAL }
      dots = d
      b.addView(d, parts.at(ViewGroup.LayoutParams.WRAP_CONTENT))
      val hint = parts.text(Look.PIN_PLACEHOLDER, Drawn.PIN_PLACEHOLDER)
      placeholder = hint
      b.addView(hint, parts.at(ViewGroup.LayoutParams.WRAP_CONTENT))
      // Read as the field it is; the digits are never read out from it.
      b.contentDescription = Drawn.PIN_PLACEHOLDER
      b.isFocusable = true
      for (i in 0 until b.childCount) b.getChildAt(i).importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
    }
    card.addView(box, parts.at(top = Look.PIN_BOX_TOP))
    card.addView(messageSlot(), parts.at(top = Look.MESSAGE_TOP))
    card.addView(keys(), parts.at(top = Look.KEYS_TOP))
  }

  /** A line under the field, with room for two lines kept when it is empty, so nothing moves. */
  private fun messageSlot(): View = parts.text(Look.MESSAGE, "").also {
    it.minHeight = parts.px(Look.MESSAGE_ROOM)
    it.accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE
    message = it
  }

  private fun <T> chips(choices: List<Pair<T, String>>, into: MutableMap<T, PaperText>, pick: (T) -> Unit): View = parts.line().also { row ->
    choices.forEachIndexed { i, (choice, words) ->
      val chip = parts.text(Look.CHIP, words).also {
        it.gravity = Gravity.CENTER
        it.minHeight = parts.px(Look.CHIP_HEIGHT)
        parts.inside(it, Look.CHIP_PAD_H, 0f, Look.CHIP_PAD_H, 0f)
        it.background = parts.box(radius = Look.CHIP_RADIUS)
        it.isClickable = true
        it.isFocusable = true
        it.setOnClickListener {
          pick(choice)
          refresh()
        }
      }
      into[choice] = chip
      // Every chip of a row as high as the highest, when large text wraps one.
      row.addView(chip, parts.at(0, ViewGroup.LayoutParams.MATCH_PARENT, weight = 1f, start = if (i == 0) 0f else Look.CHIP_GAP))
    }
  }

  private fun chipLook(chip: PaperText, on: Boolean) {
    val box = chip.background as BoxDrawable
    val fill = if (on) Look.INK else Look.CARD
    if (box.fill != fill) {
      box.fill = fill
      chip.type = chip.type.copy(color = if (on) Look.CARD else Look.INK)
      chip.invalidate()
    }
    chip.isSelected = on
    chip.accessibilityDelegate = object : View.AccessibilityDelegate() {
      override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(host, info)
        info.className = RadioButton::class.java.name
        info.isCheckable = true
        @Suppress("DEPRECATION")
        info.isChecked = on
      }
    }
  }

  /** The keys: 1 to 9, then a gap, 0 and the key that removes the last digit. */
  private fun keys(): View = parts.column().also { pad ->
    val labels = listOf("1", "2", "3", "4", "5", "6", "7", "8", "9", "", "0", "del")
    labels.chunked(3).forEachIndexed { r, three ->
      val row = parts.line()
      three.forEachIndexed { c, k ->
        val key: View = when (k) {
          "" -> parts.spring()
          "del" -> parts.line().also { d ->
            d.gravity = Gravity.CENTER
            d.background = parts.box()
            d.addView(parts.stroke(StrokeDrawable.Mark.DELETE), parts.at(parts.px(Look.DELETE_WIDTH), parts.px(Look.DELETE_HEIGHT)))
            d.contentDescription = Drawn.REMOVE_DIGIT
            parts.tappable(d) {
              if (model.delete()) refresh()
            }
          }
          else -> parts.text(Look.KEY, k).also { t ->
            t.gravity = Gravity.CENTER
            t.background = parts.box()
            parts.tappable(t) {
              if (model.type(k[0])) refresh()
            }
          }
        }
        key.minimumHeight = parts.px(Look.KEY_HEIGHT)
        row.addView(key, parts.at(0, ViewGroup.LayoutParams.MATCH_PARENT, weight = 1f, start = if (c == 0) 0f else Look.KEY_GAP))
      }
      pad.addView(row, parts.at(top = if (r == 0) 0f else Look.KEY_GAP))
    }
  }

  @SuppressLint("SetTextI18n")
  private fun reasonField(): EditText = EditText(parts.context).also { f ->
    f.id = View.generateViewId()
    Type.style(f, Look.FIELD, parts.assets)
    f.background = parts.box().also { it.band = true }
    parts.inside(f, Look.FIELD_PAD_H, Look.FIELD_PAD_V, Look.FIELD_PAD_H, Look.FIELD_PAD_V)
    f.gravity = Gravity.TOP or Gravity.START
    f.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES or InputType.TYPE_TEXT_FLAG_MULTI_LINE
    f.filters = arrayOf(InputFilter.LengthFilter(WaysOutModel.MAX_REASON))
    f.setLineSpacing(Type.px(parts.context, Look.FIELD.line) - f.paint.fontMetricsInt.let { it.descent - it.ascent }, 1f)
    f.setTextSize(TypedValue.COMPLEX_UNIT_SP, Look.FIELD.size)
    f.setText(model.reason)
    f.addTextChangedListener(object : TextWatcher {
      override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit

      override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) = Unit

      override fun afterTextChanged(s: Editable?) {
        model.reason = s?.toString().orEmpty()
        refresh()
      }
    })
  }
}
