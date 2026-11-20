package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.host.Host
import ai.lucidloom.limo.child.host.HostChange
import ai.lucidloom.limo.child.host.HostView
import ai.lucidloom.limo.child.host.PhoneHost
import ai.lucidloom.limo.child.watch.LimoPermissions
import ai.lucidloom.limo.child.watch.PhoneWatch
import ai.lucidloom.limo.child.watch.Watch
import ai.lucidloom.limo.child.watch.WatchChange
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.SystemClock
import android.util.AtomicFile
import android.util.Log
import java.io.File
import java.io.IOException
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit

/**
 * The seam of the process: the child's screens' side of LiMo on Android,
 * one per process ([of]). `LimoChildModule` hands it each request and the
 * screens' listening.
 *
 * Its work runs on a thread of its own ("LiMo seam") or on Expo's own queue
 * for requests, never on the main thread, because it waits for the host's
 * answers, which come on the main thread.
 */
internal class Seam private constructor(context: Context) {
  val app: Context = context.applicationContext ?: context
  private val main = Handler(Looper.getMainLooper())
  private val thread = HandlerThread("LiMo seam").apply { start() }
  private val worker = Handler(thread.looper)
  private val host: PhoneHost = Host.of(app)
  private val watch: PhoneWatch = Watch.of(app)
  private val phone = WatchSeamPhone(watch)

  /** The requests answered with data. */
  val requests: ChildSeam = ChildSeam(HostSeamHost(host), phone, FileMarks(marksFile(app)), AndroidSeamLog) { kind -> events.note(listOf(kind)) }

  /** The `change` event. */
  val events: SeamEvents = SeamEvents(worker, host, watch, phone, AndroidSeamLog) { kinds -> requests.answersFor(kinds) }

  /** What opens Android's screens for the steps, and LiMo's ways out. */
  val opener = StepOpener(app, main, AndroidSeamLog)

  /** Whether a state is kept: read from the file, right at once on any thread. */
  val paired: Boolean get() = host.paired

  companion object {
    @Volatile
    private var instance: Seam? = null

    fun of(context: Context): Seam = instance ?: synchronized(this) { instance ?: Seam(context).also { instance = it } }

    /** The seam's own file: the app's private `files/limo-seam/setup.json`, never backed up (as nothing of the app is). */
    fun marksFile(context: Context): File = File(context.filesDir, "limo-seam/setup.json")
  }
}

/** The host, as the seam asks it: its view, waited for on the seam's thread. */
internal class HostSeamHost(private val host: PhoneHost) : SeamHost {
  override fun view(): HostView {
    if (Looper.myLooper() == Looper.getMainLooper()) {
      throw SeamProblem(SeamProblem.NO_ANSWER, "The host's view is never waited for on the main thread.")
    }
    val answer = ArrayBlockingQueue<HostView>(1)
    host.view { answer.offer(it) }
    return answer.poll(VIEW_WAIT_MS, TimeUnit.MILLISECONDS)
      ?: throw SeamProblem(SeamProblem.NO_ANSWER, "The host did not answer in time.")
  }

  override fun childName(): String? = host.childName

  companion object {
    /** The host answers within a tick; this is far more. */
    const val VIEW_WAIT_MS = 5_000L
  }
}

/**
 * The phone, as the seam asks it, from the watch. Each app's name comes
 * from the watch's list of the apps a child can open, which is slow to
 * make: it is kept while the seam listens to the watch (which says when
 * apps change), and made again for each request otherwise.
 */
internal class WatchSeamPhone(private val watch: PhoneWatch) : SeamPhone {
  @Volatile
  private var keep = false

  @Volatile
  private var names: Map<String, String>? = null

  override fun permissions(): LimoPermissions = watch.permissions()

  override fun names(): Map<String, String> =
    names ?: SeamAnswers.namesOf(watch.launchableApps()).also { if (keep) names = it }

  override fun maker(): String = Build.MANUFACTURER.orEmpty()

  /** While [on], names are kept until [forgetNames]. */
  fun keepNames(on: Boolean) {
    keep = on
    if (!on) names = null
  }

  fun forgetNames() {
    names = null
  }
}

/**
 * The seam's marks in one small file, written all or nothing with Android's
 * AtomicFile: `{"device": ..., "agreed": ..., "intro_seen": ...}`.
 * Apart from the engine's state (`files/limo/state`), which the seam never
 * touches. The app keeps everything out of backups and device transfers.
 */
internal class FileMarks(private val file: File) : MarksStore {
  private val atomic = AtomicFile(file)

  override fun read(): SetupMarks? {
    if (!file.exists() && !File(file.path + ".bak").exists()) return null
    return try {
      parse(String(atomic.readFully(), Charsets.UTF_8))
    } catch (e: IOException) {
      AndroidSeamLog.line("seam failed what=marks_read error=${e.javaClass.simpleName}")
      null
    }
  }

  override fun write(marks: SetupMarks) {
    file.parentFile?.mkdirs()
    val out = atomic.startWrite()
    try {
      out.write(text(marks).toByteArray(Charsets.UTF_8))
      atomic.finishWrite(out)
    } catch (e: IOException) {
      atomic.failWrite(out)
      throw e
    }
  }

