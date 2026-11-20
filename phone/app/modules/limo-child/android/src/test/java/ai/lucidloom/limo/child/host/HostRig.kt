package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.ALL_PERMISSIONS
import ai.lucidloom.limo.child.ffi.CallState
import ai.lucidloom.limo.child.ffi.Cover
import ai.lucidloom.limo.child.ffi.Front
import ai.lucidloom.limo.child.ffi.GAME
import ai.lucidloom.limo.child.ffi.GrantTaken
import ai.lucidloom.limo.child.ffi.IST
import ai.lucidloom.limo.child.ffi.LoadProblem
import ai.lucidloom.limo.child.ffi.PIN
import ai.lucidloom.limo.child.ffi.PhoneEngine
import ai.lucidloom.limo.child.ffi.PhoneEngineInterface
import ai.lucidloom.limo.child.ffi.PhoneFacts
import ai.lucidloom.limo.child.ffi.PhoneOrders
import ai.lucidloom.limo.child.ffi.PhoneView
import ai.lucidloom.limo.child.ffi.PinAction
import ai.lucidloom.limo.child.ffi.TimeLeft
import ai.lucidloom.limo.child.ffi.TimeRequest
import ai.lucidloom.limo.child.ffi.TryAnswer
import ai.lucidloom.limo.child.ffi.WEDNESDAY_4_PM
import ai.lucidloom.limo.child.ffi.WarningShown
import ai.lucidloom.limo.child.ffi.app
import ai.lucidloom.limo.child.ffi.miraRules
import ai.lucidloom.limo.child.ffi.testkit.TestFamily
import java.io.File
import java.io.IOException

// The host's tests run HostCore on the PC's JVM with the REAL Rust engine
// (through the bindings that ship, with the PC library loaded by JNA) and a
// made up phone around it: facts the test sets, a state file in memory, a
// clock the test moves, and the host's tick and alarm run by the test when
// they fall due. The master PC is the testkit's family, for the welcome and
// the codes.

/** One line of what happened, in order, to check that the state is kept before anything else is done. */
internal typealias Trail = MutableList<String>

/** The kept state, in memory: what was written, how often, and a way to refuse. */
internal class MemoryStore(private val trail: Trail) : StateStore {
  var bytes: ByteArray? = null
  var writes = 0
  var refuseRead: IOException? = null
  var refuseWrite: IOException? = null

  override fun exists(): Boolean = bytes != null

  override fun read(): ByteArray? {
    refuseRead?.let { throw it }
    return bytes?.copyOf()
  }

  override fun write(bytes: ByteArray) {
    refuseWrite?.let { throw it }
    this.bytes = bytes.copyOf()
    writes += 1
    trail += "save"
  }

  override fun delete() {
    bytes = null
  }
}

/** The child's name, in memory, kept through the file's own text as the app keeps it; a write can be refused. */
internal class MemoryNames : NameStore {
  var text: String? = null
  var refuseWrite: IOException? = null

  override fun read(): String? = text?.let { ChildName.parse(it) }

  override fun write(name: String) {
    refuseWrite?.let { throw it }
    text = ChildName.text(name)
  }

  override fun delete() {
    text = null
  }
}

/** Every cover and warning the host handed out. */
internal class RecordingShower(private val trail: Trail) : Shower {
  val covers = ArrayList<Cover>()
  val warnings = ArrayList<WarningShown>()

  val last: Cover get() = covers.last()

  override fun show(cover: Cover) {
    covers += cover
    trail += "show"
  }

  override fun warn(warning: WarningShown) {
    warnings += warning
  }
}

/** Everything the keeper was handed. */
internal class RecordingKeeper : KeeperDisplay {
  val times = ArrayList<TimeLeft>()
  val warnings = ArrayList<WarningShown>()
  var cleared = 0

  val last: TimeLeft get() = times.last()

  override fun timeLeft(timeLeft: TimeLeft) {
    times += timeLeft
  }

  override fun warning(warning: WarningShown) {
    warnings += warning
  }

  override fun cleared() {
    cleared += 1
  }
}

/**
 * The real engine, watched: every tick's orders, and a failure to give at
 * the next tick, as a broken engine would.
 */
internal class EngineSpy(val real: PhoneEngine, private val trail: Trail) : PhoneEngineInterface, AutoCloseable {
  val orders = ArrayList<PhoneOrders>()
  var failNextTick: Exception? = null
  var failNextWayOut: Exception? = null
  var closed = false

  override fun tick(facts: PhoneFacts): PhoneOrders {
    failNextTick?.let {
      failNextTick = null
      throw it
    }
    trail += "tick"
    return real.tick(facts).also { orders += it }
  }

  private fun wayOut() {
    failNextWayOut?.let {
      failNextWayOut = null
      throw it
    }
  }

  override fun askForTime(minutes: Int, reason: String, wallSecs: Long): TimeRequest {
    wayOut()
    return real.askForTime(minutes, reason, wallSecs)
  }
  override fun loadProblem(): LoadProblem? = real.loadProblem()
  override fun lostContact() = real.lostContact()
  override fun outgoingReport(wallSecs: Long): ByteArray? = real.outgoingReport(wallSecs)
  override fun save(): ByteArray = real.save()
  override fun saveFailed() = real.saveFailed()
  override fun shouldSave(): Boolean = real.shouldSave()
  override fun takeGrant(grant: ByteArray, wallSecs: Long): GrantTaken = real.takeGrant(grant, wallSecs)
  override fun tryCode(code: ByteArray, wallSecs: Long): TryAnswer {
    wayOut()
    return real.tryCode(code, wallSecs)
  }

