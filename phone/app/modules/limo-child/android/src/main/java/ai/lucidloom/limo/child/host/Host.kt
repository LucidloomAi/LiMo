package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.Cover
import ai.lucidloom.limo.child.ffi.PhoneFacts
import ai.lucidloom.limo.child.ffi.PinAction
import ai.lucidloom.limo.child.ffi.TimeRequest
import ai.lucidloom.limo.child.ffi.TryAnswer
import ai.lucidloom.limo.child.ffi.WarningShown
import ai.lucidloom.limo.child.keep.AlarmReceiver
import ai.lucidloom.limo.child.keep.Keeper
import ai.lucidloom.limo.child.watch.PhoneWatch
import ai.lucidloom.limo.child.watch.Watch
import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.SystemClock
import android.util.AtomicFile
import android.util.Log
import java.io.File
import java.io.IOException
import java.util.concurrent.CopyOnWriteArrayList

/**
 * The engine's host of the process: [PhoneHost] on Android. One thread of
 * its own ("LiMo engine") runs [HostCore], the only code that calls the
 * engine; everything else here carries calls onto that thread and answers
 * back to the main thread.
 *
 * It needs nothing to be running to work: the keeper's foreground service
 * (keep/) only keeps the process alive and shows the time left. While the
 * process lives and the phone is paired, the host ticks.
 */
class Host private constructor(context: Context) : PhoneHost {
  private val app: Context = context.applicationContext ?: context
  private val thread = HandlerThread("LiMo engine").apply { start() }
  private val engineThread = Handler(thread.looper)
  private val main = Handler(Looper.getMainLooper())
  private val listeners = CopyOnWriteArrayList<HostListener>()
  private val store = AtomicStateStore(stateFile(app))

  // Already on the engine's thread.
  private val tick: Runnable = Runnable { guarded("tick") { core.tick() } }

  private val timer: HostTimer = object : HostTimer {
    override fun tickAfter(ms: Long) {
      engineThread.removeCallbacks(tick)
      engineThread.postDelayed(tick, ms.coerceAtLeast(0L))
    }

    override fun cancel() = engineThread.removeCallbacks(tick)
  }

  private val core: HostCore = HostCore(
    store = store,
    facts = WatchFacts(Watch.of(app)),
    engines = RealEngines,
    clock = AndroidClock,
    timer = timer,
    wakeups = AlarmWakeups(app),
    keeper = Keeper.display(app),
    log = AndroidHostLog,
    events = HostEvents { what -> main.post { deliver(what) } },
  )

  override fun start() {
    onEngine("start") { core.start() }
  }

  override val paired: Boolean get() = store.exists()

  override fun useShower(shower: Shower) {
    onEngine("shower") {
      core.shower = MainShower(main, shower)
      timer.tickAfter(0)
    }
  }

  override fun wake(done: (() -> Unit)?) {
    onEngine("wake", done) { core.tick() }
  }

  override fun askForTime(minutes: Int, reason: String, answer: (WayOut<TimeRequest>) -> Unit) {
    val posted = onEngine("ask") { core.askForTime(minutes, reason).also { a -> answerOnMain { answer(a) } } }
    if (!posted) answerOnMain { answer(WayOut.Failed) }
  }

  override fun tryCode(code: ByteArray, answer: (WayOut<TryAnswer>) -> Unit) {
    val posted = onEngine("code", fallback = { code.fill(0) }) {
      core.tryCode(code).also { a -> answerOnMain { answer(a) } }
    }
    if (!posted) answerOnMain { answer(WayOut.Failed) }
  }

  override fun tryPin(pin: ByteArray, action: PinAction, answer: (WayOut<TryAnswer>) -> Unit) {
    val posted = onEngine("pin", fallback = { pin.fill(0) }) {
      core.tryPin(pin, action).also { a -> answerOnMain { answer(a) } }
    }
    if (!posted) answerOnMain { answer(WayOut.Failed) }
  }

  override fun view(answer: (HostView) -> Unit) {
    val posted = onEngine("view") { core.view().also { v -> answerOnMain { answer(v) } } }
    if (!posted) answerOnMain { answer(HostView(paired = paired, json = null, problem = null)) }
  }

  override fun listen(listener: HostListener): AutoCloseable {
    listeners.add(listener)
    return AutoCloseable { listeners.remove(listener) }
  }

  override fun pair(welcome: ByteArray, codeSecret: ByteArray, answer: (Boolean) -> Unit) {
    val posted = onEngine("pair", fallback = { welcome.fill(0); codeSecret.fill(0) }) {
      core.pair(welcome, codeSecret).also { ok -> answerOnMain { answer(ok) } }
    }
    if (!posted) answerOnMain { answer(false) }
  }

  override fun forget(answer: () -> Unit) {
    val posted = onEngine("forget") {
      core.forget()
      answerOnMain(answer)
    }
    if (!posted) answerOnMain(answer)
  }