  companion object {
    fun text(m: SetupMarks): String = buildString {
      append("{\"device\":").append(quote(m.device))
      append(",\"agreed\":").append(m.agreed)
      append(",\"intro_seen\":").append(m.introSeen)
      append('}')
    }

    /** The marks in [text], or null when it is not marks (then setup starts again, as with none). */
    fun parse(text: String): SetupMarks? {
      val o = try {
        SeamJson.read(text) as? Map<*, *>
      } catch (_: SeamJson.Bad) {
        null
      } ?: return null
      val device = o["device"] as? String ?: return null
      return SetupMarks(
        device = device,
        agreed = o["agreed"] == true,
        introSeen = o["intro_seen"] == true,
      )
    }

    private fun quote(s: String): String = buildString {
      append('"')
      for (c in s) {
        when {
          c == '"' -> append("\\\"")
          c == '\\' -> append("\\\\")
          c < ' ' -> append("\\u%04x".format(c.code))
          else -> append(c)
        }
      }
      append('"')
    }
  }
}

/**
 * The `change` event: told by the host and the watch what may have
 * changed, it looks at the answers through [SeamChanges] and sends each
 * kind whose answer really changed. It listens only while the screens
 * listen (`OnStartObserving`) and the app is in front; nothing runs
 * otherwise. Everything here runs on the seam's thread.
 */
internal class SeamEvents(
  private val worker: Handler,
  private val host: PhoneHost,
  private val watch: PhoneWatch,
  private val phone: WatchSeamPhone,
  private val log: SeamLog,
  private val answers: (Collection<SeamChange>) -> Map<SeamChange, Any?>,
) {
  private val gate = SeamChanges()
  private var send: ((String) -> Unit)? = null
  private var inFront = true
  private var handles: List<AutoCloseable> = emptyList()
  private val look = Runnable { guarded("look") { lookNow() } }

  /** The screens listen ([sender], which sends one event) or stopped ([sender] null). Any thread. */
  fun listening(sender: ((String) -> Unit)?) {
    worker.post {
      guarded("listening") {
        val starting = send == null && sender != null
        send = sender
        sync()
        if (sender == null) gate.reset()
        // The screens ask for what they show as they begin to listen.
        if (starting && active()) gate.prime(answers(SeamChange.entries))
      }
    }
  }

  /** The app came to the front or left it. Any thread. */
  fun front(front: Boolean) {
    worker.post {
      guarded("front") {
        inFront = front
        sync()
        // Anything may have changed while away: each kind is looked at, and sent only if it did.
        if (front) noteHere(SeamChange.entries)
      }
    }
  }

  /** [kinds] may have changed. Any thread. */
  fun note(kinds: Collection<SeamChange>) {
    worker.post { guarded("note") { noteHere(kinds) } }
  }

  private fun active() = send != null && inFront

  private fun sync() {
    if (active() && handles.isEmpty()) {
      phone.keepNames(true)
      handles = listOf(
        host.listen { what -> note(onHost(what)) },
        watch.listen { what ->
          if (what == WatchChange.APPS) phone.forgetNames()
          note(onWatch(what))
        },
      )
    } else if (!active() && handles.isNotEmpty()) {
      for (h in handles) guarded("stop") { h.close() }
      handles = emptyList()
      phone.keepNames(false)
      gate.pause()
      worker.removeCallbacks(look)
    }
  }

  private fun noteHere(kinds: Collection<SeamChange>) {
    if (!active()) return
    val now = SystemClock.elapsedRealtime()
    gate.note(kinds, now)?.let { at -> worker.postDelayed(look, (at - now).coerceAtLeast(0L)) }
  }

  private fun lookNow() {
    val sender = send
    if (!active() || sender == null) {
      gate.pause()
      return
    }
    for (kind in gate.look(SystemClock.elapsedRealtime(), answers)) sender(kind.word)
  }

  private inline fun guarded(what: String, block: () -> Unit) {
    try {
      block()
    } catch (e: Exception) {
      log.line("seam failed what=$what error=${e.javaClass.simpleName}")
    }
  }

  companion object {
    /** What the host's word may have changed. */
    fun onHost(what: HostChange): List<SeamChange> = when (what) {
      HostChange.PAIRING -> listOf(SeamChange.PAIRING, SeamChange.AGREEMENT, SeamChange.INTRO, SeamChange.TODAY, SeamChange.RULES)
      HostChange.VIEW -> listOf(SeamChange.TODAY, SeamChange.RULES)
    }

    /** What the watch's word may have changed. The front, the screen and calls reach the screens through the host's view. */
    fun onWatch(what: WatchChange): List<SeamChange> = when (what) {
      WatchChange.PERMISSIONS -> listOf(SeamChange.STEPS, SeamChange.TODAY)
      WatchChange.APPS, WatchChange.CLOCK, WatchChange.TIME_ZONE -> listOf(SeamChange.TODAY, SeamChange.RULES)
      WatchChange.FRONT, WatchChange.SCREEN, WatchChange.CALL -> emptyList()
    }
  }
}

/** The seam's log: Android's log, tag `LiMo`, the host's. */
internal object AndroidSeamLog : SeamLog {
  private const val TAG = "LiMo"

  override fun line(text: String) {
    Log.i(TAG, text)
  }
}
