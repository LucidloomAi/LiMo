package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.ffi.PhoneFacts
import ai.lucidloom.limo.child.watch.LaunchableApp
import ai.lucidloom.limo.child.watch.LimoPermissions
import ai.lucidloom.limo.child.watch.PhoneWatch
import ai.lucidloom.limo.child.watch.WatchListener
import ai.lucidloom.limo.child.watch.WatchLook
import android.app.Activity
import android.os.Looper
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/**
 * [OwnScreen] on Android's main thread (Robolectric's): a word given on the
 * main thread reaches the watch before the hook returns, so it keeps its
 * place among the words of LiMo's other screens; one handed over from
 * another thread waits for the main thread; and the activity's window says
 * when it enters or leaves split screen without a pause.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class OwnScreenThreadsTest {
  /** Only what the main activity tells the watch, in order. */
  private class Told : PhoneWatch {
    val words = mutableListOf<Boolean>()

    override fun limoScreenShown(shown: Boolean) {
      words += shown
    }

    override fun factsNow(inReach: Boolean, prodded: Boolean): PhoneFacts = error("not asked")

    override fun look(inReach: Boolean, prodded: Boolean): WatchLook = error("not asked")

    override fun listen(listener: WatchListener): AutoCloseable = AutoCloseable {}

    override fun launchableApps(): List<LaunchableApp> = emptyList()

    override fun permissions(): LimoPermissions = error("not asked")

    override fun emergencyDiallerOpening() = Unit
  }

  private fun activity(): Activity = Robolectric.buildActivity(Activity::class.java).setup().get()

  @Test
  fun onTheMainThreadTheWatchIsToldBeforeTheHookReturns() {
    val watch = Told()
    val own = OwnScreen(watch, SeamLog {})
    own.entersForeground(activity())
    assertEquals(listOf(true), watch.words)
    own.entersBackground()
    // Told already: nothing waits on the main thread's queue for it.
    assertEquals(listOf(true, false), watch.words)
    shadowOf(Looper.getMainLooper()).idle()
    assertEquals(listOf(true, false), watch.words)
  }

  @Test
  fun aWordFromAnotherThreadWaitsForTheMainThreadAndANewerOneWins() {
    val watch = Told()
    val own = OwnScreen(watch, SeamLog {})
    val a = activity()
    // The first resume of a cold start, handed over on the thread that first reached Expo's modules.
    Thread { own.entersForeground(a) }.apply { start() }.join()
    assertEquals(emptyList<Boolean>(), watch.words)
    shadowOf(Looper.getMainLooper()).idle()
    assertEquals(listOf(true), watch.words)

    // Another process: the activity paused in order before the late resume reached the main thread.
    val later = Told()
    val own2 = OwnScreen(later, SeamLog {})
    Thread { own2.entersForeground(a) }.apply { start() }.join()
    own2.entersBackground()
    shadowOf(Looper.getMainLooper()).idle()
    assertEquals(listOf(false), later.words)
  }

  @Test
  fun inSplitScreenFalseAndTheWindowSaysWhenThatChangesWithoutAPause() {
    val watch = Told()
    val own = OwnScreen(watch, SeamLog {})
    val a = activity()
    shadowOf(a).setInMultiWindowMode(true)
    own.entersForeground(a)
    assertEquals(listOf(false), watch.words)
    // Out of split screen with no pause: the window is laid out again.
    shadowOf(a).setInMultiWindowMode(false)
    val decor = a.window.decorView
    decor.layout(0, 0, decor.width + 10, decor.height + 10)
    assertEquals(listOf(false, true), watch.words)
    // Into it again, with no pause.
    shadowOf(a).setInMultiWindowMode(true)
    decor.layout(0, 0, decor.width - 10, decor.height - 10)
    assertEquals(listOf(false, true, false), watch.words)
    // Left: false, and a layout after it says nothing.
    own.entersBackground()
    shadowOf(a).setInMultiWindowMode(false)
    decor.layout(0, 0, decor.width + 5, decor.height + 5)
    assertEquals(listOf(false, true, false, false), watch.words)
  }
}
