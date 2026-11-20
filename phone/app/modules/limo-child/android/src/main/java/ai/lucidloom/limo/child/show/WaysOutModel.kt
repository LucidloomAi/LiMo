package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.PinAction
import ai.lucidloom.limo.child.ffi.PinActionKind
import ai.lucidloom.limo.child.ffi.TimeRequest
import ai.lucidloom.limo.child.ffi.TryAnswer
import ai.lucidloom.limo.child.ffi.TryKind
import ai.lucidloom.limo.child.host.WayOut
import java.time.ZoneId

/** The three ways out, each a screen of its own: B1, B2 and B3. */
internal enum class Panel { ASK, CODE, PIN }

/** How long a child asks for: the drawing's four chips. */
internal enum class AskChoice(val minutes: Int, val label: String) {
  MIN15(15, "15 min"),
  MIN30(30, "30 min"),
  MIN45(45, "45 min"),
  HOUR(60, "1 hour"),
}

/** What a guardian gives with the PIN: three amounts to add, and three ways to keep it open, as drawn. */
internal enum class PinChoice(val label: String, val addMinutes: Int?, val openSecs: Long?) {
  ADD15("15 min", 15, null),
  ADD30("30 min", 30, null),
  ADD60("1 hour", 60, null),
  OPEN60("1 hour", null, 3_600L),
  OPEN120("2 hours", null, 7_200L),
  TODAY("Today", null, null),
  ;

  val adds: Boolean get() = addMinutes != null

  /**
   * The engine's action: add the minutes, or keep it open until an hour or
   * two from now, or until the end of today on the phone's clock (at least
   * a minute away), as the PC's lock screen does.
   */
  fun action(nowSecs: Long, zone: ZoneId): PinAction = when {
    addMinutes != null -> PinAction(PinActionKind.ADD_MINUTES, minutes = addMinutes)
    openSecs != null -> PinAction(PinActionKind.UNLOCK_UNTIL, until = nowSecs + openSecs)
    else -> PinAction(PinActionKind.UNLOCK_UNTIL, until = maxOf(ClockWords.endOfDay(nowSecs, zone), nowSecs + 60))
  }
}

/** A line under the field: the engine's words, coral when it was not right, teal when it was. */
internal data class Message(val text: String, val right: Boolean)

/** A call to the engine, through the host, made by [WaysOutModel.submit]. */
internal sealed class WayCall {
  data class Ask(val minutes: Int, val reason: String) : WayCall()

  /** The typed digits as UTF-8 bytes. The host fills them with zeros once the engine has read them. */
  class Code(val typed: ByteArray) : WayCall()

  class Pin(val typed: ByteArray, val action: PinAction) : WayCall()
}

/** What the screen does after an answer. */
internal sealed class After {
  /** Stay, showing [WaysOutModel.message]. */
  data object Stay : After()

  /** Back to the lock now: it shows a request that waits, or the wait. */
  data object Close : After()

  /** A right answer: show it for [Look.OPEN_SHOWN_MS], then close; the engine opens the phone, or the lock is there still. */
  data object CloseSoon : After()
}

/**
 * One way out's screen, with no Android in it. Main thread only.
 *
 * **Secrets.** The digits of a code or a PIN live in one char array, never
 * in a string. [submit] turns them into the bytes the host takes and fills
 * the array with zeros; [back] and [wipe] fill it too. [saved] never holds
 * them, so a screen rebuilt after a turn of the phone starts with an empty
 * field. Nothing here logs.
 */
internal class WaysOutModel(val panel: Panel) {
  var ask: AskChoice = AskChoice.MIN30
    private set

  var pin: PinChoice = PinChoice.ADD30
    private set

  /** The reason as typed so far: not a secret, but never logged. */
  var reason: String = ""

  var message: Message? = null
    private set

  /** A call is with the engine: taps wait for its answer. */
  var busy: Boolean = false
    private set

  private val digits = CharArray(MAX_DIGITS)

  var typed: Int = 0
    private set

  /** A digit of a code, for its cell; never a PIN's (the PIN box shows dots). */
  fun digit(i: Int): Char? = if (panel == Panel.CODE && i < typed) digits[i] else null

  fun type(d: Char): Boolean {
    if (busy || panel == Panel.ASK || d !in '0'..'9' || typed >= MAX_DIGITS) return false
    digits[typed++] = d
    if (message != null && message?.right == false) message = null
    return true
  }

  fun delete(): Boolean {
    if (busy || typed == 0) return false
    typed -= 1
    digits[typed] = '\u0000'
    return true
  }

  fun pick(choice: AskChoice) {
    if (!busy) ask = choice
  }

  fun pick(choice: PinChoice) {
    if (!busy) pin = choice
  }

