package ai.lucidloom.limo.child.link

import ai.lucidloom.limo.child.ffi.AppEntry
import ai.lucidloom.limo.child.ffi.EngineException
import ai.lucidloom.limo.child.ffi.LinkEvent
import ai.lucidloom.limo.child.ffi.LinkEventKind
import ai.lucidloom.limo.child.ffi.LinkException
import ai.lucidloom.limo.child.ffi.Offer
import ai.lucidloom.limo.child.ffi.PairRequest
import ai.lucidloom.limo.child.ffi.PairedMaster
import ai.lucidloom.limo.child.ffi.PhoneEngineInterface
import ai.lucidloom.limo.child.ffi.PhonePairing
import ai.lucidloom.limo.child.ffi.PhoneSession
import ai.lucidloom.limo.child.ffi.appsHash
import java.io.IOException
import java.util.ArrayDeque
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

// The phone's half of the link to the family's PC with no Android in it, so
// the JVM tests drive it against a master played by the bridge's testkit.
// What it needs from Android is behind the small interfaces below.

/** Which way a connection to the family's PC goes: through the USB cable, or over the home Wi-Fi. */
enum class Way(val word: String) {
  CABLE("cable"),
  WIFI("wifi"),
}

/**
 * Where to knock: an address and a port, and which way it goes. Only ever
 * the phone's own loopback (the cable's tunnel to the PC) or an address of
 * a home network ([Addresses.isHome]), on the port the PC gave.
 */
data class Route(val way: Way, val address: String, val port: Int)

/** One connection to the family's PC. */
interface LinkConn : AutoCloseable {
  /** Writes all of [bytes]. Throws [IOException] when it cannot. */
  fun write(bytes: ByteArray)

  /**
   * Waits at most [waitMs] for bytes: what came; empty when nothing came in
   * time; null once the PC closed the connection. Throws [IOException] when
   * the connection broke.
   */
  fun read(waitMs: Long): ByteArray?

  /**
   * Closes the connection both ways at once. Never one way alone: the
   * cable's tunnel (adb's streams) has no half close, and bytes still on
   * their way would be lost.
   */
  override fun close()
}

/** Opens connections. */
fun interface LinkDialer {
  /** A connection to [route]; throws [IOException] when nothing answers there. */
  fun dial(route: Route): LinkConn
}

/** The kept pairing: the app's private file `files/limo/paired`, which holds the phone's private key. */
interface PairingStore {
  /** The kept bytes, or null when none are kept. Throws [IOException] when they cannot be read. */
  fun read(): ByteArray?

  /** Keeps [bytes] in place of what was kept, all or nothing. Throws [IOException] when it cannot. */
  fun write(bytes: ByteArray)

  fun delete()
}

/** The phone's two clocks. */
interface LinkClock {
  /** `SystemClock.elapsedRealtime()`. */
  fun elapsedMs(): Long

  /** The wall clock, unix seconds. */
  fun wallSecs(): Long
}

/** Finds the master on the home network by its announcement (Android's NsdManager). */
interface MasterFinder {
  /** Starts looking for the master with this id (16 hex characters), unless it is looking already. */
  fun look(masterId: String)

  /** The addresses found for that master so far, newest first. */
  fun found(masterId: String): List<String>
}

/** No discovery: only the cable and the addresses the pairing gave. */
object NoFinder : MasterFinder {
  override fun look(masterId: String) = Unit

  override fun found(masterId: String): List<String> = emptyList()
}

/** The apps a child can open, with their labels: the list the master keeps for the guardian to pick from. */
fun interface AppsSource {
  fun apps(): List<AppEntry>
}

/** What the phone says of itself when it pairs: the name the family will see, and Android's API level. */
data class PhoneSelf(val name: String, val androidVersion: Int)

/** Where the link's log lines go (tag `LiMo`): never an offer, a key, a code, a name or an address. */
fun interface LinkLog {
  fun line(text: String)
}

/** The engine's host, as the link sees it. Every call may be made from the link's thread. */
interface LinkHostSide {
  /** A state is kept: the phone is paired. */
  val paired: Boolean

  /**
   * Keeps a new engine, made by pairing, and the child's name the master
   * gave: its state first, before anything else. True once kept; on false
   * the engine was closed. Returns once done.
   */
  fun adopt(engine: PhoneEngineInterface, childName: String?): Boolean