  override fun tryPin(pin: ByteArray, action: PinAction, wallSecs: Long): TryAnswer {
    wayOut()
    return real.tryPin(pin, action, wallSecs)
  }
  override fun view(): PhoneView? = real.view()

  override fun close() {
    closed = true
    real.close()
  }
}

/** The engines the host makes, each one watched. */
internal class SpyEngines(private val trail: Trail) : Engines {
  val made = ArrayList<EngineSpy>()
  var loads = 0

  val current: EngineSpy get() = made.last()

  override fun load(state: ByteArray): PhoneEngineInterface {
    loads += 1
    return EngineSpy(PhoneEngine.load(state), trail).also { made += it }
  }

  override fun paired(welcome: ByteArray, codeSecret: ByteArray, wallSecs: Long): PhoneEngineInterface =
    EngineSpy(PhoneEngine.paired(welcome, codeSecret, wallSecs), trail).also { made += it }
}

/** A phone for the host, and the host's tick and alarm run by hand. */
internal class HostRig(folder: File, start: Long = WEDNESDAY_4_PM) : AutoCloseable {
  val trail: Trail = ArrayList()
  val family = TestFamily(folder.absolutePath, start)
  val child: String = family.addChild("Mira", miraRules(), start).also { family.setPin(PIN, start) }

  var wall = start
  var elapsedMs = 3_600_000L
  var front: Front = app(GAME, "Game")
  var interactive = true
  var keyguard = false
  var call = CallState.IDLE
  var permissions = ALL_PERMISSIONS

  val store = MemoryStore(trail)
  val names = MemoryNames()
  val shower = RecordingShower(trail)
  val keeper = RecordingKeeper()
  val engines = SpyEngines(trail)
  val log = ArrayList<String>()
  val events = ArrayList<HostChange>()

  /** When the host's next tick is due, on the elapsed count; null when none waits. */
  var tickDueAt: Long? = null

  /** The alarm the host set, on the elapsed count. */
  var alarmAt: Long? = null
  var alarmsFired = 0
  var ticksRun = 0
  var listening = 0
  private var onChange: (() -> Unit)? = null

  private val facts = object : FactsSource {
    override fun factsNow(): PhoneFacts = PhoneFacts(
      wallSecs = wall,
      elapsedMs = elapsedMs,
      bootCount = 7,
      utcOffsetSecs = IST,
      hours24 = false,
      autoTime = true,
      interactive = interactive,
      keyguardLocked = keyguard,
      call = call,
      front = front,
      inReach = false,
      prodded = false,
      permissions = permissions,
    )

    override fun listen(onChange: () -> Unit): AutoCloseable {
      this@HostRig.onChange = onChange
      listening += 1
      return AutoCloseable {
        this@HostRig.onChange = null
        listening -= 1
      }
    }
  }

  private val clock = object : HostClock {
    override fun elapsedMs(): Long = elapsedMs

    override fun wallSecs(): Long = wall
  }

  private val timer = object : HostTimer {
    override fun tickAfter(ms: Long) {
      tickDueAt = elapsedMs + ms
    }

    override fun cancel() {
      tickDueAt = null
    }
  }

  private val wakeups = object : Wakeups {
    override fun at(elapsedMs: Long) {
      alarmAt = elapsedMs
    }

    override fun cancel() {
      alarmAt = null
    }
  }

  var core = newCore()

  /** A host as a new process makes it, on the same kept state. */
  fun newCore(): HostCore = HostCore(store, names, facts, engines, clock, timer, wakeups, keeper, { log += it }, { events += it }).also {
    it.shower = shower
  }

  /** Pairs the phone from a welcome the family's master made, for the child called [name]; returns the phone's device id. */
  fun pair(name: String? = "Mira"): String {
    val welcome = family.welcome(child, "Phone", wall)
    check(core.pair(welcome.welcome, welcome.codeSecret, name)) { "the pairing was refused" }
    // The host filled both with zeros.
    check(welcome.codeSecret.all { it.toInt() == 0 } && welcome.welcome.all { it.toInt() == 0 })
    return welcome.deviceId
  }

  /** A fact changed: the watch tells the host, which ticks at once. */
  fun changed() {
    onChange?.invoke()
    runDue()
  }

  /** Time passes, second by second, and the host runs its tick and its alarm when they fall due. */
  fun pass(secs: Int) {
    repeat(secs) {
      wall += 1
      elapsedMs += 1_000
      runDue()
    }
  }

  /** Runs what is due now: the tick, then the alarm (whose receiver ticks too). */
  fun runDue() {
    var guard = 0
    while (true) {
      val due = tickDueAt
      val alarm = alarmAt
      when {
        due != null && due <= elapsedMs -> {
          tickDueAt = null
          ticksRun += 1
          core.tick()
        }
        alarm != null && alarm <= elapsedMs -> {
          alarmAt = null
          alarmsFired += 1
          ticksRun += 1
          core.tick()
        }
        else -> return
      }
      guard += 1
      check(guard < 100) { "the host keeps ticking without time passing" }
    }
  }

  /**
   * LiMo's process is killed: nothing more is written, the old engine is
   * gone, and a new process starts its host on what was kept.
   */
  fun kill() {
    tickDueAt = null
    onChange = null
    listening = 0
    core = newCore()
  }

  override fun close() {
    for (spy in engines.made) if (!spy.closed) spy.close()
    family.close()
  }
}
