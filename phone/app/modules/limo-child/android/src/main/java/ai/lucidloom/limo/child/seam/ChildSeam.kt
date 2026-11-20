package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.host.HostView
import ai.lucidloom.limo.child.host.WayOut
import ai.lucidloom.limo.child.watch.LimoPermissions

// What the seam needs from the rest of the child form, each behind a small
// interface, so that the JVM tests drive the requests with the REAL engine's
// view and a made up phone.

/** The engine's host, as the seam asks it (host/). */
internal interface SeamHost {
  /** The engine's view now; see [HostView]. Never on the main thread: it waits for the host's answer. */
  fun view(): HostView

  /** The child's name the host keeps (`PhoneHost.childName`); null when none is kept. */
  fun childName(): String?

  /**
   * Tells the engine that a guardian finished setup on this phone
   * (`PhoneHost.finishSetup`), and waits for its answer. Never on the main
   * thread.
   */
  fun finishSetup(): WayOut<Boolean>
}

/** The phone, as the seam asks it (the watch, and Android itself). */
internal interface SeamPhone {
  /** Every permission LiMo needs, read now. */
  fun permissions(): LimoPermissions

  /** Each app's own name by its package, for the apps that have one. */
  fun names(): Map<String, String>

  /** Who made the phone, as Android says it (`Build.MANUFACTURER`). */
  fun maker(): String
}

/**
 * What the seam keeps on the phone for setup, apart from the engine's state:
 * for one pairing (the engine's device id), whether a guardian agreed (S2)
 * and whether the child saw the introduction (C1 to C3).
 */
internal data class SetupMarks(
  val device: String,
  val agreed: Boolean,
  val introSeen: Boolean,
)

/** Where [SetupMarks] are kept: a small file of the seam's own. */
internal interface MarksStore {
  /** What is kept, or null when nothing is (or it cannot be read). */
  fun read(): SetupMarks?

  /** Keeps [marks] in place of what was kept. Throws when it cannot. */
  fun write(marks: SetupMarks)
}

/** Where the seam's log lines go: a request's name or a kind of failure, never a name or a sentence. */
internal fun interface SeamLog {
  fun line(text: String)
}

/**
 * The child's screens' requests (`child.<call>` in `src/host.ts`) that are
 * answered with data, from the host's view and the child's name it keeps,
 * the watch, and the seam's own marks. The two that open something of
 * Android's (`child.openStep`, `child.askForMoreTime`) are answered by
 * `LimoChildModule` through [StepOpener] instead.
 *
 * [changed] is told what a request itself changed (agreeing, the
 * introduction seen), for the `change` event.
 */
