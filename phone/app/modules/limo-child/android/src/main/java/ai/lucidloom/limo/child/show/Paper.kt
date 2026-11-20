package ai.lucidloom.limo.child.show

import android.content.Context
import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.DashPathEffect
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.view.View

/** Density independent pixels to pixels, for one context. */
internal class Dp(val density: Float) {
  operator fun invoke(v: Float): Float = v * density

  fun px(v: Float): Int = (v * density + 0.5f).toInt()

  companion object {
    fun of(context: Context) = Dp(context.resources.displayMetrics.density)
  }
}

/**
 * A box of the theme: a fill, a border of 1.5 inside its edge (solid or
 * dashed), round corners, a hard shadow set down and to the right with no
 * blur, and the marigold band at the bottom of a field that has the focus.
 *
 * The shadow is drawn outside the box's own bounds, as a browser draws a
 * box shadow: it takes no room, so the parents of a box with a shadow do
 * not clip their children ([Parts.open]).
 */
internal class BoxDrawable(
  private val dp: Dp,
  var fill: Int? = Look.CARD,
  var edge: Int? = Look.INK,
  private val edgeWidth: Float = Look.BORDER_WIDTH,
  private val radius: Float = Look.RADIUS_MD,
  var shadow: Float = 0f,
  var dashed: Boolean = false,
  var band: Boolean = false,
) : Drawable() {
  private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
  private val box = RectF()
  private val path = Path()

  override fun draw(canvas: Canvas) {
    val b = bounds
    box.set(b.left.toFloat(), b.top.toFloat(), b.right.toFloat(), b.bottom.toFloat())
    val r = dp(radius)
    paint.pathEffect = null
    paint.style = Paint.Style.FILL
    if (shadow > 0f) {
      paint.color = Look.SHADOW
      val s = dp(shadow)
      canvas.drawRoundRect(box.left + s, box.top + s, box.right + s, box.bottom + s, r, r, paint)
    }
    fill?.let {
      paint.color = it
      canvas.drawRoundRect(box, r, r, paint)
    }
    val w = if (edge != null) dp(edgeWidth) else 0f
    if (band) {
      // inset 0 -4px 0 marigold: a band inside the border, under the field's last line.
      val save = canvas.save()
      path.reset()
      path.addRoundRect(box.left + w, box.top + w, box.right - w, box.bottom - w, r - w, r - w, Path.Direction.CW)
      canvas.clipPath(path)
      paint.color = Look.MARIGOLD
      canvas.drawRect(box.left, box.bottom - w - dp(Look.FIELD_BAND), box.right, box.bottom, paint)
      canvas.restoreToCount(save)
    }
    edge?.let {
      paint.color = it
      paint.style = Paint.Style.STROKE
      paint.strokeWidth = w
      if (dashed) paint.pathEffect = DashPathEffect(floatArrayOf(dp(Look.DASH), dp(Look.DASH_GAP)), 0f)
      val h = w / 2f
      canvas.drawRoundRect(box.left + h, box.top + h, box.right - h, box.bottom - h, r - h, r - h, paint)
    }
  }

  override fun setAlpha(alpha: Int) = Unit

  override fun setColorFilter(colorFilter: ColorFilter?) = Unit

  @Deprecated("Deprecated in Java")
  override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
}

/**
 * The page: warm paper with a faint dot grid, one dot in the middle of every
 * square of 18, counted from the top left of the window, so that the page
 * and the strip pinned to its bottom line up.
 */
internal class DotsDrawable(private val dp: Dp) : Drawable() {
  private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Look.DOT }

  override fun draw(canvas: Canvas) {
    canvas.drawColor(Look.PAPER)
    val step = dp(Look.DOT_GRID)
    val r = dp(Look.DOT_RADIUS)
    val b = bounds
    var y = step / 2f
    while (y < b.bottom) {
      var x = step / 2f
      while (x < b.right) {
        canvas.drawCircle(x, y, r, paint)
        x += step
      }
      y += step
    }
  }

  override fun setAlpha(alpha: Int) = Unit

  override fun setColorFilter(colorFilter: ColorFilter?) = Unit

  @Deprecated("Deprecated in Java")
  override fun getOpacity(): Int = PixelFormat.OPAQUE
}