  /** The master's answer to a report: the core's grant as JSON bytes. */
  fun grant(grant: ByteArray)

  /** A report got no answer: the master could not be reached. */
  fun lostContact()

  /** The master came in or out of reach, or asked for a report: tick soon. */
  fun reachChanged()

  /** The family's PC removed this phone and said so: let go of its limits and its protection. */
  fun released()
}

/** The link, as the engine's host sees it. Any thread. */
interface LinkPort {
  /** A session with the master is open now: the engine is in reach of it. */
  val inReach: Boolean

  /** True once since the master asked for a report, or a session opened; reading it clears it. */
  fun takeProdded(): Boolean

  /** True when a report may be written now: a session is open and the phone has not called on the PC often this minute. */
  fun mayReport(): Boolean

  /** Sends the engine's report with the open session. False when it cannot: the host then tells the engine contact was lost. */
  fun send(report: ByteArray): Boolean
}

/** No link: the master is never in reach. */
object NoLink : LinkPort {
  override val inReach: Boolean get() = false

  override fun takeProdded(): Boolean = false

  override fun mayReport(): Boolean = false

  override fun send(report: ByteArray): Boolean = false
}

/** What the cable door's offer came to. */
enum class OfferAnswer(val word: String) {
  /** Taken: the phone pairs with it. */
  TAKEN("taken"),

  /** The phone is paired already: no PC may pair it again over the cable. */
  PAIRED("paired"),

  /** An offer is held already, and is still being paired with. */
  BUSY("busy"),

  /** Nothing came. */
  EMPTY("empty"),

  /** More than an offer can be. */
  TOO_LONG("too_long"),

  /** Not an offer. */
  NOT_AN_OFFER("not_an_offer"),
}

/** The addresses the link ever connects to. */
object Addresses {
  /** The phone's own loopback, where the cable's tunnel (`adb reverse`) arrives. */
  const val LOOPBACK = "127.0.0.1"

  /** The four bytes of a dotted IPv4 address, or null for anything else. */
  fun parse(dotted: String): ByteArray? {
    val parts = dotted.split('.')
    if (parts.size != 4) return null
    val bytes = ByteArray(4)
    for ((i, part) in parts.withIndex()) {
      if (part.isEmpty() || part.length > 3 || !part.all { it in '0'..'9' }) return null
      val n = part.toInt()
      if (n > 255) return null
      bytes[i] = n.toByte()
    }
    return bytes
  }

  /**
   * True for an IPv4 address in the private ranges home routers give out:
   * 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16. Never the loopback, a self
   * assigned address, a carrier's shared range or a public address.
   */
  fun isHome(dotted: String): Boolean {
    val b = parse(dotted)?.map { it.toInt() and 0xff } ?: return false
    return b[0] == 10 || (b[0] == 172 && b[1] in 16..31) || (b[0] == 192 && b[1] == 168)
  }
}

/**
 * The phone's half of the link to the family's PC: pairing once from the
 * offer the cable door took, then a session whenever the PC is in reach,
 * carrying the engine's reports and the master's grants, the app list, and
 * the PC's word that it removed the phone. The engine stays the judge: out
 * of reach it keeps to its last grant and the away share, as the core says.
 *
 * Driven by [step], on one thread only (the link's own on the phone, the
 * test's in the JVM tests); [takeOffer], [poke] and the [LinkPort] calls may
 * come from any thread.
 *
 * **Finding the master**, in this order at each try: the cable (the phone's
 * own loopback, where `adb reverse` brings the PC's port while the cable is
 * in); the home address that worked last; the addresses the announcement on
 * the home network gave ([MasterFinder]); the addresses the pairing offer
 * gave. A route whose handshake failed is tried last the next time, so
 * nothing listening on the phone's loopback can stand in the way.
 *
 * **Never too often.** One connection at a time; after a try that reached
 * nothing it waits 5 seconds, doubling to a minute (a refusal it should not
 * argue with: ten minutes); at most [MAX_CALLS_A_MINUTE] calls on the PC
 * (sessions opened, reports, app lists) in any minute.
 */