  /** The call to make now, or null while one is with the engine. */
  fun submit(nowSecs: Long, zone: ZoneId): WayCall? {
    if (busy) return null
    val call = when (panel) {
      Panel.ASK -> WayCall.Ask(ask.minutes, reason)
      Panel.CODE -> WayCall.Code(bytes())
      Panel.PIN -> WayCall.Pin(bytes(), pin.action(nowSecs, zone))
    }
    wipe()
    busy = true
    return call
  }

  /** The engine's answer to the call [submit] made, in the engine's words. */
  fun answered(answer: WayOut<Any>): After {
    busy = false
    return when (answer) {
      is WayOut.NotPaired -> After.Close
      is WayOut.Failed -> After.Close
      is WayOut.Refused -> {
        message = Message(Typo.quotes(answer.refusal.message.orEmpty()), right = false)
        After.Stay
      }
      is WayOut.Done -> when (val v = answer.value) {
        is TimeRequest -> After.Close
        is TryAnswer -> tried(v)
        else -> After.Close
      }
    }
  }

  private fun tried(a: TryAnswer): After = when (a.kind) {
    TryKind.ADDED, TryKind.UNLOCKED, TryKind.RELEASE_CONFIRMED -> {
      val opens = a.opensNow?.takeIf { it.isNotBlank() }
      message = Message(Typo.quotes(listOfNotNull(a.message, opens).joinToString(" ")), right = true)
      busy = true
      After.CloseSoon
    }
    // The wait has begun: the lock says so, with its rows greyed.
    TryKind.MUST_WAIT -> After.Close
    TryKind.WRONG -> if (a.waitUntil != null) {
      After.Close
    } else {
      message = Message(Typo.quotes(a.message), right = false)
      After.Stay
    }
    TryKind.MALFORMED, TryKind.ALREADY_USED, TryKind.NOT_SET -> {
      message = Message(Typo.quotes(a.message), right = false)
      After.Stay
    }
  }

  /** Back to the lock: what was typed is wiped. */
  fun back(): After {
    wipe()
    return After.Close
  }

  fun wipe() {
    digits.fill('\u0000')
    typed = 0
  }

  /** True when every char of the digits is zero: for the tests of the secrets. */
  internal fun wiped(): Boolean = typed == 0 && digits.all { it == '\u0000' }

  private fun bytes(): ByteArray = ByteArray(typed) { digits[it].code.toByte() }

  /** What survives the screen being rebuilt: never the digits. */
  fun saved(): Saved = Saved(panel, ask, pin, message)

  data class Saved(val panel: Panel, val ask: AskChoice, val pin: PinChoice, val message: Message?)

  fun restore(s: Saved, waiting: Boolean) {
    ask = s.ask
    pin = s.pin
    message = s.message
    busy = waiting
  }

  companion object {
    /** A code has 8 digits, a PIN 6 to 8. */
    const val MAX_DIGITS = 8

    /** The longest reason the rules core takes. */
    const val MAX_REASON = 200
  }
}

/**
 * Carries a way out's answer to its screen across the screen being rebuilt
 * (the phone turned, the font size changed): the screen that made the call
 * may be gone when the answer arrives, and its new self picks it up by the
 * call's number. One per process ([WaysOutCalls]); main thread only. Holds
 * answers, never what was typed.
 */
internal class CallRelay {
  private object Pending

  private var next = 1
  private val calls = LinkedHashMap<Int, Any>()
  private var sink: ((Int, WayOut<Any>) -> Unit)? = null

  /** A new call: its number, to keep with the screen. */
  fun begin(): Int {
    val id = next++
    calls[id] = Pending
    return id
  }

  /** The host answered call [id]. */
  @Suppress("UNCHECKED_CAST")
  fun answered(id: Int, answer: WayOut<Any>) {
    if (!calls.containsKey(id)) return
    val s = sink
    if (s != null) {
      calls.remove(id)
      s(id, answer)
    } else {
      calls[id] = answer
    }
  }

  /**
   * A screen is up and waits for [id] (or for nothing). An answer that came
   * while no screen was up is handed over now. False when [id] is not known
   * (the process was started again meanwhile): nothing will come.
   */
  @Suppress("UNCHECKED_CAST")
  fun attach(id: Int?, sink: (Int, WayOut<Any>) -> Unit): Boolean {
    this.sink = sink
    if (id == null) return true
    val held = calls[id] ?: return false
    if (held !== Pending) {
      calls.remove(id)
      sink(id, held as WayOut<Any>)
    }
    return true
  }

  fun detach() {
    sink = null
  }
}

/** The process's relay. */
internal object WaysOutCalls {
  val relay = CallRelay()
}