/** A dashed divider inside a card: 1.5 high, in the theme's border colour. */
internal class DashLine(context: Context, private val dp: Dp) : View(context) {
  private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
    color = Look.BORDER
    style = Paint.Style.STROKE
  }

  init {
    importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
  }

  override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
    setMeasuredDimension(MeasureSpec.getSize(widthMeasureSpec), dp.px(Look.BORDER_WIDTH))
  }

  override fun onDraw(canvas: Canvas) {
    val w = dp(Look.BORDER_WIDTH)
    paint.strokeWidth = w
    paint.pathEffect = DashPathEffect(floatArrayOf(dp(Look.DASH), dp(Look.DASH_GAP)), 0f)
    canvas.drawLine(0f, w / 2f, width.toFloat(), w / 2f, paint)
  }
}

/**
 * The small marks beside text, plain ink strokes as the drawings make them
 * (never icons): the arrow of a way out, the arrow of Back, the key that
 * removes a digit. Paths in the drawing's own units, stroke 1.8, round ends.
 */
internal class StrokeDrawable(private val dp: Dp, private val mark: Mark, var color: Int = Look.INK) : Drawable() {
  enum class Mark(val w: Float, val h: Float, val paths: List<FloatArray>, val closed: List<Boolean>) {
    RIGHT(16f, 16f, listOf(floatArrayOf(6f, 3f, 11f, 8f, 6f, 13f)), listOf(false)),
    LEFT(16f, 16f, listOf(floatArrayOf(10f, 3f, 5f, 8f, 10f, 13f)), listOf(false)),
    DELETE(
      24f,
      16f,
      listOf(
        floatArrayOf(9f, 2f, 21.5f, 2f, 21.5f, 14f, 9f, 14f, 2.5f, 8f),
        floatArrayOf(12.5f, 5.5f, 17.5f, 10.5f),
        floatArrayOf(17.5f, 5.5f, 12.5f, 10.5f),
      ),
      listOf(true, false, false),
    ),
  }

  private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
    style = Paint.Style.STROKE
    strokeCap = Paint.Cap.ROUND
    strokeJoin = Paint.Join.ROUND
  }
  private val path = Path()

  override fun getIntrinsicWidth(): Int = dp.px(mark.w)

  override fun getIntrinsicHeight(): Int = dp.px(mark.h)

  override fun draw(canvas: Canvas) {
    val b = bounds
    val s = minOf(b.width() / mark.w, b.height() / mark.h)
    val ox = b.left + (b.width() - mark.w * s) / 2f
    val oy = b.top + (b.height() - mark.h * s) / 2f
    paint.color = color
    paint.strokeWidth = 1.8f * s
    for ((i, p) in mark.paths.withIndex()) {
      path.reset()
      path.moveTo(ox + p[0] * s, oy + p[1] * s)
      var j = 2
      while (j < p.size) {
        path.lineTo(ox + p[j] * s, oy + p[j + 1] * s)
        j += 2
      }
      if (mark.closed[i]) path.close()
      canvas.drawPath(path, paint)
    }
  }

  override fun setAlpha(alpha: Int) = Unit

  override fun setColorFilter(colorFilter: ColorFilter?) = Unit

  @Deprecated("Deprecated in Java")
  override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
}

/** A filled circle: a dot of the PIN box. */
internal class DotDrawable(private val dp: Dp, private val size: Float) : Drawable() {
  private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Look.INK }

  override fun getIntrinsicWidth(): Int = dp.px(size)

  override fun getIntrinsicHeight(): Int = dp.px(size)

  override fun draw(canvas: Canvas) {
    val b = bounds
    canvas.drawCircle(b.exactCenterX(), b.exactCenterY(), minOf(b.width(), b.height()) / 2f, paint)
  }

  override fun setAlpha(alpha: Int) = Unit

  override fun setColorFilter(colorFilter: ColorFilter?) = Unit

  @Deprecated("Deprecated in Java")
  override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
}
