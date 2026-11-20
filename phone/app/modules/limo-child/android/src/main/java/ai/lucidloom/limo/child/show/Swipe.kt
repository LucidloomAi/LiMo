package ai.lucidloom.limo.child.show

import kotlin.math.abs

/**
 * Whether a touch on the warning card is a swipe that pushes it away: up,
 * or to either side, as a person expects of something that floats at the
 * top of the screen. No Android in here; the card feeds it its touches.
 *
 * - A touch becomes a swipe once it has moved Android's own touch slop
 *   ([slop]); one that only rests on the card and lifts is never a swipe.
 *   Its way is settled then: sideways when it moved more across than up or
 *   down, up when it moved more up than across. A touch that first moves
 *   down is not a swipe, whatever it does after.
 * - A swipe pushes the card away the moment it has gone [away] its way, or
 *   when the finger lifts after a quick flick, at [speed] or faster on
 *   average since the touch began.
 *
 * Sizes are pixels and times milliseconds; [speed] is pixels a millisecond.
 */
internal class Swipe(private val slop: Float, private val away: Float, private val speed: Float) {
  /** What a touch is so far. */
  enum class Now {
    /** Not a swipe, or not yet: the card's own button (OK) works as usual. */
    TOUCH,

    /** A swipe: the card takes the rest of the touch from what is under the finger. */
    SWIPE,

    /** The card is pushed away. Said once for a touch. */
    AWAY,

    /** Over: a tap that lifted, a move down first, a swipe too short and too slow. */
    OVER,
  }

  private enum class Way { NONE, UP, SIDE }

  private var x0 = 0f
  private var y0 = 0f
  private var t0 = 0L
  private var way = Way.NONE

  var now: Now = Now.OVER
    private set

  fun down(x: Float, y: Float, t: Long): Now {
    x0 = x
    y0 = y
    t0 = t
    way = Way.NONE
    now = Now.TOUCH
    return now
  }

  fun move(x: Float, y: Float): Now {
    if (now != Now.TOUCH && now != Now.SWIPE) return now
    val dx = x - x0
    val dy = y - y0
    if (way == Way.NONE) {
      if (abs(dx) < slop && abs(dy) < slop) return now
      way = when {
        abs(dx) > abs(dy) -> Way.SIDE
        dy < 0 -> Way.UP
        else -> {
          now = Now.OVER
          return now
        }
      }
      now = Now.SWIPE
    }
    if (travel(dx, dy) >= away) now = Now.AWAY
    return now
  }

  /**
   * The finger lifts: AWAY for a swipe that reaches [away] here or was
   * quick enough; OVER for anything else, a tap among them.
   */
  fun up(x: Float, y: Float, t: Long): Now {
    if (now != Now.TOUCH && now != Now.SWIPE) {
      now = Now.OVER
      return now
    }
    if (move(x, y) == Now.SWIPE) {
      val ms = (t - t0).coerceAtLeast(1L)
      now = if (travel(x - x0, y - y0) / ms >= speed) Now.AWAY else Now.OVER
      return now
    }
    if (now != Now.AWAY) now = Now.OVER
    return now
  }

  fun cancel() {
    now = Now.OVER
  }

  /** How far it went its way; going back counts as nothing. */
  private fun travel(dx: Float, dy: Float): Float = when (way) {
    Way.SIDE -> abs(dx)
    Way.UP -> (-dy).coerceAtLeast(0f)
    Way.NONE -> 0f
  }
}
