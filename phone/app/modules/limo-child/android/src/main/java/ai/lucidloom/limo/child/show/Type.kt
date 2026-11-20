package ai.lucidloom.limo.child.show

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.text.Layout
import android.text.SpannableString
import android.text.Spanned
import android.text.style.LineHeightSpan
import android.util.TypedValue
import android.widget.TextView
import java.util.Locale

/**
 * The theme's three fonts and the maker's logo, as the views need them.
 *
 * On a phone they come from the child form's own resources: the three
 * variable fonts the app bundles (`res/font/limo_font_body`, `_mono` and
 * `_display`, put there by plugins/with-limo-fonts.js from screens/fonts)
 * and the LucidLoom AI logo. They are looked up by name because they live
 * in the app, not in this module. Anything missing falls back to Android's
 * own sans serif, monospace and serif, and the mark is then said in words
 * alone; the screens still work. The tests hand in the repository's own
 * files instead.
 */
internal class ShowAssets(
  val body: Typeface,
  val mono: Typeface,
  val display: Typeface,
  /** The fonts are the variable fonts, so a weight and an optical size can be set. */
  val variable: Boolean,
  val logo: () -> Drawable?,
) {
  fun face(family: Look.Family): Typeface = when (family) {
    Look.Family.BODY -> body
    Look.Family.MONO -> mono
    Look.Family.DISPLAY -> display
  }

  companion object {
    @Volatile
    private var cached: ShowAssets? = null

    /** The names the logo may have in the app: the module's own first, then the one the screens' bundle gives it. */
    val LOGO_NAMES = listOf("limo_maker_logo", "src_generated_lucidloomlogo")

    @SuppressLint("DiscouragedApi")
    fun of(context: Context): ShowAssets = cached ?: synchronized(this) {
      cached ?: run {
        val res = context.resources
        val pkg = context.packageName
        fun font(name: String): Typeface? = try {
          val id = res.getIdentifier(name, "font", pkg)
          if (id == 0) null else res.getFont(id)
        } catch (e: Exception) {
          null
        }
        val body = font("limo_font_body")
        val mono = font("limo_font_mono")
        val display = font("limo_font_display")
        val all = body != null && mono != null && display != null
        val app = context.applicationContext ?: context
        ShowAssets(
          body = body ?: Typeface.SANS_SERIF,
          mono = mono ?: Typeface.MONOSPACE,
          display = display ?: Typeface.SERIF,
          variable = all,
          logo = {
            LOGO_NAMES.firstNotNullOfOrNull { name ->
              try {
                val id = app.resources.getIdentifier(name, "drawable", app.packageName)
                if (id == 0) null else app.getDrawable(id)
              } catch (e: Exception) {
                null
              }
            }
          },
        ).also { cached = it }
      }
    }
  }
}

/**
 * A line height as CSS sets it: every line exactly [height] pixels, with the
 * text centred in it (half the leading above, half below), so that the
 * views stand where the drawings put them.
 */
internal class CssLine(private val height: Int) : LineHeightSpan {
  override fun chooseHeight(text: CharSequence?, start: Int, end: Int, spanstartv: Int, lineHeight: Int, fm: Paint.FontMetricsInt) {
    val natural = fm.descent - fm.ascent
    val extra = height - natural
    val above = Math.floorDiv(extra, 2)
    fm.ascent -= above
    fm.descent += extra - above
    fm.top = fm.ascent
    fm.bottom = fm.descent
  }
}

/** Sets text in one of the table's kinds of text. */
internal object Type {
  fun px(context: Context, sp: Float): Float =
    TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, sp, context.resources.displayMetrics)

  /** The kind of text, without its words: family, weight, optical size, size, spacing, colour. */
  fun style(view: TextView, type: Look.Type, assets: ShowAssets) {
    val face = assets.face(type.family)
    view.typeface = face
    val axes = if (type.family == Look.Family.DISPLAY) "'wght' ${type.weight}, 'opsz' ${type.size.toInt()}" else "'wght' ${type.weight}"
    if (!assets.variable || !view.setFontVariationSettings(axes)) {
      view.setTypeface(face, if (type.weight >= 600) Typeface.BOLD else Typeface.NORMAL)
    }
    view.setTextSize(TypedValue.COMPLEX_UNIT_SP, type.size)
    view.letterSpacing = type.spacing
    view.setTextColor(type.color)
    view.includeFontPadding = false
    view.hyphenationFrequency = Layout.HYPHENATION_FREQUENCY_NONE
  }
}

/**
 * A text of the screens: it knows its kind of text, so its words are set in
 * capitals where the kind says so (and read by a screen reader as they were
 * written), with every line the table's height.
 */
@SuppressLint("ViewConstructor")
internal class PaperText(context: Context, private val assets: ShowAssets, type: Look.Type) : TextView(context) {
  var type: Look.Type = type
    set(value) {
      field = value
      Type.style(this, value, assets)
      say(words)
    }

  /** The words as given, before capitals. */
  var words: CharSequence = ""
    private set

  /**
   * As CSS `text-wrap: pretty` sets the drawings' reading text: a last line
   * of one word takes the word before it down with it. For text set from
   * the start of its line only.
   */
  var pretty = false

  init {
    Type.style(this, type, assets)
  }

  override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
    super.onMeasure(widthMeasureSpec, heightMeasureSpec)
    if (!pretty) return
    val l = layout ?: return
    val n = l.lineCount
    if (n < 2) return
    val s = l.text
    val lastStart = l.getLineStart(n - 1)
    val last = s.subSequence(lastStart, l.getLineEnd(n - 1)).toString().trim()
    if (last.isEmpty() || last.contains(' ')) return
    val prevStart = l.getLineStart(n - 2)
    val prev = s.subSequence(prevStart, lastStart).toString().trimEnd()
    val cut = prev.lastIndexOf(' ')
    if (cut <= 0) return
    val sides = totalPaddingLeft + totalPaddingRight
    val room = measuredWidth - sides
    val narrow = Math.ceil(l.getPrimaryHorizontal(prevStart + cut).toDouble()).toInt()
    // Never at the cost of a much narrower paragraph.
    if (narrow < room * 2 / 3 || narrow >= room) return
    val width = measuredWidth
    super.onMeasure(MeasureSpec.makeMeasureSpec(narrow + sides, MeasureSpec.EXACTLY), heightMeasureSpec)
    // The lines are laid out narrower; the view keeps the room it was given.
    setMeasuredDimension(width, measuredHeight)
  }

  fun say(words: CharSequence) {
    this.words = words
    val kept = Typo.timesWhole(words.toString())
    val shown = if (type.caps) kept.uppercase(Locale.ROOT) else kept
    val s = SpannableString(shown)
    s.setSpan(CssLine(Type.px(context, type.line).toInt()), 0, s.length, Spanned.SPAN_INCLUSIVE_INCLUSIVE)
    text = s
    contentDescription = if (type.caps) words else null
  }
}
