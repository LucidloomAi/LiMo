package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.BlockCover
import ai.lucidloom.limo.child.ffi.Cover
import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.LockCover
import ai.lucidloom.limo.child.ffi.LockWhy
import ai.lucidloom.limo.child.ffi.NothingToDo
import ai.lucidloom.limo.child.ffi.RequestAnswer
import ai.lucidloom.limo.child.ffi.RequestLine
import ai.lucidloom.limo.child.ffi.RequestShown
import ai.lucidloom.limo.child.ffi.TimeRequest
import ai.lucidloom.limo.child.ffi.Way
import ai.lucidloom.limo.child.ffi.WayRow
import java.io.File

// Made up orders in the engine's own words (phone/engine/README.md, "The
// words"), as the drawings show them: Mira's phone at 9:42 pm, her time used
// up, the apps Phone, Messages, Clock and Maps still open.

internal const val DIALLER = "org.example.dialler"
internal const val MESSAGES = "org.example.messages"
internal const val CLOCK_APP = "org.example.clock"
internal const val MAPS = "org.example.maps"
internal const val GONE = "org.example.uninstalled"
internal const val LIMO = "ai.lucidloom.limo.child"

internal val LABELS = linkedMapOf(DIALLER to "Phone", MESSAGES to "Messages", CLOCK_APP to "Clock", MAPS to "Maps")

/** The phone's apps, made up: the dialler and three always allowed apps; one allowed app not installed. */
internal class FakeApps(
  override val dialler: String? = DIALLER,
  private val labels: Map<String, String> = LABELS,
) : AppCatalog {
  override val ownPackage: String = LIMO
  var asked = 0

  override fun tile(packageName: String): TilePlan? {
    asked += 1
    return labels[packageName]?.let { TilePlan(packageName, it) }
  }
}

internal fun row(way: Way, off: Boolean = false): WayRow {
  val (title, sub) = when (way) {
    Way.ASK -> "Ask for more time" to "A guardian sees your request in LiMo"
    Way.CODE -> "Type a one time code" to "A guardian who is away can read you 8 digits"
    Way.PIN -> "Guardian PIN" to "For a guardian who is with you"
  }
  return WayRow(way, title, if (off) "Wait until 9:47 pm" else sub, off)
}

internal fun lockCover(
  why: LockWhy = LockWhy.TIME_USED_UP,
  ways: List<WayRow> = listOf(row(Way.ASK), row(Way.CODE), row(Way.PIN)),
  allowed: List<String> = listOf(MESSAGES, CLOCK_APP, MAPS),
  waitWords: String? = null,
  request: RequestAnswer? = null,
  nothing: NothingToDo? = null,
  callOn: Boolean = false,
): LockCover {
  val (title, message) = when (why) {
    LockWhy.BLOCKED_HOURS -> "Screen time is off for now." to "This is a time when the phone is not used."
    LockWhy.GUARDIAN_LOCK -> "Locked by a guardian." to "A guardian has locked the phone for now."
    else -> "Today's time is used up." to "You have used all of today's screen time."
  }
  val shown = request?.let {
    RequestShown(
      TimeRequest("r1", "c1", "d1", 20_775L, 30, "To finish my science project", 1_795_018_920L),
      it,
      if (it == RequestAnswer.GRANTED) 15 else null,
      null,
      true,
    )
  }
  val line = request?.let {
    RequestLine(
      "You asked for 30 min at 9:32 pm",
      when (it) {
        RequestAnswer.WAITING -> "Waiting"
        RequestAnswer.GRANTED -> "Given 15 min"
        RequestAnswer.REFUSED -> "Not this time"
      },
    )
  }
  return LockCover(
    why = why,
    stamp = "Locked",
    title = title,
    message = message,
    until = 1_795_048_200L,
    untilWords = "Tomorrow, 7:00 am",
    allowedPackages = allowed,
    canAskForTime = ways.any { it.way == Way.ASK },
    canUseCode = ways.any { it.way == Way.CODE },
    canUsePin = ways.any { it.way == Way.PIN },
    tryWaitUntil = if (waitWords != null) 1_795_019_820L else null,
    waitWords = waitWords,
    latestRequest = shown,
    requestLine = line,
    ways = ways,
    nothingToDo = nothing,
    callOn = callOn,
    backToCall = if (callOn) "Back to the call" else null,
  )
}

internal fun lock(l: LockCover = lockCover()) = Cover(CoverKind.LOCK, null, l)

internal fun block(label: String? = "Game Launcher", callOn: Boolean = false) = Cover(
  CoverKind.BLOCK,
  BlockCover(
    "org.example.gamelauncher",
    if (label != null) "$label is not allowed." else "This app is not allowed.",
    "It is blocked by your family's rules.",
    true,
    callOn,
    if (callOn) "Back to the call" else null,
  ),
  null,
)

internal val NOTHING = Cover(CoverKind.NOTHING, null, null)

/** Where a cover would be drawn: every call recorded, and a refusal on demand. */
internal class FakeCover : CoverSurface {
  val drawn = ArrayList<CoverPlan>()
  var removed = 0
  var refreshed = 0
  var refuse = false
  private var isUp = false

  override val up: Boolean get() = isUp

  override fun draw(plan: CoverPlan): Boolean {
    if (refuse) {
      isUp = false
      return false
    }
    drawn += plan
    isUp = true
    return true
  }

  override fun remove() {
    if (isUp) removed += 1
    isUp = false
  }

  override fun refresh() {
    refreshed += 1
  }

  /** Android took the window away (the service was switched off). */
  fun lost() {
    isUp = false
  }
}

internal class FakeCard : CardSurface {
  val floated = ArrayList<WarningPlan>()
  var removed = 0
  var refuse = false
  private var isUp = false

  override val up: Boolean get() = isUp

  override fun float(warning: WarningPlan): Boolean {
    if (refuse) return false
    floated += warning
    isUp = true
    return true
  }

  override fun remove() {
    if (isUp) removed += 1
    isUp = false
  }
}

/** A clock and timers the test moves by hand. */
internal class FakeTimer : ShowTimer {
  var now = 1_000_000L
  private class Due(val at: Long, val block: () -> Unit) {
    var cancelled = false
  }

  private val due = ArrayList<Due>()

  override fun after(ms: Long, block: () -> Unit): AutoCloseable {
    val d = Due(now + ms, block)
    due += d
    return AutoCloseable { d.cancelled = true }
  }

  override fun nowMs(): Long = now

  fun pass(ms: Long) {
    val end = now + ms
    while (true) {
      val next = due.filter { !it.cancelled && it.at <= end }.minByOrNull { it.at } ?: break
      now = next.at
      due.remove(next)
      next.block()
    }
    now = end
  }
}

/** The repository, for the design's files, the fonts and the logo. */
internal val REPO: File by lazy {
  System.getProperty("limo.repo")?.let { File(it) }?.takeIf { File(it, "design/tokens.json").isFile } ?: run {
    var dir: File? = File(System.getProperty("user.dir") ?: ".").absoluteFile
    while (dir != null && !File(dir, "design/tokens.json").isFile) dir = dir.parentFile
    dir ?: error("the repository was not found from the tests' working folder")
  }
}
