package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.Cover
import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.EngineException
import ai.lucidloom.limo.child.ffi.InternalException
import ai.lucidloom.limo.child.ffi.PhoneEngine
import ai.lucidloom.limo.child.ffi.PhoneEngineInterface
import ai.lucidloom.limo.child.ffi.PhoneFacts
import ai.lucidloom.limo.child.ffi.PhoneOrders
import ai.lucidloom.limo.child.ffi.PhonePermissions
import ai.lucidloom.limo.child.ffi.PinAction
import ai.lucidloom.limo.child.ffi.TimeLeft
import ai.lucidloom.limo.child.ffi.TimeRequest
import ai.lucidloom.limo.child.ffi.TryAnswer
import ai.lucidloom.limo.child.ffi.WarningShown

// What the host needs from Android, each behind a small interface, so that
// the JVM tests play the phone and the Rust engine is the real one.

/** The engine's kept state: the app's private file `files/limo/state`. */
internal interface StateStore {
  /** A state is kept (the file, or Android's backup of it while a write was cut short, is there). */
  fun exists(): Boolean

  /** The kept bytes; null when no state is kept. Throws when one is kept and cannot be read. */
  fun read(): ByteArray?

  /** Keeps [bytes] in place of what was kept, all or nothing. Throws when it cannot. */
  fun write(bytes: ByteArray)

  /** Deletes the kept state. */
  fun delete()
}

/** The facts for each tick, and word of a change (the watch). */
internal interface FactsSource {
  /** The facts now. There is no link yet: the master is never in reach, nor has it asked for a report. */
  fun factsNow(): PhoneFacts

  /** Calls [onChange] (on any thread) whenever a fact changed. */
  fun listen(onChange: () -> Unit): AutoCloseable
}

/** Makes engines: the real ones in the app, the same ones watched in the tests. */
internal interface Engines {
  fun load(state: ByteArray): PhoneEngineInterface

  fun paired(welcome: ByteArray, codeSecret: ByteArray, wallSecs: Long): PhoneEngineInterface
}

/** The phone's two clocks. */
internal interface HostClock {
  /** `SystemClock.elapsedRealtime()`. */
  fun elapsedMs(): Long

  /** The wall clock, unix seconds. */
  fun wallSecs(): Long
}

/** The host's next tick, on its own thread. */
internal interface HostTimer {
  /** The next tick in [ms], in place of any tick already waiting. Any thread. */
  fun tickAfter(ms: Long)

  /** No tick waits any more. */
  fun cancel()
}

/** An ordinary alarm (not an exact one) that wakes the host while the screen is off, and brings LiMo back if it was stopped. */
internal interface Wakeups {
  /** Wakes the host at about [elapsedMs] (`SystemClock.elapsedRealtime()`), in place of any alarm set before. */
  fun at(elapsedMs: Long)

  fun cancel()
}

/** The keeper: LiMo's notifications of the time left and of the warnings (keep/). */
internal interface KeeperDisplay {
  /** At every tick: the time left, in the engine's words. */
  fun timeLeft(timeLeft: TimeLeft)

  /** In the one tick a warning falls due. */
  fun warning(warning: WarningShown)

  /** The phone is not paired any more: nothing to show, and nothing to keep running. */
  fun cleared()
}

/** Where the host's log lines go (tag `LiMo`): a package name at most, never a label, a name or anything typed. */
internal fun interface HostLog {
  fun line(text: String)
}

/** Told when the pairing or the view may have changed. */
internal fun interface HostEvents {
  fun changed(what: HostChange)
}

/** The engines of the app: the Rust engine itself. */
internal object RealEngines : Engines {
  override fun load(state: ByteArray): PhoneEngineInterface = PhoneEngine.load(state)

  override fun paired(welcome: ByteArray, codeSecret: ByteArray, wallSecs: Long): PhoneEngineInterface =
    PhoneEngine.paired(welcome, codeSecret, wallSecs)
}

/** Nothing over the screen. */
internal val NO_COVER = Cover(CoverKind.NOTHING, null, null)

/**
 * The engine's host with no Android in it: the one owner of the one engine,
 * and everything the engine's README asks of a host. Not thread safe: [Host]
 * calls it on its one thread only.
 *
 * - **At start** it loads the kept state. With none, the phone is not
 *   paired and the host only waits for a pairing: no tick, no alarm, no
 *   watching. A state that cannot be read gives the engine's own answer,
 *   an engine that locks the phone and never asks to be kept, so the host
 *   never writes over it.
 * - **Each tick** takes the facts, calls the engine, keeps the state when
 *   the orders say so and before anything else, hands the cover to the
 *   [Shower] and the time left and any warning to the [KeeperDisplay], logs
 *   the engine's notes, and asks for the next tick: in a second while the
 *   screen is on, at the engine's next check while it is off (see
 *   [HostTimes]).
 * - **A failure** is logged and the next tick tries again. An engine that
 *   reports itself broken (or panicked) is closed and made again from the
 *   kept state.
 */
