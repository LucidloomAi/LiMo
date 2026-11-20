package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.BlockCover
import ai.lucidloom.limo.child.ffi.Cover
import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.LockCover
import ai.lucidloom.limo.child.ffi.LockWhy
import ai.lucidloom.limo.child.ffi.RequestAnswer
import ai.lucidloom.limo.child.ffi.Way

/** The pictures the covers use, each an icon of the design's set (`limo_icon_<name>`). */
internal enum class Picture(val icon: String) {
  HOURGLASS("hourglass"),
  MOON("moon"),
  CALENDAR("calendar"),
  SYNC("sync"),
  AWAY("away"),
  DEVICES("devices"),
  LOCK("lock"),
  ALERT("alert"),
  BLOCK("block"),
  HAND("hand"),
  CODE("code"),
  PIN("pin"),
  BELL("bell"),
}

/** The colour of a small stamp, by what it says: ink while waiting, teal for given, coral for not this time. */
internal enum class Tone { INK, TEAL, CORAL }

/** What is pinned to the bottom of a lock or a block screen, in order from the top. */
internal enum class Pinned { BACK_TO_CALL, EMERGENCY }

/** One way out, as a row of the lock: the engine's words, and whether it must wait. */
internal data class RowPlan(val way: Way, val picture: Picture, val title: String, val sub: String, val off: Boolean)

/** One app that stays open under the lock: its package and its name as Android gives it. */
internal data class TilePlan(val packageName: String, val label: String)

/** The child's newest request, in the engine's words, with its stamp. */
internal data class RequestPlan(val text: String, val stamp: String, val tone: Tone)

/** What the lock says when it offers no way out: a line in bold and one under it. */
internal data class NothingPlan(val bold: String, val line: String)

/**
 * What a cover shows, decided from the engine's order with no Android in
 * it: which parts, in which order, with which words. Two plans that are
 * equal draw the same screen, so the shower draws again only when the plan
 * changes ([ShowerCore]).
 */
internal sealed class CoverPlan {
  /** Nothing is drawn. */
  data object None : CoverPlan()

  /** D: the block screen. */
  data class Block(
    val packageName: String,
    /** "Mira · this phone", or "this phone" with no name. */
    val eyebrow: String,
    val title: String,
    val message: String,
    /** "Back to the call" while a call is on; null otherwise. */
    val backToCall: String?,
    val pinned: List<Pinned>,
  ) : CoverPlan()

  /** A and C: the device lock. */
  data class Lock(
    val why: LockWhy,
    val picture: Picture,
    val eyebrow: String,
    val stamp: String,
    val title: String,
    val message: String,
    /** "Tomorrow, 7:00 am", under "Opens again"; null leaves the part out. */
    val untilWords: String?,
    val request: RequestPlan?,
    /** "Too many wrong tries. The next try is at 9:47 pm." */
    val waitWords: String?,
    val rows: List<RowPlan>,
    /** Only when no row is offered. */
    val nothing: NothingPlan?,
    val tiles: List<TilePlan>,
    val backToCall: String?,
    val pinned: List<Pinned>,
  ) : CoverPlan()
}

/** The apps of the phone, as far as the lock needs them: Android's on a phone, made up in tests. */
internal interface AppCatalog {
  /** The phone's own package, left out of the tiles: LiMo is open anyway. */
  val ownPackage: String

  /** The default dialler, shown first among the tiles: the phone is always usable under the lock. Null when unknown. */
  val dialler: String?

  /** A tile for [packageName] when Android can open it, with its name; null when it is not installed or cannot be opened. */
  fun tile(packageName: String): TilePlan?
}

/** The decisions of the covers' layout. */
internal object Plans {
  fun of(cover: Cover, childName: String?, apps: AppCatalog): CoverPlan = when (cover.kind) {
    CoverKind.NOTHING -> CoverPlan.None
    CoverKind.BLOCK -> cover.block?.let { block(it, childName) } ?: CoverPlan.None
    CoverKind.LOCK -> cover.lock?.let { lock(it, childName, apps) } ?: CoverPlan.None
  }

