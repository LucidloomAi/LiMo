package ai.lucidloom.limo.child.watch

/**
 * Android's record of activities (usage events), as LiMo follows it: which
 * app's activity was resumed last, and which apps still have one on the
 * screen. It is read in pieces, each from a little before where the last one
 * ended, so an event Android wrote late is still taken, and an event read
 * twice is taken once.
 *
 * **The front is the app whose activity resumed last.** A window that is not
 * an activity (a keyboard, the notification shade, a floating bubble,
 * LiMo's own covers, a picture in picture window) never resumes one, so it
 * never moves the front; nor do the framework's own overlays
 * ([Overlays.FRAMEWORK]). An app that sends a window event after another
 * app came up (proof two's late event over the home screen) has not resumed
 * since, so it does not come back.
 *
 * Pure: no Android here.
 */
internal class UsageTrack(
  private val lookbackMs: Long = WatchTimes.LOOKBACK_MS,
  private val overlapMs: Long = WatchTimes.OVERLAP_MS,
) {
  /** A piece of the record to read: from [fromWallMs] up to [toWallMs], not including it. */
  data class Window(val fromWallMs: Long, val toWallMs: Long)

  private class Seen(val kind: UsageKind, val atWallMs: Long)

  /** The package whose activity resumed last, overlays aside; null when none was seen. */
  var front: String? = null
    private set

  /** Grows by one for every activity taken as the front, so a caller can tell that one resumed since. */
  var resumes: Long = 0
    private set

  /**
   * Grows by one each time the record is forgotten and read again from the
   * start (the first read, the clock set), so a caller can tell that what
   * the next read takes may be from long before.
   */
  var forgets: Long = 0
    private set

  private val taken = ArrayList<String>()

  /**
   * The packages the last [take] took as the front, oldest first: one entry
   * for every activity that resumed, the same package again when it resumed
   * again. Empty when that read moved nothing.
   */
  val lastTaken: List<String> get() = taken

  private var frontAtWallMs = Long.MIN_VALUE
  private val latest = HashMap<String, Seen>()
  private var readTo: Long? = null
  private val nearEdge = HashSet<UsageEvent>()

  /**
   * The piece to read next. From a little before where the last read ended;
   * or, for a first read, or when the clock went back past where the last
   * read ended, the record is forgotten and the piece goes back
   * [lookbackMs].
   */
  fun window(nowWallMs: Long): Window {
    val to = nowWallMs + 1
    val last = readTo
    if (last == null || nowWallMs < last - overlapMs) {
      forget()
      return Window(nowWallMs - lookbackMs, to)
    }
    return Window(last - overlapMs, to)
  }

  /**
   * Takes the events read for [window], oldest first. Returns true when the
   * front moved. [isOverlay] names the packages that never become the front.
   */
  fun take(events: List<UsageEvent>, window: Window, isOverlay: (String) -> Boolean): Boolean {
    val before = front
    taken.clear()
    // A stable sort keeps Android's own order for events with the same time.
    val fresh = events.filter { it.atWallMs >= window.fromWallMs && it !in nearEdge }.sortedBy { it.atWallMs }
    for (event in fresh) apply(event, isOverlay)
    readTo = window.toWallMs
    val edge = window.toWallMs - overlapMs
    nearEdge.removeAll { it.atWallMs < edge }
    for (event in events) if (event.atWallMs >= edge) nearEdge.add(event)
    return front != before
  }

  /**
   * Apps with an activity still on the screen: resumed, or paused but not
   * stopped (beside the front in split screen, in a picture in picture
   * window, or under a see through dialog). Only Android 10 and later report
   * an activity stopped; before that, only resumed ones can be told.
   */
  fun visible(stopsReported: Boolean): Set<String> =
    latest.filter { (_, seen) -> seen.kind == UsageKind.RESUMED || (stopsReported && seen.kind == UsageKind.PAUSED) }.keys

  /** Forgets the record: the next [window] reads it again from [lookbackMs] back. */
  fun forget() {
    front = null
    frontAtWallMs = Long.MIN_VALUE
    latest.clear()
    readTo = null
    nearEdge.clear()
    taken.clear()
    forgets += 1
  }

  private fun apply(event: UsageEvent, isOverlay: (String) -> Boolean) {
    if (event.kind == UsageKind.STARTUP) {
      // The phone started: nothing from before is on the screen any more.
      if (event.atWallMs >= frontAtWallMs) {
        front = null
        frontAtWallMs = event.atWallMs
      }
      latest.entries.removeAll { it.value.atWallMs <= event.atWallMs }
      return
    }
    val seen = latest[event.packageName]
    if (seen == null || event.atWallMs >= seen.atWallMs) latest[event.packageName] = Seen(event.kind, event.atWallMs)
    if (event.kind == UsageKind.RESUMED && !isOverlay(event.packageName) && event.atWallMs >= frontAtWallMs) {
      front = event.packageName
      frontAtWallMs = event.atWallMs
      resumes += 1
      taken.add(event.packageName)
    }
  }
}
