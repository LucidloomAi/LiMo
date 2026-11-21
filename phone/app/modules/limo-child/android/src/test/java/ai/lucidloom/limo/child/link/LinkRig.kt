package ai.lucidloom.limo.child.link

import ai.lucidloom.limo.child.ffi.AppEntry
import ai.lucidloom.limo.child.ffi.PhoneEngine
import ai.lucidloom.limo.child.ffi.PhoneEngineInterface
import ai.lucidloom.limo.child.ffi.WEDNESDAY_7_AM
import ai.lucidloom.limo.child.ffi.testkit.TestFamily
import ai.lucidloom.limo.child.ffi.testkit.TestkitException
import ai.lucidloom.limo.child.host.EngineSpy
import ai.lucidloom.limo.child.host.HostRig
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException

// The link's tests run LinkCore on the PC's JVM with the REAL Rust link and
// engine, the REAL host (HostCore, on the host's own made up phone), and the
// family's PC played by the bridge's testkit: its master's end of each
// connection, over a family file, answering as the Windows service does.
// The cable and the home Wi-Fi are played too: a connection reaches the
// master while the test says the cable is in, or the PC is on the Wi-Fi.

/** The address the testkit's offers give for the PC on the home network. */
internal const val PC_ON_WIFI = "192.168.1.20"

/** The kept pairing, in memory. */
internal class MemoryPairing(private val trail: MutableList<String>) : PairingStore {
  var bytes: ByteArray? = null
  var writes = 0
  var refuseWrite: IOException? = null

  override fun read(): ByteArray? = bytes?.copyOf()

  override fun write(bytes: ByteArray) {
    refuseWrite?.let { throw it }
    this.bytes = bytes.copyOf()
    writes += 1
    trail += "pairing"
  }

  override fun delete() {
    bytes = null
  }
}

/**
 * One connection to the played PC: what the phone writes goes to the
 * master's end at once, and what the master says back waits to be read.
 * Pulling the cable (or the Wi-Fi going) cuts it: reads say the PC closed,
 * writes fail.
 */
internal class PlayedConn(private val family: TestFamily, private val clock: LinkClock, val route: Route) : LinkConn {
  private val inbox = ByteArrayOutputStream()
  var cut = false
  var closed = false

  /** The master never answers (a stranger listening on the phone's loopback). */
  var deaf = false

  override fun write(bytes: ByteArray) {
    if (cut || closed) throw IOException("the connection is gone")
    if (deaf) return
    val back = try {
      family.exchange(bytes, clock.elapsedMs(), clock.wallSecs())
    } catch (e: TestkitException) {
      // The master's end gave up: as the service closes the socket.
      cut = true
      throw IOException("the PC closed it")
    }
    inbox.write(back)
  }

  /** What the master says without being asked (a request to report). */
  fun push(bytes: ByteArray) {
    inbox.write(bytes)
  }

  override fun read(waitMs: Long): ByteArray? {
    if (cut || closed) return null
    val bytes = inbox.toByteArray()
    inbox.reset()
    return bytes
  }

  override fun close() {
    closed = true
  }
}

/** The cable and the home Wi-Fi, as a test sets them. */
internal class PlayedWires(private val family: TestFamily, private val clock: LinkClock) : LinkDialer {
  var cableIn = true
  var onWifi = false

  /** The phone's loopback answers but no PC is behind it: a stranger listening on the PC's port. */
  var strangerOnLoopback = false

  /** The PC takes connections but answers nothing: its service is stuck. */
  var pcDeaf = false
    set(value) {
      field = value
      current?.deaf = value
    }

  /** Every route the link knocked on, in order, answered or not. */
  val knocked = ArrayList<Route>()
  var current: PlayedConn? = null

  override fun dial(route: Route): LinkConn {
    knocked += route
    if (route.way == Way.CABLE && strangerOnLoopback && !cableIn) {
      return PlayedConn(family, clock, route).also { it.deaf = true }
    }
    val reaches = when (route.way) {
      Way.CABLE -> cableIn && route.address == Addresses.LOOPBACK
      Way.WIFI -> onWifi && route.address == PC_ON_WIFI
    }
    if (!reaches) throw IOException("nothing answers")
    family.connect(clock.elapsedMs(), clock.wallSecs())
    return PlayedConn(family, clock, route).also {
      it.deaf = pcDeaf
      current = it
    }
  }