class LinkCore(
  private val host: LinkHostSide,
  private val store: PairingStore,
  private val dialer: LinkDialer,
  private val clock: LinkClock,
  private val finder: MasterFinder,
  private val apps: AppsSource,
  private val self: () -> PhoneSelf,
  private val log: LinkLog,
) : LinkPort {
  private class Waiting(val offer: Offer, val takenAt: Long) {
    var nextTryAt = 0L
  }

  private class Session(val conn: LinkConn, val session: PhoneSession, val route: Route) {
    var opened = false
    var appsCheckedAt = Long.MIN_VALUE
  }

  private sealed class Paired {
    data object Done : Paired()

    class Refused(val why: String) : Paired()

    data object NoAnswer : Paired()
  }

  @Volatile private var open = false
  private val prodded = AtomicBoolean(false)
  private val reports = ConcurrentLinkedQueue<ByteArray>()
  private val waiting = AtomicReference<Waiting?>(null)
  private val signal = LinkedBlockingQueue<Unit>()
  @Volatile private var poked = false

  // The link's thread only.
  private var master: PairedMaster? = null
  private var masterRead = false
  private var session: Session? = null
  private var nextTryAt = 0L
  private var backoffMs = FIRST_WAIT_MS
  private var lastGood: Route? = null
  private var lastFailed: Route? = null
  private var appsSent: String? = null
  private val calls = ArrayDeque<Long>()
  private val failedAt = HashMap<String, Long>()

  override val inReach: Boolean get() = open

  override fun takeProdded(): Boolean = prodded.getAndSet(false)

  override fun mayReport(): Boolean = open && roomForCall(clock.elapsedMs())

  override fun send(report: ByteArray): Boolean {
    if (!open) return false
    reports.add(report)
    wake()
    return true
  }

  /** Something changed that may bring the master in reach (Wi-Fi came, a cable): try again soon. Any thread. */
  fun poke() {
    poked = true
    wake()
  }

  /** True while an offer is held for pairing. Any thread. */
  val offerHeld: Boolean
    get() = waiting.get()?.let { clock.elapsedMs() - it.takenAt < OFFER_HELD_MS } ?: false

  /**
   * An offer the cable door took, as the PC wrote it: at most
   * [MAX_OFFER_BYTES] of `limo1:` text. Refused while the phone is paired
   * (no PC may pair it again over the cable) and while another offer is
   * held. [text] is filled with zeros, whatever happens. Any thread.
   */
  fun takeOffer(text: ByteArray?): OfferAnswer {
    val answer = try {
      when {
        text == null -> OfferAnswer.TOO_LONG
        text.isEmpty() -> OfferAnswer.EMPTY
        text.size > MAX_OFFER_BYTES -> OfferAnswer.TOO_LONG
        host.paired -> OfferAnswer.PAIRED
        offerHeld -> OfferAnswer.BUSY
        else -> hold(text)
      }
    } catch (e: Exception) {
      failed("offer", e)
      OfferAnswer.NOT_AN_OFFER
    } finally {
      text?.fill(0)
    }
    // Never the offer itself, nor anything of it.
    log.line(if (answer == OfferAnswer.TAKEN) "cable offer taken" else "cable offer refused why=${answer.word}")
    return answer
  }

  private fun hold(text: ByteArray): OfferAnswer {
    val offer = try {
      // The bridge wipes its own copy.
      Offer.fromText(text.copyOf())
    } catch (e: LinkException) {
      return OfferAnswer.NOT_AN_OFFER
    }
    waiting.getAndSet(Waiting(offer, clock.elapsedMs()))?.offer?.close()
    wake()
    return OfferAnswer.TAKEN
  }

  /**
   * One round of the link's work: pairing with a held offer, or knocking on
   * the PC, or one read and the answers on an open session. Returns how
   * long the caller may wait before the next round (it waits less when
   * anything wakes it, see [waitForWork]).
   */
  fun step(): Long {
    val now = clock.elapsedMs()
    waiting.get()?.let { return pairingStep(it, now) }
    if (!host.paired) {
      // Not paired, or forgotten: no session, and a pairing kept with no
      // state beside it (a pairing cut short, a forgotten phone) is gone.
      session?.let { end(it, now, error = null) }
      if (master != null || !masterRead) {
        master?.close()
        master = null
        safely("pairing_delete", Unit) { store.delete() }
      }
      masterRead = true
      return IDLE_MS
    }
    val master = master() ?: return IDLE_MS
    val open = session
    return if (open == null) knock(master, now) else talk(open)
  }

  /** Waits at most [ms] for something to do. The link's thread. */
  fun waitForWork(ms: Long) {
    if (ms <= 0) return
    signal.poll(ms, TimeUnit.MILLISECONDS)
    signal.clear()
  }

  private fun wake() {
    signal.offer(Unit)
  }

  /** The kept pairing, read once; null when this phone was paired another way (the development tool). */
  private fun master(): PairedMaster? {
    master?.let { return it }
    if (masterRead) return null
    masterRead = true
    val bytes = safely("pairing_read", null) { store.read() } ?: return null
    master = try {
      PairedMaster.fromBytes(bytes)
    } catch (e: LinkException) {
      failed("pairing_read", e)
      null
    } finally {
      bytes.fill(0)
    }
    return master
  }

  // ---------------------------------------------------------------- pairing

  private fun pairingStep(w: Waiting, now: Long): Long {
    if (host.paired || now - w.takenAt >= OFFER_HELD_MS) {
      log.line(if (host.paired) "link offer dropped why=paired" else "link offer dropped why=too_old")
      dropOffer(w)
      return 0
    }
    if (now < w.nextTryAt) return w.nextTryAt - now
    // The cable alone at first: the PC sets its tunnel up a moment after
    // it writes the offer. Then the offer's addresses on the home Wi-Fi too.
    val routes = ArrayList<Route>()
    routes += Route(Way.CABLE, Addresses.LOOPBACK, w.offer.port())
    if (now - w.takenAt >= CABLE_FIRST_MS) {
      for (address in w.offer.addresses().filter(Addresses::isHome)) routes += Route(Way.WIFI, address, w.offer.port())
    }
    for (route in routes) {
      when (val outcome = pairOver(route, w.offer)) {
        Paired.Done -> {
          dropOffer(w)
          return 0
        }
        is Paired.Refused -> {
          log.line("link pair_refused why=${outcome.why}")
          dropOffer(w)
          return 0
        }
        Paired.NoAnswer -> continue
      }
    }
    w.nextTryAt = clock.elapsedMs() + PAIR_AGAIN_MS
    return PAIR_AGAIN_MS
  }

  private fun dropOffer(w: Waiting) {
    if (waiting.compareAndSet(w, null)) w.offer.close()
  }

  private fun pairOver(route: Route, offer: Offer): Paired {
    val conn = try {
      dialer.dial(route)
    } catch (e: IOException) {
      return Paired.NoAnswer
    } catch (e: SecurityException) {
      failed("dial", e)
      return Paired.NoAnswer
    }
    val me = self()
    val pairing = try {
      PhonePairing.start(offer, PairRequest(me.name, "android", me.androidVersion), clock.elapsedMs())
    } catch (e: LinkException) {
      conn.close()
      failed("pair_start", e)
      return Paired.Refused("start")
    }
    try {
      conn.write(pairing.takeOutgoing())
      while (true) {
        val got = conn.read(READ_WAIT_MS) ?: return Paired.NoAnswer
        val at = clock.elapsedMs()
        if (got.isNotEmpty() && pairing.receive(got, at)) {
          // The goodbye, then the connection closed both ways.
          conn.write(pairing.takeOutgoing())
          return keep(pairing)
        }
        val out = pairing.takeOutgoing()
        if (out.isNotEmpty()) conn.write(out)
        pairing.tick(at)
      }
    } catch (e: LinkException) {
      refusal(e)?.let { return Paired.Refused(it) }
      failed("pair", e)
      return Paired.NoAnswer
    } catch (e: IOException) {
      return Paired.NoAnswer
    } finally {
      conn.close()
      pairing.close()
    }
  }

  /** What a welcomed pairing leaves: the kept pairing and the engine's state, both kept before anything else. */
  private fun keep(pairing: PhonePairing): Paired {
    val bytes = try {
      pairing.pairedMaster()
    } catch (e: LinkException) {
      failed("pairing", e)
      return Paired.Refused("not_kept")
    }
    val made = try {
      store.write(bytes)
      PairedMaster.fromBytes(bytes.copyOf())
    } catch (e: IOException) {
      failed("pairing_write", e)
      return Paired.Refused("not_kept")
    } catch (e: LinkException) {
      failed("pairing_read", e)
      safely("pairing_delete", Unit) { store.delete() }
      return Paired.Refused("not_kept")
    } finally {
      bytes.fill(0)
    }
    val engine = try {
      pairing.makeEngine(clock.wallSecs())
    } catch (e: EngineException) {
      failed("engine", e)
      made.close()
      safely("pairing_delete", Unit) { store.delete() }
      return Paired.Refused("not_kept")
    }
    val name = safely("child_name", null) { pairing.childName() }
    if (!host.adopt(engine, name)) {
      made.close()
      safely("pairing_delete", Unit) { store.delete() }
      return Paired.Refused("not_kept")
    }
    master?.close()
    master = made
    masterRead = true
    appsSent = null
    lastGood = null
    lastFailed = null
    nextTryAt = 0
    backoffMs = FIRST_WAIT_MS
    log.line("link paired")
    return Paired.Done
  }

  // ---------------------------------------------------------------- sessions

  private fun routes(master: PairedMaster): List<Route> {
    val port = master.port()
    val all = ArrayList<Route>()
    all += Route(Way.CABLE, Addresses.LOOPBACK, port)
    lastGood?.takeIf { it.way == Way.WIFI }?.let { all += it }
    val found = safely("found", emptyList<String>()) { finder.found(master.masterId()) }
    for (address in found + master.addresses()) {
      if (Addresses.isHome(address)) all += Route(Way.WIFI, address, port)
    }
    val distinct = all.distinct()
    val failed = lastFailed ?: return distinct
    return distinct.filter { it != failed } + distinct.filter { it == failed }
  }

  private fun knock(master: PairedMaster, now: Long): Long {
    if (now < nextTryAt && !poked) return nextTryAt - now
    poked = false
    if (!roomForCall(now)) {
      nextTryAt = now + FIRST_WAIT_MS
      return FIRST_WAIT_MS
    }
    for (route in routes(master)) {
      val conn = try {
        dialer.dial(route)
      } catch (e: IOException) {
        continue
      } catch (e: SecurityException) {
        failed("dial", e)
        continue
      }
      val started = try {
        PhoneSession.start(master, clock.elapsedMs())
      } catch (e: LinkException) {
        conn.close()
        failed("session_start", e)
        break
      }
      val s = Session(conn, started, route)
      try {
        conn.write(started.takeOutgoing())
      } catch (e: IOException) {
        conn.close()
        started.close()
        continue
      }
      session = s
      countCall(now)
      return 0
    }
    safely("look", Unit) { finder.look(master.masterId()) }
    nextTryAt = now + backoffMs
    val waited = backoffMs
    backoffMs = (backoffMs * 2).coerceAtMost(MAX_WAIT_MS)
    return waited
  }

  private fun talk(s: Session): Long {
    try {
      while (true) {
        val report = reports.poll() ?: break
        s.session.sendReport(report, clock.elapsedMs())
        countCall(clock.elapsedMs())
      }
      flush(s)
      val got = s.conn.read(READ_WAIT_MS)
      val at = clock.elapsedMs()
      if (got == null) {
        end(s, at, error = null)
        return 0
      }
      if (got.isNotEmpty()) {
        for (event in s.session.receive(got, at)) hear(s, event, at)
      }
      if (session !== s) return 0
      if (s.opened && at - s.appsCheckedAt >= APPS_EVERY_MS) sendApps(s, at)
      s.session.tick(at)
      flush(s)
    } catch (e: LinkException) {
      end(s, clock.elapsedMs(), e)
    } catch (e: IOException) {
      end(s, clock.elapsedMs(), error = null)
    }
    return 0
  }

  private fun flush(s: Session) {
    val out = s.session.takeOutgoing()
    if (out.isNotEmpty()) s.conn.write(out)
  }

  private fun hear(s: Session, event: LinkEvent, at: Long) {
    when (event.kind) {
      LinkEventKind.OPENED -> {
        s.opened = true
        open = true
        lastGood = s.route
        lastFailed = null
        backoffMs = FIRST_WAIT_MS
        log.line("link open via=${s.route.way.word}")
        // A report at once, as the PC expects when a session opens.
        prodded.set(true)
        sendApps(s, at)
        host.reachChanged()
      }
      LinkEventKind.GRANT -> event.grant?.let { host.grant(it) }
      LinkEventKind.REPORT_NOW -> {
        prodded.set(true)
        host.reachChanged()
      }
      LinkEventKind.PONG -> Unit
    }
  }

  /** The app list, when it changed since it was last sent in this process. */
  private fun sendApps(s: Session, at: Long) {
    s.appsCheckedAt = at
    val list = safely("apps", null) { apps.apps().take(MAX_APPS) } ?: return
    val hash = appsHash(list)
    if (hash == appsSent || !roomForCall(at)) return
    s.session.sendApps(list, at)
    countCall(at)
    appsSent = hash
    log.line("link apps sent count=${list.size}")
  }

  /** The session is over: closed, and why. A report waiting is a contact lost. */
  private fun end(s: Session, now: Long, error: LinkException?) {
    val reportWaiting = safely("report_waiting", false) { s.session.reportWaiting() }
    s.conn.close()
    s.session.close()
    session = null
    val wasOpen = s.opened
    open = false
    prodded.set(false)
    var handed = false
    while (reports.poll() != null) handed = true
    if (reportWaiting || handed || error is LinkException.NoAnswerToReport) host.lostContact()
    if (error is LinkException.RefusedRemoved) return release()
    val refused = error?.let { refusal(it) }
    if (refused != null) {
      log.line("link refused why=$refused")
      nextTryAt = now + REFUSED_WAIT_MS
    } else {
      if (!wasOpen) lastFailed = s.route
      nextTryAt = now + backoffMs
    }
    if (wasOpen) {
      log.line("link closed")
      host.reachChanged()
    }
  }

  /** The PC removed this phone: what was kept for it goes, and the host lets go. */
  private fun release() {
    log.line("link removed releasing")
    master?.close()
    master = null
    masterRead = true
    safely("pairing_delete", Unit) { store.delete() }
    host.released()
  }

  /** The word for a refusal from the master, or null for any other error. */
  private fun refusal(e: LinkException): String? = when (e) {
    is LinkException.RefusedRemoved -> "removed"
    is LinkException.RefusedUnknown -> "unknown"
    is LinkException.RefusedOtherDevice -> "other_device"
    is LinkException.RefusedVersion -> "version"
    is LinkException.RefusedOfferUsed -> "offer_used"
    is LinkException.RefusedOfferExpired -> "offer_expired"
    is LinkException.RefusedNotSupported -> "not_supported"
    else -> null
  }

  private fun roomForCall(now: Long): Boolean = synchronized(calls) {
    while (calls.isNotEmpty() && now - calls.first() >= MINUTE_MS) calls.removeFirst()
    calls.size < MAX_CALLS_A_MINUTE
  }

  private fun countCall(now: Long) {
    synchronized(calls) { calls.addLast(now) }
  }

  private inline fun <T> safely(what: String, fallback: T, block: () -> T): T {
    try {
      return block()
    } catch (e: Exception) {
      failed(what, e)
    }
    return fallback
  }

  /** One log line a minute at most for each kind of failure: what failed and the kind of error, never its words. */
  private fun failed(what: String, e: Throwable) {
    val key = "$what ${e.javaClass.simpleName}"
    val now = clock.elapsedMs()
    synchronized(failedAt) {
      val last = failedAt[key]
      if (last != null && now - last < MINUTE_MS) return
      failedAt[key] = now
    }
    log.line("link failed what=$what error=${e.javaClass.simpleName}")
  }

  companion object {
    /** The most bytes an offer's text may have, as the cable writes it. */
    const val MAX_OFFER_BYTES = 1_024

    /** How long an offer is held for pairing: its life on the PC, ten minutes. */
    const val OFFER_HELD_MS = 600_000L

    /** For this long after an offer came, it is paired over the cable only. */
    const val CABLE_FIRST_MS = 10_000L

    /** How soon a pairing that reached nothing is tried again. */
    const val PAIR_AGAIN_MS = 1_000L

    /** The longest one read waits on an open session. */
    const val READ_WAIT_MS = 500L

    /** The wait after a try that reached nothing: first, and at most. */
    const val FIRST_WAIT_MS = 5_000L
    const val MAX_WAIT_MS = 60_000L

    /** After a refusal the PC will say again (unknown, a version it does not speak): ten minutes. */
    const val REFUSED_WAIT_MS = 600_000L

    /** The wait while nothing is to be done (not paired). */
    const val IDLE_MS = 60_000L

    /** The most calls on the PC in any minute; the PC cuts a phone off past 60. */
    const val MAX_CALLS_A_MINUTE = 40

    /** How often an open session checks whether the app list changed. */
    const val APPS_EVERY_MS = 600_000L

    /** The most apps a list may hold (the link's own limit). */
    const val MAX_APPS = 2_000

    private const val MINUTE_MS = 60_000L
  }
}
