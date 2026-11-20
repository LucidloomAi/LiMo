package ai.lucidloom.limo.child.ffi

import ai.lucidloom.limo.child.ffi.testkit.TestFamily
import ai.lucidloom.limo.child.ffi.testkit.TestRules
import java.io.File

// The JVM tests drive the real Rust engine and the real link from Kotlin,
// through the bindings that ship in the APK (package
// ai.lucidloom.limo.child.ffi), with the library built for this PC loaded
// by JNA. Only TestFamily, which plays the master PC, comes from the test
// copy of the bindings (package ...ffi.testkit): the library on a phone
// never has it.

/** India, the home zone. */
const val IST = 19_800

/** Wednesday 18 November 2026, 16:00 in India, unix seconds. */
const val WEDNESDAY_4_PM = 1_794_997_800L

/** Wednesday 18 November 2026, 07:00 in India, unix seconds. */
const val WEDNESDAY_7_AM = 1_794_965_400L

const val GAME = "org.example.game"
const val CLOCK = "org.example.clock"
const val CASINO = "org.example.casino"

/** The guardian's PIN. */
const val PIN = "482915"

/** Mira's rules: 30 minutes a day, the casino blocked, the clock always allowed, the whole day for away. */
fun miraRules() = TestRules(
  minutesADay = 30,
  blockedApps = listOf(CASINO),
  allowedApps = listOf(CLOCK),
  awaySharePercent = 100,
  utcOffsetSecs = IST,
)

/** Every permission LiMo needs, given, in setup order. */
val ALL_PERMISSIONS = PhonePermissions(
  notifications = true,
  phone = true,
  usageAccess = true,
  accessibility = true,
  battery = true,
  protection = true,
)

fun app(packageName: String, label: String) = Front(FrontKind.APP, packageName, label)

fun screen(kind: FrontKind) = Front(kind)

/**
 * A phone as the host will drive it: its own clocks and facts, and a state
 * file that holds only what the engine asked to keep. Paired from a made up
 * welcome: the welcome and the code secret the master PC makes, played here
 * by the testkit's family.
 */
class FakePhone(folder: File, start: Long) : AutoCloseable {
  val family = TestFamily(folder.absolutePath, start)
  val child: String = family.addChild("Mira", miraRules(), start)
  val deviceId: String
  var engine: PhoneEngine
  var kept: ByteArray
  var wall = start
  var elapsedMs = 3_600_000L
  var front = app(GAME, "Game")
  var inReach = false
  var permissions = ALL_PERMISSIONS
  val notes = mutableListOf<String>()

  init {
    family.setPin(PIN, start)
    val welcome = family.welcome(child, "Phone", start)
    deviceId = welcome.deviceId
    engine = PhoneEngine.paired(welcome.welcome, welcome.codeSecret, start)
    // The code secret is the engine's now: no copy is left lying about.
    welcome.codeSecret.fill(0)
    check(engine.shouldSave())
    kept = engine.save()
  }

  fun facts() = PhoneFacts(
    wallSecs = wall,
    elapsedMs = elapsedMs,
    bootCount = 7,
    utcOffsetSecs = IST,
    hours24 = false,
    autoTime = true,
    interactive = true,
    keyguardLocked = false,
    call = CallState.IDLE,
    front = front,
    inReach = inReach,
    prodded = false,
    permissions = permissions,
  )

  /** One call, carried out as the host does: keep the state when told, and carry a report and its grant. */
  fun tick(): PhoneOrders {
    var orders = engine.tick(facts())
    carryOut(orders)
    if (orders.sendReport) {
      val report = engine.outgoingReport(wall)!!
      kept = engine.save()
      val grant = family.answer(report, wall)
      engine.takeGrant(grant, wall)
      kept = engine.save()
      orders = engine.tick(facts())
      carryOut(orders)
    }
    return orders
  }

  private fun carryOut(orders: PhoneOrders) {
    if (orders.saveState) kept = engine.save()
    notes += orders.notes
  }

  /** Time passes by [secs], with a call every second, as while the screen is in use. */
  fun run(secs: Int): PhoneOrders {
    var orders = tick()
    repeat(secs) {
      wall += 1
      elapsedMs += 1_000
      orders = tick()
    }
    return orders
  }

  /** Counted use on the phone today, in seconds. */
  fun counted(): Long = engine.view()!!.countedSecs

  override fun close() {
    engine.close()
    family.close()
  }
}