  fun block(b: BlockCover, childName: String?): CoverPlan.Block {
    val back = b.backToCall?.takeIf { b.callOn && it.isNotBlank() }?.let(Typo::quotes)
    return CoverPlan.Block(
      packageName = b.packageName,
      eyebrow = Drawn.eyebrow(childName),
      title = Typo.quotes(b.title),
      message = Typo.quotes(b.message),
      backToCall = back,
      pinned = pinned(back),
    )
  }

  fun lock(l: LockCover, childName: String?, apps: AppCatalog): CoverPlan.Lock {
    val back = l.backToCall?.takeIf { l.callOn && it.isNotBlank() }?.let(Typo::quotes)
    val rows = l.ways.map { RowPlan(it.way, picture(it.way), Typo.quotes(it.title), Typo.quotes(it.sub), it.off) }
    return CoverPlan.Lock(
      why = l.why,
      picture = picture(l.why),
      eyebrow = Drawn.eyebrow(childName),
      stamp = Typo.quotes(l.stamp),
      title = Typo.quotes(l.title),
      message = Typo.quotes(l.message),
      untilWords = l.untilWords?.takeIf { it.isNotBlank() }?.let(Typo::quotes),
      request = l.requestLine?.let { line ->
        RequestPlan(Typo.quotes(line.text), Typo.quotes(line.stamp), tone(l.latestRequest?.answer))
      },
      waitWords = l.waitWords?.takeIf { it.isNotBlank() }?.let(Typo::quotes),
      rows = rows,
      nothing = if (rows.isEmpty()) l.nothingToDo?.let { NothingPlan(Typo.quotes(it.bold), Typo.quotes(it.line)) } else null,
      tiles = tiles(l.allowedPackages, apps),
      backToCall = back,
      pinned = pinned(back),
    )
  }

  /** Back to the call above Emergency call while a call is on; Emergency call always. */
  fun pinned(backToCall: String?): List<Pinned> =
    if (backToCall != null) listOf(Pinned.BACK_TO_CALL, Pinned.EMERGENCY) else listOf(Pinned.EMERGENCY)

  /**
   * The apps that stay open: the dialler first (the phone is always usable
   * under the lock), then the family's always allowed apps in the engine's
   * order; each once, only those Android can open, LiMo itself left out.
   */
  fun tiles(allowed: List<String>, apps: AppCatalog): List<TilePlan> {
    val seen = HashSet<String>()
    val out = ArrayList<TilePlan>()
    for (p in listOfNotNull(apps.dialler) + allowed) {
      if (p.isBlank() || p == apps.ownPackage || !seen.add(p)) continue
      apps.tile(p)?.let(out::add)
    }
    return out
  }

  /** The picture for each reason: an hourglass for time used up, a moon for the hours off, a lock for a guardian's lock. */
  fun picture(why: LockWhy): Picture = when (why) {
    LockWhy.BLOCKED_HOURS -> Picture.MOON
    LockWhy.NO_TIME_TODAY -> Picture.CALENDAR
    LockWhy.TIME_USED_UP -> Picture.HOURGLASS
    LockWhy.WAITING_FOR_TIME -> Picture.SYNC
    LockWhy.AWAY_TIME_USED_UP -> Picture.AWAY
    LockWhy.HELD_BY_ANOTHER_DEVICE -> Picture.DEVICES
    LockWhy.GUARDIAN_LOCK -> Picture.LOCK
    LockWhy.CANNOT_CHECK -> Picture.ALERT
    LockWhy.APP_BLOCKED -> Picture.BLOCK
  }

  fun picture(way: Way): Picture = when (way) {
    Way.ASK -> Picture.HAND
    Way.CODE -> Picture.CODE
    Way.PIN -> Picture.PIN
  }

  fun tone(answer: RequestAnswer?): Tone = when (answer) {
    RequestAnswer.GRANTED -> Tone.TEAL
    RequestAnswer.REFUSED -> Tone.CORAL
    else -> Tone.INK
  }
}