internal class ChildSeam(
  private val host: SeamHost,
  private val phone: SeamPhone,
  private val marks: MarksStore,
  private val log: SeamLog = SeamLog {},
  private val changed: (SeamChange) -> Unit = {},
) {
  /** The fallback for the name was last logged, so it is logged once until a name is seen. */
  @Volatile
  private var namelessLogged = false

  /** The answer to request [name]; throws [SeamProblem] when there is none. */
  fun answer(name: String): Any? = when (name) {
    "child.pairing" -> pairing(now())
    "child.agreed" -> now()?.let { marksFor(it)?.agreed } ?: false
    "child.agree" -> {
      mark { it.copy(agreed = true) }
      changed(SeamChange.AGREEMENT)
      null
    }
    "child.introSeen" -> now()?.let { marksFor(it)?.introSeen } ?: false
    "child.markIntroSeen" -> {
      mark { it.copy(introSeen = true) }
      changed(SeamChange.INTRO)
      // A second chance: the introduction comes after setup's last screen,
      // so setup is finished by now. Already finished, nothing changes; a
      // failure here is logged and the introduction stays seen.
      try {
        finishSetup()
      } catch (e: SeamProblem) {
        log.line("seam failed what=finish_setup error=${e.code}")
      }
      null
    }
    "child.finishSetup" -> {
      finishSetup()
      null
    }
    "child.steps" -> SeamAnswers.steps(phone.permissions())
    "child.today" -> SeamAnswers.today(viewOf(now()), phone.names())
    "child.rules" -> SeamAnswers.rules(viewOf(now()), phone.names())
    "child.maker" -> phone.maker()
    else -> throw SeamProblem(SeamProblem.UNKNOWN_REQUEST, "The child module has no request named $name.")
  }

  /**
   * The answers the screens would get now for each of [kinds], for the
   * `change` event: the host is asked once for all of them. A kind whose
   * answer fails is given as [FAILED].
   */
  fun answersFor(kinds: Collection<SeamChange>): Map<SeamChange, Any?> {
    val out = LinkedHashMap<SeamChange, Any?>()
    // One view and one list of names for all.
    val now = Once { now() }
    val names = Once { phone.names() }
    for (kind in kinds) {
      out[kind] = try {
        when (kind) {
          SeamChange.PAIRING -> pairing(now.get())
          SeamChange.AGREEMENT -> now.get()?.let { marksFor(it)?.agreed } ?: false
          SeamChange.INTRO -> now.get()?.let { marksFor(it)?.introSeen } ?: false
          SeamChange.STEPS -> SeamAnswers.steps(phone.permissions())
          SeamChange.TODAY -> now.get()?.let { SeamAnswers.today(viewOf(it), names.get()) } ?: NOT_PAIRED
          SeamChange.RULES -> now.get()?.let { SeamAnswers.rules(viewOf(it), names.get()) } ?: NOT_PAIRED
        }
      } catch (_: Exception) {
        FAILED
      }
    }
    return out
  }

  /**
   * The phone as the host has it: paired, with the engine's view and this
   * pairing's device id when the engine reads its kept state. A kept state
   * the engine cannot read is still a pairing (the engine runs, and locks
   * the phone), with no view and no device id.
   */
  private class Now(val view: Map<String, Any?>?, val device: String?)

  /** Null when not paired. */
  private fun now(): Now? {
    val v = host.view()
    if (!v.paired) return null
    val json = v.json ?: return Now(null, null)
    val view = SeamAnswers.readView(json)
    return Now(view, SeamAnswers.deviceId(view))
  }

  /**
   * `child.pairing`: never "not paired" while an engine runs, or the phone
   * would keep its limits while its own screen said it was not set up.
   */
  private fun pairing(now: Now?): Map<String, Any?> {
    if (now == null) return SeamAnswers.notPaired()
    val name = host.childName()
    if (name == null && !namelessLogged) log.line("seam pairing no_child_name fallback")
    namelessLogged = name == null
    return SeamAnswers.paired(name)
  }

  /**
   * `child.finishSetup`: the guardian left setup's last screen ("Show the
   * child how it works"), with every step on or one passed. Until the
   * engine has it, LiMo counts nothing and covers nothing. Finished again,
   * nothing changes. Refused as the other requests are when the phone is
   * not paired or its kept state cannot be read.
   */
  private fun finishSetup() {
    when (val answer = host.finishSetup()) {
      is WayOut.Done -> if (answer.value) log.line("seam setup finished")
      is WayOut.Refused -> throw SeamProblem(SeamProblem.CANNOT_READ, "The kept state cannot be read.")
      WayOut.NotPaired -> throw SeamProblem(SeamProblem.NOT_PAIRED, "The phone is not paired.")
      WayOut.Failed -> throw SeamProblem(SeamProblem.FAILED, "The engine could not take it.")
    }
  }

  private fun viewOf(now: Now?): Map<String, Any?> {
    if (now == null) throw SeamProblem(SeamProblem.NOT_PAIRED, "The phone is not paired.")
    return now.view ?: throw SeamProblem(SeamProblem.CANNOT_READ, "The kept state cannot be read.")
  }

  /**
   * The marks of this pairing; those of another (or none) are none. With
   * a kept state the engine cannot read, the device id is not known, and
   * the marks kept are taken as they are.
   */
  private fun marksFor(now: Now): SetupMarks? = marks.read()?.takeIf { now.device == null || it.device == now.device }

  /** Changes this pairing's marks, starting from none for a new pairing. One change at a time, from any thread. */
  private fun mark(change: (SetupMarks) -> SetupMarks) {
    val now = now() ?: throw SeamProblem(SeamProblem.NOT_PAIRED, "The phone is not paired.")
    val device = now.device ?: throw SeamProblem(SeamProblem.CANNOT_READ, "The kept state cannot be read.")
    synchronized(this) {
      marks.write(change(marksFor(now) ?: SetupMarks(device, agreed = false, introSeen = false)))
    }
  }

  /** A value made on first use, or its failure, once. */
  private class Once<T>(private val make: () -> T) {
    private var made = false
    private var value: T? = null
    private var failure: Exception? = null

    fun get(): T {
      if (!made) {
        made = true
        try {
          value = make()
        } catch (e: Exception) {
          failure = e
        }
      }
      failure?.let { throw it }
      @Suppress("UNCHECKED_CAST")
      return value as T
    }
  }

  companion object {
    /** In [answersFor], a kind whose answer failed. */
    val FAILED = Any()

    /** In [answersFor], Today or the rules while the phone is not paired. */
    val NOT_PAIRED = Any()
  }
}
