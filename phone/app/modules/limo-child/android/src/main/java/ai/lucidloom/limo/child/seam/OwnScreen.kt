package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.watch.PhoneWatch
import android.app.Activity
import android.os.Handler
import android.os.Looper
import android.view.View
import java.lang.ref.WeakReference

/**
 * Whether LiMo's main activity (setup, the child's introduction, Today, How
 * it works) is LiMo's own screen in front, for the watch
 * (`PhoneWatch.limoScreenShown`). The watch takes LiMo's word for it, a
 * fact that holds with usage access off too, when the front would
 * otherwise be unknown, counted and maybe covered. The ways out screen
 * (`show/WaysOutActivity`) says the same for itself.
 *
 * The rules, all here:
 *
 * - **In front: true; leaving it: false.** From the activity's resume and
 *   pause, as Expo hands them to `LimoChildModule`.
 * - **At once, on the main thread, in order.** A word is given on the
 *   thread that hands it, inside the activity's own resume or pause, never
 *   later through another thread. The watch holds one word for all of
 *   LiMo's screens, the last given: when the ways out screen opens over
 *   Today, Today's pause says false and then the ways out screen's resume
 *   says true, and that order must hold.
 * - **Never in split screen** (`Activity.isInMultiWindowMode`, which also
 *   covers a floating window): there another app shares the screen, and
 *   "LiMo is in front" would let it run uncounted and uncovered with usage
 *   access off. False is the safe word: the front is then unknown. The
 *   activity handles its own size changes, so it may enter or leave split
 *   screen without a pause: its window being laid out again makes it look.
 * - **The first resume of a cold start** may be handed over late, on
 *   another thread: Expo holds lifecycle events until JavaScript first
 *   reaches its modules, then gives them on that thread (README.md, "LiMo's
 *   own screen"). Such a word is taken on the main thread, and only if no
 *   word was given in order since, which is always newer.
 *
 * Pure: the clock of words is the caller's; [OwnScreen] feeds it on Android.
 * [tell] is guarded: nothing here throws into the activity.
 */
internal class MainScreen(
  private val tell: (Boolean) -> Unit,
  private val log: SeamLog,
) {
  private var resumed = false
  private var split = false
  private var inOrder = false

  /** The activity's resume (true) or pause (false), in order, on the main thread. [split] is the window now. */
  fun inOrder(foreground: Boolean, split: Boolean) {
    inOrder = true
    take(foreground, split)
  }

  /**
   * A resume or pause Expo held back at the start and handed over on
   * another thread, now on the main thread: taken only if no word was given
   * in order since.
   */
  fun handedOver(foreground: Boolean, split: Boolean) {
    if (!inOrder) take(foreground, split)
  }

  /** The window was laid out again: while resumed, a change into or out of split screen is said. */
  fun laidOut(split: Boolean) {
    if (!resumed || split == this.split) return
    this.split = split
    say(!split)
  }

  private fun take(foreground: Boolean, split: Boolean) {
    resumed = foreground
    this.split = split
    say(foreground && !split)
  }

  private fun say(shown: Boolean) {
    try {
      tell(shown)
    } catch (e: Exception) {
      log.line("seam failed what=own_screen error=${e.javaClass.simpleName}")
    }
  }
}

/**
 * [MainScreen] on Android: words from `LimoChildModule`'s two hooks, told to
 * the watch at once on the main thread, and the activity's window watched
 * for split screen.
 */
internal class OwnScreen(private val watch: PhoneWatch, private val log: SeamLog) {
  private val main = Handler(Looper.getMainLooper())
  private val core = MainScreen(tell = { watch.limoScreenShown(it) }, log = log)
  private var watched: WeakReference<Activity>? = null
  private var watchedDecor: WeakReference<View>? = null

  // Laid out again: into or out of split screen, without a pause. Always on the main thread.
  private val layout = View.OnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
    guarded { watched?.get()?.let { core.laidOut(splitOf(it)) } }
  }

  /** The activity came to the front. Any thread; on the main thread the watch is told before this returns. */
  fun entersForeground(activity: Activity?) = word(true, activity)

  /** The activity left the front. Any thread; on the main thread the watch is told before this returns. */
  fun entersBackground() = word(false, null)

  private fun word(foreground: Boolean, activity: Activity?) {
    if (Looper.myLooper() == Looper.getMainLooper()) {
      guarded {
        core.inOrder(foreground, foreground && splitOf(activity))
        if (foreground) watchWindow(activity)
      }
    } else {
      main.post {
        guarded {
          core.handedOver(foreground, foreground && splitOf(activity))
          if (foreground) watchWindow(activity)
        }
      }
    }
  }

  /** Watches [activity]'s window for being laid out again, once for each activity. */
  private fun watchWindow(activity: Activity?) {
    val decor = activity?.window?.decorView ?: return
    if (watchedDecor?.get() === decor) return
    watchedDecor?.get()?.removeOnLayoutChangeListener(layout)
    decor.addOnLayoutChangeListener(layout)
    watchedDecor = WeakReference(decor)
    watched = WeakReference(activity)
  }

  /** In split screen, or a floating window; when that cannot be read, so as to be safe. */
  private fun splitOf(activity: Activity?): Boolean = try {
    activity?.isInMultiWindowMode ?: false
  } catch (_: Exception) {
    true
  }

  private inline fun guarded(block: () -> Unit) {
    try {
      block()
    } catch (e: Exception) {
      log.line("seam failed what=own_screen error=${e.javaClass.simpleName}")
    }
  }
}