internal class HostCore(
  private val store: StateStore,
  private val names: NameStore,
  private val facts: FactsSource,
  private val engines: Engines,
  private val clock: HostClock,
  private val timer: HostTimer,
  private val wakeups: Wakeups,
  private val keeper: KeeperDisplay,
  private val log: HostLog,
  private val events: HostEvents,
) {
  /** Who draws the covers. Until show/ gives one, a stand in that logs them. */
  var shower: Shower = LoggingShower(log)

  private var engine: PhoneEngineInterface? = null
  private var watching: AutoCloseable? = null
  private var started = false
  private var interactive = true
  private var alarmAt: Long? = null
  private var reportOrdered = false
  private val failedAt = HashMap<String, Long>()

  // What the screens were last told about, to say when the view may have changed.
  private var lastWords: String? = null
  private var lastCover: CoverKind? = null
  private var lastPermissions: PhonePermissions? = null

  /** A state is kept: the phone is paired. */
  val paired: Boolean get() = engine != null || safely("state_exists", false) { store.exists() }

  /** The child's name the pairing gave, cleaned; null when not paired or when none is kept. */
  val childName: String? get() = if (paired) safely("name_read", null) { names.read() } else null

  /** Loads the kept state and ticks, or waits for a pairing. Once. */
  fun start() {
    if (started) return
    started = true
    if (ensureEngine() == null) {
      if (!paired) log.line("host not_paired waiting")
      return
    }
    tick()
  }

  /** One call into the engine, with the facts now, its orders carried out. */
  fun tick() {
    val engine = ensureEngine() ?: return
    val seen = try {
      facts.factsNow()
    } catch (e: Exception) {
      return failedTick("facts", e)
    } catch (e: LinkageError) {
      return failedTick("facts", e)
    }
    interactive = seen.interactive
    val orders = try {
      engine.tick(seen)
    } catch (e: EngineException.Broken) {
      return broken(e)
    } catch (e: InternalException) {
      return broken(e)
    } catch (e: Exception) {
      return failedTick("tick", e)
    } catch (e: LinkageError) {
      return failedTick("tick", e)
    }
    // Before anything else.
    if (orders.saveState) save(engine)
    carryOut(orders)
    schedule(orders.nextCheckInSecs)
    if (viewMayHaveChanged(orders, seen.permissions)) events.changed(HostChange.VIEW)
  }

  /** The child asks for more time. */
  fun askForTime(minutes: Int, reason: String): WayOut<TimeRequest> =
    wayOut<TimeRequest>("ask", { "done" }) { it.askForTime(minutes, reason, clock.wallSecs()) }

  /** A one time code; [code] is filled with zeros, whatever the answer. */
  fun tryCode(code: ByteArray): WayOut<TryAnswer> = try {
    wayOut<TryAnswer>("code", { it.kind.name.lowercase() }) { it.tryCode(code, clock.wallSecs()) }
  } finally {
    code.fill(0)
  }

  /** A guardian's PIN; [pin] is filled with zeros, whatever the answer. */
  fun tryPin(pin: ByteArray, action: PinAction): WayOut<TryAnswer> = try {
    wayOut<TryAnswer>("pin", { it.kind.name.lowercase() }) { it.tryPin(pin, action, clock.wallSecs()) }
  } finally {
    pin.fill(0)
  }

  /** The engine's view, for the child's own screens. */
  fun view(): HostView {
    val engine = ensureEngine() ?: return HostView(paired = false, json = null, problem = null)
    return try {
      HostView(paired = true, json = engine.view()?.json, problem = engine.loadProblem()?.kind)
    } catch (e: EngineException.Broken) {
      broken(e)
      HostView(paired = true, json = null, problem = null)
    } catch (e: InternalException) {
      broken(e)
      HostView(paired = true, json = null, problem = null)
    } catch (e: Exception) {
      failed("view", e)
      HostView(paired = true, json = null, problem = null)
    } catch (e: LinkageError) {
      failed("view", e)
      HostView(paired = true, json = null, problem = null)
    }
  }

  /**
   * Pairs the phone from a welcome and its code secret, both filled with
   * zeros after, for the child called [childName] (cleaned by [ChildName];
   * null or nothing left is no name). The new state is kept before the
   * engine is used; if it cannot be kept, the phone stays as it was, its
   * name too. Replaces any state and name kept before, a state that cannot
   * be read among them. A name that cannot be kept leaves the phone paired
   * with no name.
   */
  fun pair(welcome: ByteArray, codeSecret: ByteArray, childName: String?): Boolean {
    started = true
    try {
      val made = engines.paired(welcome, codeSecret, clock.wallSecs())
      val kept = try {
        val bytes = made.save()
        try {
          store.write(bytes)
          true
        } finally {
          bytes.fill(0)
        }
      } catch (e: Exception) {
        failed("state_write", e)
        false
      }
      if (!kept) {
        // Not kept, so not used: a restart would lose it.
        close(made)
        return false
      }
      close(engine)
      engine = made
      // Never in a log line: only whether one is kept.
      val name = ChildName.clean(childName)
      safely("name_write", Unit) {
        try {
          if (name != null) names.write(name) else names.delete()
        } catch (e: Exception) {
          // Never an older pairing's name for this one.
          names.delete()
          throw e
        }
      }
      watch()
      log.line(if (name != null) "host paired" else "host paired no_child_name")
      events.changed(HostChange.PAIRING)
      tick()
      return true
    } catch (e: EngineException) {
      log.line("host pair_refused error=${e.javaClass.simpleName}")
      return false
    } catch (e: Exception) {
      failed("pair", e)
      return false
    } catch (e: LinkageError) {
      failed("pair", e)
      return false
    } finally {
      welcome.fill(0)
      codeSecret.fill(0)
    }
  }

  /** Forgets the pairing: the state is deleted, nothing is covered, nothing ticks. */
  fun forget() {
    close(engine)
    engine = null
    watching?.let { safely("watch_stop", Unit) { it.close() } }
    watching = null
    safely("state_delete", Unit) { store.delete() }
    safely("name_delete", Unit) { names.delete() }
    timer.cancel()
    safely("alarm_cancel", Unit) { wakeups.cancel() }
    alarmAt = null
    safely("shower", Unit) { shower.show(NO_COVER) }
    safely("keeper", Unit) { keeper.cleared() }
    lastWords = null
    lastCover = null
    lastPermissions = null
    log.line("host forgotten")
    events.changed(HostChange.PAIRING)
  }

  /** The engine, loaded from the kept state when there is none yet; null when not paired or it cannot be made. */
  private fun ensureEngine(): PhoneEngineInterface? {
    engine?.let { return it }
    val bytes = try {
      store.read()
    } catch (e: Exception) {
      // Kept and cannot be read: the engine is loaded from nothing, which
      // locks the phone and never asks to be kept, so the file stays as it is.
      failed("state_read", e)
      ByteArray(0)
    } ?: return null
    val made = try {
      engines.load(bytes)
    } catch (e: Exception) {
      failed("engine_load", e)
      null
    } catch (e: LinkageError) {
      failed("engine_load", e)
      null
    } finally {
      bytes.fill(0)
    }
    if (made == null) {
      timer.tickAfter(HostTimes.RETRY_MS)
      return null
    }
    engine = made
    safely("load_problem", null) { made.loadProblem() }?.let {
      log.line("host state_unreadable kind=${it.kind.name.lowercase()} phone_locked")
    }
    watch()
    return made
  }

  private fun watch() {
    if (watching != null) return
    watching = safely("watch_start", null) { facts.listen { timer.tickAfter(0) } }
  }

  private fun carryOut(orders: PhoneOrders) {
    safely("shower", Unit) { shower.show(orders.cover) }
    safely("keeper", Unit) { keeper.timeLeft(orders.timeLeft) }
    orders.warning?.let { warning ->
      safely("keeper", Unit) { keeper.warning(warning) }
      safely("shower", Unit) { shower.warn(warning) }
    }
    // The engine's notes hold no sentence and no name: a package at most.
    for (note in orders.notes) log.line("note $note")
    if (orders.sendReport && !reportOrdered) {
      // Only ever ordered in reach of the master, which is never, until the link.
      reportOrdered = true
      log.line("host report_ordered no_link")
    }
  }

  /** The engine says to keep the state: exactly then, in place of the old bytes. */
  private fun save(engine: PhoneEngineInterface) {
    val bytes = try {
      engine.save()
    } catch (e: Exception) {
      failed("save", e)
      return
    }
    try {
      // Empty: the kept state could not be read, and is never written over.
      if (bytes.isEmpty()) return
      store.write(bytes)
    } catch (e: Exception) {
      failed("state_write", e)
      safely("save_failed", Unit) { engine.saveFailed() }
    } finally {
      bytes.fill(0)
    }
  }

  private fun <T> wayOut(what: String, kind: (T) -> String, call: (PhoneEngineInterface) -> T): WayOut<T> {
    val engine = ensureEngine() ?: return WayOut.NotPaired
    val answer: WayOut<T> = try {
      WayOut.Done(call(engine))
    } catch (e: EngineException.Broken) {
      broken(e)
      WayOut.Failed
    } catch (e: InternalException) {
      broken(e)
      WayOut.Failed
    } catch (e: EngineException) {
      WayOut.Refused(e)
    } catch (e: Exception) {
      failed(what, e)
      WayOut.Failed
    } catch (e: LinkageError) {
      failed(what, e)
      WayOut.Failed
    }
    // Right or wrong, kept before the answer is shown.
    this.engine?.let { current -> if (safely("should_save", false) { current.shouldSave() }) save(current) }
    val said = when (answer) {
      is WayOut.Done -> kind(answer.value)
      is WayOut.Refused -> "refused error=${answer.refusal.javaClass.simpleName}"
      WayOut.NotPaired -> "not_paired"
      WayOut.Failed -> "failed"
    }
    // Never the reason, the code or the PIN: only what kind of answer it was.
    log.line("way_out $what answer=$said")
    events.changed(HostChange.VIEW)
    tick()
    return answer
  }

  /** The engine is broken (a panic inside it): closed, and made again from the kept state. */
  private fun broken(e: Exception) {
    log.line("host engine_broken error=${e.javaClass.simpleName} loading_again")
    close(engine)
    engine = null
    ensureEngine()
    timer.tickAfter(HostTimes.AFTER_BROKEN_MS)
  }

  private fun failedTick(what: String, e: Throwable) {
    failed(what, e)
    schedule(null)
  }

  /** The next tick and the alarm, by [HostTimes.plan]. */
  private fun schedule(nextCheckInSecs: Long?) {
    val now = clock.elapsedMs()
    val plan = HostTimes.plan(interactive, nextCheckInSecs, now, alarmAt)
    timer.tickAfter(plan.tickInMs)
    plan.alarmAt?.let { at ->
      // Remembered only when Android took it, so a refused one is asked for again at the next tick.
      if (safely("alarm", false) { wakeups.at(at); true }) alarmAt = at
    }
  }

  private fun viewMayHaveChanged(orders: PhoneOrders, permissions: PhonePermissions): Boolean {
    val changed = orders.timeLeft.words != lastWords ||
      orders.cover.kind != lastCover ||
      permissions != lastPermissions ||
      orders.notes.isNotEmpty()
    lastWords = orders.timeLeft.words
    lastCover = orders.cover.kind
    lastPermissions = permissions
    return changed
  }

  private fun close(engine: PhoneEngineInterface?) {
    if (engine is AutoCloseable) safely("engine_close", Unit) { engine.close() }
  }

  private inline fun <T> safely(what: String, fallback: T, block: () -> T): T {
    try {
      return block()
    } catch (e: Exception) {
      failed(what, e)
    } catch (e: LinkageError) {
      failed(what, e)
    }
    return fallback
  }

  /** One log line a minute at most for each kind of failure: what failed and the kind of error, never its message. */
  private fun failed(what: String, e: Throwable) {
    val key = "$what ${e.javaClass.simpleName}"
    val now = try {
      clock.elapsedMs()
    } catch (_: Exception) {
      0L
    }
    val last = failedAt[key]
    if (last != null && now - last < HostTimes.LOG_REPEAT_MS) return
    failedAt[key] = now
    log.line("host failed what=$what error=${e.javaClass.simpleName}")
  }
}

/** The stand in [Shower] until show/ gives one: a log line for each cover as it changes. */
internal class LoggingShower(private val log: HostLog) : Shower {
  private var last: String? = null

  override fun show(cover: Cover) {
    val line = when (cover.kind) {
      CoverKind.NOTHING -> "cover now nothing"
      CoverKind.BLOCK -> "cover now block package=${cover.block?.packageName}"
      CoverKind.LOCK -> "cover now lock why=${cover.lock?.why?.name?.lowercase()}"
    }
    if (line == last) return
    last = line
    log.line(line)
  }

  override fun warn(warning: WarningShown) {
    log.line("warning now cause=${warning.cause.name.lowercase()} minutes=${warning.minutes}")
  }
}