  /** The cable is pulled out: the tunnel goes, and the connection through it. */
  fun pullCable() {
    cableIn = false
    current?.takeIf { it.route.way == Way.CABLE }?.cut = true
  }

  /** The Wi-Fi goes. */
  fun wifiOff() {
    onWifi = false
    current?.takeIf { it.route.way == Way.WIFI }?.cut = true
  }
}

/** The host's side, as the link sees it, over the host's test rig, on the test's one thread. */
internal class RigSide(private val rig: HostRig) : LinkHostSide {
  override val paired: Boolean get() = rig.core.paired

  override fun adopt(engine: PhoneEngineInterface, childName: String?): Boolean {
    // Watched, as every engine the rig makes is.
    val watched = EngineSpy(engine as PhoneEngine, rig.trail).also { rig.engines.made += it }
    return rig.core.adopt(watched, childName)
  }

  override fun grant(grant: ByteArray) = rig.core.takeGrant(grant)

  override fun lostContact() = rig.core.lostContact()

  override fun reachChanged() = rig.core.tick()

  override fun released() = rig.core.release()
}

/** A phone with LiMo, its host and its link, and the family's PC played beside it. */
internal class LinkRig(folder: File, start: Long = WEDNESDAY_7_AM) : AutoCloseable {
  val rig = HostRig(folder, start)
  val family: TestFamily get() = rig.family
  val pairing = MemoryPairing(rig.trail)
  val log = ArrayList<String>()

  val clock = object : LinkClock {
    override fun elapsedMs(): Long = rig.elapsedMs

    override fun wallSecs(): Long = rig.wall
  }

  val wires = PlayedWires(rig.family, clock)

  /** The phone's apps, as the watch would list them. */
  var apps = listOf(AppEntry("org.example.game", "Game"), AppEntry("org.example.maps", "Maps"))

  val core = LinkCore(
    host = RigSide(rig),
    store = pairing,
    dialer = wires,
    clock = clock,
    finder = NoFinder,
    apps = { apps },
    self = { PhoneSelf("Galaxy S20 Ultra", 33) },
    log = { log += it },
  )

  init {
    rig.link = core
    rig.core = rig.newCore()
  }

  /** The link's rounds for this moment: until it would wait, at most a few. */
  fun steps() {
    repeat(6) {
      if (core.step() > 0) return
    }
  }

  /** Time passes, second by second: the host ticks when due, and the link takes its rounds. */
  fun pass(secs: Int) {
    repeat(secs) {
      rig.pass(1)
      steps()
    }
  }

  /** Where the link's log stands now, for [passUntil]. */
  fun mark(): Int = log.size

  /** Passes until the link has said this since [from] (by default, ever), or fails. */
  fun passUntil(line: String, most: Int = 120, from: Int = 0) {
    repeat(most) {
      if (log.drop(from).contains(line)) return
      pass(1)
    }
    check(log.drop(from).contains(line)) { "never: $line; the link said ${log.drop(from)}" }
  }

  /** Pairs the phone with an offer from the PC, over the cable, and finishes setup on it. Returns its device id. */
  fun pairOverTheCable(finished: Boolean = true): String {
    val offer = family.makeOffer(rig.child, rig.wall)
    val from = mark()
    check(core.takeOffer(offer) == OfferAnswer.TAKEN)
    passUntil("link paired", from = from)
    if (finished) check(rig.core.finishSetup() is ai.lucidloom.limo.child.host.WayOut.Done)
    return deviceId()
  }

  /** The phone's id at the master, from its engine. */
  fun deviceId(): String {
    val view = rig.engines.current.real.view() ?: error("not paired")
    return view.deviceId
  }

  /** The master asks the phone to report now, if the family's file wants to hear from it. */
  fun prod() {
    val bytes = family.prod(rig.elapsedMs, rig.wall)
    if (bytes.isNotEmpty()) wires.current?.push(bytes)
  }

  override fun close() {
    rig.close()
  }
}
