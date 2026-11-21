package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.Cover
import ai.lucidloom.limo.child.ffi.PhoneEngineInterface
import ai.lucidloom.limo.child.ffi.PhoneFacts
import ai.lucidloom.limo.child.ffi.PinAction
import ai.lucidloom.limo.child.ffi.TimeRequest
import ai.lucidloom.limo.child.ffi.TryAnswer
import ai.lucidloom.limo.child.ffi.WarningShown
import ai.lucidloom.limo.child.keep.AdminReceiver
import ai.lucidloom.limo.child.keep.AlarmReceiver
import ai.lucidloom.limo.child.keep.Keeper
import ai.lucidloom.limo.child.link.LinkHost
import ai.lucidloom.limo.child.link.LinkHostSide
import ai.lucidloom.limo.child.link.OfferAnswer
import ai.lucidloom.limo.child.watch.PhoneWatch
import ai.lucidloom.limo.child.watch.Watch
import android.app.AlarmManager
import android.app.PendingIntent
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
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
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * The engine's host of the process: [PhoneHost] on Android. One thread of
 * its own ("LiMo engine") runs [HostCore], the only code that calls the
 * engine; everything else here carries calls onto that thread and answers
 * back to the main thread.
 *
 * It needs nothing to be running to work: the keeper's foreground service
 * (keep/) only keeps the process alive and shows the time left. While the
 * process lives and the phone is paired, the host ticks.
 *
 * It owns the link to the family's PC (link/, its own thread "LiMo link"):
 * it starts it with itself, takes from it whether the master is in reach,
 * hands it the engine's reports, and takes the master's grants, a contact
 * lost, a pairing and a release from it on its own thread.
 */
class Host private constructor(context: Context) : PhoneHost {
  private val app: Context = context.applicationContext ?: context
  private val thread = HandlerThread("LiMo engine").apply { start() }
  private val engineThread = Handler(thread.looper)
  private val main = Handler(Looper.getMainLooper())
  private val listeners = CopyOnWriteArrayList<HostListener>()
  private val store = AtomicStateStore(stateFile(app))
  private val names = AtomicNameStore(nameFile(app))
  private val link = LinkHost(app, LinkSide())

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
    names = names,
    facts = WatchFacts(Watch.of(app)),
    engines = RealEngines,
    clock = AndroidClock,
    timer = timer,
    wakeups = AlarmWakeups(app),
    keeper = Keeper.display(app),
    log = AndroidHostLog,
    events = HostEvents { what -> main.post { deliver(what) } },
    link = link.core,
  )

  override fun start() {
    onEngine("start") { core.start() }
    link.start()
  }

  /** True while the cable door's last offer is still being paired with. */
  internal val cableBusy: Boolean get() = link.core.offerHeld

  /** An offer the cable door read ([ai.lucidloom.limo.child.link.CableDoor]); null when it was too long. Wiped. */
  internal fun cableOffer(text: ByteArray?): OfferAnswer {
    link.start()
    return link.core.takeOffer(text)
  }

  override val paired: Boolean get() = store.exists()

  override val childName: String? get() = if (paired) names.read() else null

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

  override fun tryGuardPin(pin: ByteArray, answer: (WayOut<TryAnswer>) -> Unit) {
    val posted = onEngine("guard_pin", fallback = { pin.fill(0) }) {
      core.tryGuardPin(pin).also { a -> answerOnMain { answer(a) } }
    }
    if (!posted) answerOnMain { answer(WayOut.Failed) }
  }

  override fun finishSetup(answer: (WayOut<Boolean>) -> Unit) {
    val posted = onEngine("setup") { core.finishSetup().also { a -> answerOnMain { answer(a) } } }
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

  override fun pair(welcome: ByteArray, codeSecret: ByteArray, childName: String?, answer: (Boolean) -> Unit) {
    val posted = onEngine("pair", fallback = { welcome.fill(0); codeSecret.fill(0) }) {
      core.pair(welcome, codeSecret, childName).also { ok -> answerOnMain { answer(ok) } }
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

  /** The host's side, as the link sees it: every call carried onto the engine's thread. */
  private inner class LinkSide : LinkHostSide {
    override val paired: Boolean get() = store.exists()

    override fun adopt(engine: PhoneEngineInterface, childName: String?): Boolean {
      val done = CountDownLatch(1)
      var ok = false
      val posted = engineThread.post {
        try {
          ok = core.adopt(engine, childName)
        } catch (e: Exception) {
          AndroidHostLog.line("host failed what=adopt error=${e.javaClass.simpleName}")
        } finally {
          done.countDown()
        }
      }
      if (!posted) {
        (engine as? AutoCloseable)?.let { guarded("engine_close") { it.close() } }
        return false
      }
      if (!done.await(ADOPT_WAIT_SECS, TimeUnit.SECONDS)) return false
      // Paired: the keeper's service and the covers, as after any pairing.
      if (ok) Keeper.start(app, "link_pair")
      return ok
    }

    override fun grant(grant: ByteArray) {
      onEngine("grant") { core.takeGrant(grant) }
    }

    override fun lostContact() {
      onEngine("lost_contact") { core.lostContact() }
    }

    override fun reachChanged() = timer.tickAfter(0)

    override fun released() {
      onEngine("release") { core.release() }
      protectionOff()
    }
  }

  /**
   * The family's PC removed this phone: LiMo's device admin is switched off
   * too, so LiMo can be removed from the phone, as the family decided. An
   * app may always switch its own admin off.
   */
  private fun protectionOff() {
    guarded("protection_off") {
      val policy = app.getSystemService(DevicePolicyManager::class.java) ?: return@guarded
      val admin = ComponentName(app, AdminReceiver::class.java)
      if (policy.isAdminActive(admin)) policy.removeActiveAdmin(admin)
      AndroidHostLog.line("host protection_off")
    }
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

    /** How long the link waits for a pairing to be kept on the engine's thread. */
    private const val ADOPT_WAIT_SECS = 30L

    /** The engine's kept state: the app's private file `files/limo/state`. */
    internal fun stateFile(context: Context): File = File(context.filesDir, "limo/state")

    /** The child's name, beside it: `files/limo/child.json`. */
    internal fun nameFile(context: Context): File = File(context.filesDir, "limo/child.json")
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

/**
 * The child's name in one small file ([ChildName]), written all or nothing
 * with Android's AtomicFile. One that cannot be read is no name.
 */
internal class AtomicNameStore(private val file: File) : NameStore {
  private val atomic = AtomicFile(file)

  override fun read(): String? {
    if (!file.exists() && !File(file.path + ".bak").exists()) return null
    return try {
      ChildName.parse(String(atomic.readFully(), Charsets.UTF_8))
    } catch (_: IOException) {
      null
    }
  }

  override fun write(name: String) {
    file.parentFile?.mkdirs()
    val out = atomic.startWrite()
    try {
      out.write(ChildName.text(name).toByteArray(Charsets.UTF_8))
      atomic.finishWrite(out)
    } catch (e: IOException) {
      atomic.failWrite(out)
      throw e
    }
  }

  override fun delete() = atomic.delete()
}

/** The watch's facts. Whether the master is in reach, and asked for a report, the host takes from the link. */
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