  /**
   * Runs [block] on the engine's thread, guarded: a failure is logged and
   * never reaches Android. [done] runs after it, whatever happens. False
   * when it could not be posted at all (then [fallback] runs at once).
   */
  private fun onEngine(
    what: String,
    done: (() -> Unit)? = null,
    fallback: (() -> Unit)? = null,
    block: () -> Unit,
  ): Boolean {
    val posted = engineThread.post {
      try {
        block()
      } catch (e: Exception) {
        AndroidHostLog.line("host failed what=$what error=${e.javaClass.simpleName}")
      } catch (e: LinkageError) {
        AndroidHostLog.line("host failed what=$what error=${e.javaClass.simpleName}")
      } finally {
        done?.let { guarded(what) { it() } }
      }
    }
    if (!posted) {
      fallback?.let { guarded(what) { it() } }
      done?.let { guarded(what) { it() } }
    }
    return posted
  }

  private fun answerOnMain(block: () -> Unit) {
    main.post { guarded("answer") { block() } }
  }

  private fun deliver(what: HostChange) {
    for (listener in listeners) guarded("listener") { listener.changed(what) }
  }

  private inline fun guarded(what: String, block: () -> Unit) {
    try {
      block()
    } catch (e: Exception) {
      AndroidHostLog.line("host failed what=$what error=${e.javaClass.simpleName}")
    } catch (e: LinkageError) {
      AndroidHostLog.line("host failed what=$what error=${e.javaClass.simpleName}")
    }
  }

  companion object {
    @Volatile
    private var instance: Host? = null

    /** The host of this process, made on first use (it does not start it: [PhoneHost.start]). Any thread. */
    @JvmStatic
    fun of(context: Context): PhoneHost = get(context)

    internal fun get(context: Context): Host =
      instance ?: synchronized(this) { instance ?: Host(context).also { instance = it } }

    /** The engine's kept state: the app's private file `files/limo/state`. */
    internal fun stateFile(context: Context): File = File(context.filesDir, "limo/state")
  }
}

/** The kept state in one file, written all or nothing with Android's AtomicFile. */
internal class AtomicStateStore(private val file: File) : StateStore {
  private val atomic = AtomicFile(file)

  // AtomicFile keeps a ".bak" beside the file while a write is cut short on
  // older Android; reading puts it back.
  override fun exists(): Boolean = file.exists() || File(file.path + ".bak").exists()

  override fun read(): ByteArray? {
    if (!exists()) return null
    return atomic.readFully()
  }

  override fun write(bytes: ByteArray) {
    file.parentFile?.mkdirs()
    val out = atomic.startWrite()
    try {
      out.write(bytes)
      atomic.finishWrite(out)
    } catch (e: IOException) {
      atomic.failWrite(out)
      throw e
    }
  }

  override fun delete() = atomic.delete()
}

/** The watch's facts. There is no link yet, so the master is never in reach and never asks for a report. */
internal class WatchFacts(private val watch: PhoneWatch) : FactsSource {
  override fun factsNow(): PhoneFacts = watch.factsNow(inReach = false, prodded = false)

  override fun listen(onChange: () -> Unit): AutoCloseable = watch.listen { onChange() }
}

internal object AndroidClock : HostClock {
  override fun elapsedMs(): Long = SystemClock.elapsedRealtime()

  override fun wallSecs(): Long = System.currentTimeMillis().floorDiv(1_000L)
}

/**
 * The host's alarm: an ordinary one, `setAndAllowWhileIdle` on the elapsed
 * clock, which needs no permission and is let through Android's deep sleep
 * now and then. Never an exact alarm. It goes to [AlarmReceiver], from a
 * pending intent only LiMo makes.
 */
internal class AlarmWakeups(private val context: Context) : Wakeups {
  private fun pending(): PendingIntent = PendingIntent.getBroadcast(
    context,
    0,
    Intent(context, AlarmReceiver::class.java).setAction(AlarmReceiver.ACTION_WAKE),
    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
  )

  override fun at(elapsedMs: Long) {
    val alarms = context.getSystemService(AlarmManager::class.java) ?: return
    alarms.setAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, elapsedMs, pending())
  }

  override fun cancel() {
    context.getSystemService(AlarmManager::class.java)?.cancel(pending())
  }
}

/** Hands each cover and warning to show/'s [Shower] on the main thread, guarded. */
internal class MainShower(private val main: Handler, private val shower: Shower) : Shower {
  override fun show(cover: Cover) {
    main.post { guardedShow { shower.show(cover) } }
  }

  override fun warn(warning: WarningShown) {
    main.post { guardedShow { shower.warn(warning) } }
  }

  private inline fun guardedShow(block: () -> Unit) {
    try {
      block()
    } catch (e: Exception) {
      AndroidHostLog.line("host failed what=shower error=${e.javaClass.simpleName}")
    }
  }
}

/** The host's log: Android's log, tag `LiMo`. */
internal object AndroidHostLog : HostLog {
  private const val TAG = "LiMo"

  override fun line(text: String) {
    Log.i(TAG, text)
  }
}
