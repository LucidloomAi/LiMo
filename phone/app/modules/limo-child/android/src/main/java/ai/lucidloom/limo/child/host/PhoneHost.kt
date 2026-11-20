package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.Cover
import ai.lucidloom.limo.child.ffi.EngineException
import ai.lucidloom.limo.child.ffi.LoadProblemKind
import ai.lucidloom.limo.child.ffi.PinAction
import ai.lucidloom.limo.child.ffi.TimeRequest
import ai.lucidloom.limo.child.ffi.TryAnswer
import ai.lucidloom.limo.child.ffi.WarningShown

/**
 * The engine's host, as the other pieces of the child form see it: the one
 * owner of the one Rust engine of the process ([Host.of]).
 *
 * **Threads.** Every call may be made from any thread and returns at once;
 * the work runs on the host's own thread, the only one that ever calls the
 * engine, one call at a time. Every answer is handed back on the main
 * thread.
 *
 * **Secrets.** A typed code or PIN goes in as a byte array, never a string,
 * and the host fills it with zeros once the engine has read it, whatever the
 * answer. The caller makes the bytes from the field it was typed in (a
 * `CharArray`), fills that too, and clears the field.
 *
 * **Never throws, never stops the process.** A failure is logged (tag
 * `LiMo`) and the next tick tries again; an engine that reports itself
 * broken is made again from the kept state.
 */
interface PhoneHost {
  /**
   * Starts the host if it is not running: loads the kept state and starts
   * ticking, or, with no state kept, waits for a pairing and does nothing
   * else. Calling it again does nothing.
   */
  fun start()

  /**
   * A state is kept on this phone: it is paired. True also while that state
   * cannot be read (the phone is then locked until a guardian pairs it
   * again). Read from the file itself, so it is right on any thread at once.
   */
  val paired: Boolean

  /**
   * The child's name, as the pairing gave it, for the child's screens and
   * the lock: trimmed, control characters and direction marks dropped, at
   * most 40 characters. Null when not paired, or when no name is kept (the
   * pairing gave none, or its file was lost or damaged). Kept in its own
   * file beside the engine's state (`files/limo/child.json`), never in a log
   * line, a report or the engine's state. Read from that file, so it is
   * right at once on any thread; a change is told as [HostChange.PAIRING].
   */
  val childName: String?

  /**
   * Who draws the covers (show/). Until one is given, a stand in only logs
   * each cover as it changes. See [Shower].
   */
  fun useShower(shower: Shower)

  /** Ticks at once, for a receiver that was woken; [done], if given, runs on the host's thread after it. */
  fun wake(done: (() -> Unit)? = null)

  /**
   * The child asks for [minutes] more today, with a short [reason] (or an
   * empty one), from the lock. The answer is the core's request record, or
   * its refusal in its own words.
   */
  fun askForTime(minutes: Int, reason: String, answer: (WayOut<TimeRequest>) -> Unit)

  /** A one time code typed on the lock, as UTF-8 bytes; [code] is filled with zeros. */
  fun tryCode(code: ByteArray, answer: (WayOut<TryAnswer>) -> Unit)

  /** A guardian's PIN typed on the lock, as UTF-8 bytes, to do what [action] says; [pin] is filled with zeros. */
  fun tryPin(pin: ByteArray, action: PinAction, answer: (WayOut<TryAnswer>) -> Unit)

  /**
   * A guardian's PIN typed on the guard, as UTF-8 bytes, to open the pages
   * that could switch LiMo off or remove it for a few minutes; [pin] is
   * filled with zeros. A right PIN answers `RELEASE_CONFIRMED` (the core's
   * way of saying only that the PIN was right), with the engine's words for
   * until when; wrong PINs count with the lock's.
   */
  fun tryGuardPin(pin: ByteArray, answer: (WayOut<TryAnswer>) -> Unit)

  /**
   * A guardian finished setup on this phone: they left setup's last screen
   * (the seam calls it for the screens). Until the engine has it, it counts
   * nothing and covers nothing, and the time left says setup is not
   * finished. `Done(true)` when this finished it, `Done(false)` when it was
   * finished already, which changes nothing; `Refused` when the kept state
   * cannot be read; `NotPaired`; `Failed`. Kept before the answer, and
   * ticked at once, so covers and counting start then.
   */
  fun finishSetup(answer: (WayOut<Boolean>) -> Unit)

  /** The phone as the engine holds it now, for the child's own screens. */
  fun view(answer: (HostView) -> Unit)

  /** Told on the main thread when the pairing or the view may have changed. Close the handle to stop. */
  fun listen(listener: HostListener): AutoCloseable

  /**
   * Pairs the phone from a welcome that came by another way than the link
   * (the development tool over the cable): the core's welcome grant as JSON
   * bytes and the 32 byte code secret, both filled with zeros after, and
   * the child's name ([childName]). It replaces any state and name kept
   * before, a state that cannot be read among them. The answer says
   * whether the phone is paired and the state kept.
   */
  fun pair(welcome: ByteArray, codeSecret: ByteArray, childName: String?, answer: (Boolean) -> Unit)

  /** Forgets the pairing: the kept state and the child's name are deleted, and nothing is covered any more. */
  fun forget(answer: () -> Unit)
}

/**
 * Who draws what the engine orders over the screen: the covers piece
 * (show/) implements it, as views in the accessibility service's overlay
 * windows. Called on the main thread.
 *
 * [show] is called at every tick, about once a second while the screen is
 * on and at once on any change, with the cover as a level: draw it if it is
 * not drawn, change it if it differs, take it away for `NOTHING`. The same
 * cover given twice changes nothing; one missed is given again at the next
 * tick. [warn] is called once, in the tick the warning falls due.
 */
interface Shower {
  fun show(cover: Cover)

  fun warn(warning: WarningShown) = Unit
}

/** The answer to a way out of the lock. */
sealed class WayOut<out T> {
  /** The engine's answer. The state was kept before it was handed back. */
  data class Done<T>(val value: T) : WayOut<T>()

  /**
   * The core refused, in its own words ([EngineException.message], such as
   * "A request from this device is still waiting for an answer."). The
   * exception's class says which refusal it is.
   */
  data class Refused(val refusal: EngineException) : WayOut<Nothing>()

  /** The phone is not paired. */
  data object NotPaired : WayOut<Nothing>()

  /** Something failed inside LiMo (it is in the log); trying again may work. */
  data object Failed : WayOut<Nothing>()
}

/** The phone as the engine holds it, for the child's own screens. */
data class HostView(
  /** A state is kept on this phone. */
  val paired: Boolean,
  /**
   * The engine's whole view as JSON (snake case), with `today` and
   * `rules_words` for the screens: see the engine's README, "The child's
   * own screens". Null when not paired, or when the kept state cannot be
   * read.
   */
  val json: String?,
  /** Why the kept state cannot be read, when it cannot; the phone is locked meanwhile. */
  val problem: LoadProblemKind?,
)

/** Told when something the screens show may have changed. Called on the main thread. */
fun interface HostListener {
  fun changed(what: HostChange)
}

/** What may have changed. */
enum class HostChange {
  /** The phone was paired or forgotten: ask [PhoneHost.paired] and [PhoneHost.childName] again. */
  PAIRING,

  /** The time left, the cover, the apps used, the permissions or the rules: ask [PhoneHost.view] again. */
  VIEW,
}
