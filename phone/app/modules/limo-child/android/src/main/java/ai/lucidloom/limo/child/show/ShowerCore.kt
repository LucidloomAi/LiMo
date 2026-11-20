package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.WarningShown

/** Where a cover is drawn: an overlay window on a phone, a record in tests. Main thread only. */
internal interface CoverSurface {
  /** True while a cover's window is up, attached to the accessibility service that is connected now. */
  val up: Boolean

  /**
   * Draws [plan] (never [CoverPlan.None]): in a new window when none is up,
   * or by changing what the window up shows. False when it cannot be drawn
   * (the accessibility service is not connected, or Android refused the
   * window); then nothing is up.
   */
  fun draw(plan: CoverPlan): Boolean

  /** Takes the cover's window away at once, if one is up. */
  fun remove()

  /** The same cover, a second later: only the clock at its top may change. */
  fun refresh()
}

/** Where the warning card floats. Main thread only. */
internal interface CardSurface {
  val up: Boolean

  /** Floats [warning] at the top of the screen, in place of one already up; false when it cannot. */
  fun float(warning: WarningPlan): Boolean

  fun remove()
}

/** A timer on the main thread. */
internal interface ShowTimer {
  fun after(ms: Long, block: () -> Unit): AutoCloseable

  /** Milliseconds on a clock that only goes forward. */
  fun nowMs(): Long
}

/** F: a warning over the child's own app, in the engine's words. */
internal data class WarningPlan(val message: String, val hint: String, val whenWords: String) {
  companion object {
    fun of(w: WarningShown) = WarningPlan(Typo.quotes(w.message), Typo.quotes(w.hint), Typo.quotes(w.whenWords))
  }
}

/**
 * The shower's decisions, with no Android in them. Main thread only.
 *
 * **A level.** [show] is given the engine's cover at every tick. The same
 * plan as the one drawn changes nothing (only the clock is refreshed); a
 * different plan of the same kind changes the window that is up; another
 * kind replaces it; [CoverPlan.None] takes it down at once.
 *
 * **Stepping aside.** A cover is an overlay above every app, so when the
 * child taps something on it that opens another screen (the emergency
 * dialler, the call, an app that stays open, the home screen, a way out),
 * the cover steps aside at once instead of hiding that screen until the
 * engine's next order says it may go. It follows the orders again as soon
 * as an order differs from the one it stepped aside from (the engine saw
 * the new screen), or after [STEP_ASIDE_MS] if none does. While LiMo's own
 * screen of the ways out is up ([limoScreen]) nothing is drawn: the engine
 * never covers LiMo, and this makes sure no frame of the cover sits over it.
 * When that screen closes, the orders lead again: the lock is back as soon
 * as the engine sees the app under it. Nothing here ever draws against an
 * order of nothing, so a ringing call, a dialler or LiMo is never covered
 * by a cover kept from before.
 *
 * **The warning** floats for [Look.WARNING_SHOWN_MS] (ten seconds) and goes
 * by itself, or at once at OK or a swipe up or to either side
 * ([dismissWarning]). A cover that comes up takes it away.
 */
internal class ShowerCore(
  private val cover: CoverSurface,
  private val card: CardSurface,
  private val timer: ShowTimer,
) {
  /** What the engine orders now. */
  var ordered: CoverPlan = CoverPlan.None
    private set

  /** What is drawn now. */
  var drawn: CoverPlan = CoverPlan.None
    private set

  private var limoUp = false
  private var aside: Aside? = null
  private var asideTimer: AutoCloseable? = null
  private var cardTimer: AutoCloseable? = null

  private class Aside(val from: CoverPlan, val sinceMs: Long)

  /** True while the cover stands aside for a screen the child opened from it. */
  val steppingAside: Boolean get() = aside != null

  fun show(plan: CoverPlan) {
    ordered = plan
    val a = aside
    if (a != null && (plan != a.from || timer.nowMs() - a.sinceMs >= STEP_ASIDE_MS)) endAside()
    apply()
  }

  /** LiMo's own screen of the ways out is up, or gone; gone, the orders lead again. */
  fun limoScreen(up: Boolean) {
    limoUp = up
    if (!up) endAside()
    apply()
  }

  /** The child tapped something on the cover that opens another screen: stand aside now. */
  fun stepAside() {
    if (ordered is CoverPlan.None) return
    aside = Aside(ordered, timer.nowMs())
    asideTimer?.close()
    asideTimer = timer.after(STEP_ASIDE_MS) {
      asideTimer = null
      if (aside != null) {
        endAside()
        apply()
      }
    }
    apply()
  }

  fun warn(warning: WarningPlan) {
    if (drawn != CoverPlan.None) return
    cardTimer?.close()
    cardTimer = null
    if (card.float(warning)) {
      cardTimer = timer.after(Look.WARNING_SHOWN_MS) {
        cardTimer = null
        card.remove()
      }
    }
  }

  /** OK on the warning, or a swipe that pushed it away: gone at once, its timer with it. */
  fun dismissWarning() {
    cardTimer?.close()
    cardTimer = null
    card.remove()
  }

  private fun endAside() {
    aside = null
    asideTimer?.close()
    asideTimer = null
  }

  private fun apply() {
    val want = if (limoUp || aside != null) CoverPlan.None else ordered
    if (want == CoverPlan.None) {
      if (drawn != CoverPlan.None || cover.up) cover.remove()
      drawn = CoverPlan.None
      return
    }
    if (want == drawn && cover.up) {
      cover.refresh()
      return
    }
    drawn = if (cover.draw(want)) want else CoverPlan.None
    if (drawn != CoverPlan.None && card.up) dismissWarning()
  }

  companion object {
    /** The longest the cover stands aside with no new order from the engine: the watch confirms a new screen within a second. */
    const val STEP_ASIDE_MS = 3_000L
  }
}
