package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.ffi.ALL_PERMISSIONS
import ai.lucidloom.limo.child.ffi.CLOCK
import ai.lucidloom.limo.child.ffi.CallState
import ai.lucidloom.limo.child.ffi.Front
import ai.lucidloom.limo.child.ffi.GAME
import ai.lucidloom.limo.child.ffi.IST
import ai.lucidloom.limo.child.ffi.PIN
import ai.lucidloom.limo.child.ffi.PhoneEngine
import ai.lucidloom.limo.child.ffi.PhoneFacts
import ai.lucidloom.limo.child.ffi.PhoneOrders
import ai.lucidloom.limo.child.ffi.PhonePermissions
import ai.lucidloom.limo.child.ffi.app
import ai.lucidloom.limo.child.ffi.miraRules
import ai.lucidloom.limo.child.ffi.testkit.TestFamily
import ai.lucidloom.limo.child.ffi.testkit.TestRules
import ai.lucidloom.limo.child.host.HostView
import ai.lucidloom.limo.child.watch.LimoPermissions
import java.io.File

// The seam's tests turn the REAL engine's view, through the bindings that
// ship (the PC library loaded by JNA), into the seam's answers. The master
// PC is the testkit's family, as in the other tests; its rules are changed
// in the welcome where the testkit has no way to set them (a day with no
// limit, hours off), as a guardian would change them on the PC.

/** An app with no name under its icon: installed without one, or not installed. */
const val NAMELESS = "org.example.nameless"

/** The apps' own names, as the watch gives them: the casino and the nameless app have none. */
val NAMES = mapOf(GAME to "Game", CLOCK to "Clock")

/** The rules as the welcome carries them, changed: see [SeamPhoneRig]. */
typealias RulesEdit = (MutableMap<String, Any?>) -> Unit

/** No limit on any day. */
val NO_LIMIT: RulesEdit = { rules ->
  rules["budgets"] = DAYS.associateWith { "no_limit" }
}

/** Screens off from 21:00 to 07:00 every day. */
val BEDTIME: RulesEdit = { rules ->
  rules["blocked_hours"] = listOf(mapOf("days" to DAYS, "start_minute" to 1260L, "end_minute" to 420L))
}

val DAYS = listOf("monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday")

/**
 * A paired phone driven as the host drives it, with Mira's rules (30
 * minutes, the casino blocked, the clock always allowed) unless [edit]
 * changes them, and its engine's view as the seam gets it.
 */
class SeamPhoneRig(folder: File, start: Long, rules: TestRules = miraRules(), edit: RulesEdit? = null) : AutoCloseable {
  val family = TestFamily(folder.absolutePath, start)
  val child: String = family.addChild("Mira", rules, start)
  val deviceId: String
  val engine: PhoneEngine
  var wall = start
  var elapsedMs = 3_600_000L
  var front: Front = app(GAME, "Game")
  var permissions: PhonePermissions = ALL_PERMISSIONS
  var hours24 = false

  init {
    family.setPin(PIN, start)
    val made = family.welcome(child, "Phone", start)
    deviceId = made.deviceId
    val welcome = if (edit == null) made.welcome else editedWelcome(made.welcome, edit)
    engine = PhoneEngine.paired(welcome, made.codeSecret, start)
    made.codeSecret.fill(0)
  }

  fun facts() = PhoneFacts(
    wallSecs = wall,
    elapsedMs = elapsedMs,
    bootCount = 7,
    utcOffsetSecs = IST,
    hours24 = hours24,
    autoTime = true,
    interactive = true,
    keyguardLocked = false,
    call = CallState.IDLE,
    front = front,
    inReach = false,
    prodded = false,
    permissions = permissions,
  )

  fun tick(): PhoneOrders = engine.tick(facts())

  /** [secs] seconds pass with [front] in front, a call every second. */
  fun use(front: Front, secs: Int): PhoneOrders {
    this.front = front
    var orders = tick()
    repeat(secs) {
      wall += 1
      elapsedMs += 1_000
      orders = tick()
    }
    return orders
  }

  /** The view as the host hands it to the seam. */
  fun hostView() = HostView(paired = true, json = engine.view()?.json, problem = engine.loadProblem()?.kind)

  /** The engine's own `today` and `rules_words`, to compare the seam's answers with. */
  @Suppress("UNCHECKED_CAST")
  fun raw(part: String): Map<String, Any?> = (SeamJson.read(engine.view()!!.json) as Map<String, Any?>)[part] as Map<String, Any?>

  override fun close() {
    engine.close()
    family.close()
  }

  private fun editedWelcome(welcome: ByteArray, edit: RulesEdit): ByteArray {
    @Suppress("UNCHECKED_CAST")
    val grant = (SeamJson.read(String(welcome, Charsets.UTF_8)) as Map<String, Any?>).toMutableMap()
    @Suppress("UNCHECKED_CAST")
    val rules = (grant["rules"] as Map<String, Any?>).toMutableMap()
    edit(rules)
    grant["rules"] = rules
    return TestJson.write(grant).toByteArray(Charsets.UTF_8)
  }
}

/** The host as the seam sees it, playing a rig's engine, or no pairing at all, and the child's name it keeps. */
internal class RigHost(var rig: SeamPhoneRig?, var name: String? = "Mira") : SeamHost {
  /** When set, the view the host gives in place of the rig's: a kept state that cannot be read, for one. */
  var given: HostView? = null

  override fun view(): HostView = given ?: rig?.hostView() ?: HostView(paired = false, json = null, problem = null)

  override fun childName(): String? = if (view().paired) name else null
}

/** The phone as the seam sees it. */
internal class RigPhone(
  var permissions: LimoPermissions = allOn(),
  var names: Map<String, String> = NAMES,
  var maker: String = "samsung",
) : SeamPhone {
  var namesAsked = 0

  override fun permissions(): LimoPermissions = permissions

  override fun names(): Map<String, String> = names.also { namesAsked += 1 }

  override fun maker(): String = maker
}

internal fun allOn() = LimoPermissions(
  notifications = true,
  phone = true,
  usageAccess = true,
  accessibility = true,
  accessibilityRunning = true,
  batteryExempt = true,
  deviceAdmin = true,
)

/** The marks in memory, written through the file's own text, as the app keeps them. */
internal class MemoryMarks : MarksStore {
  var text: String? = null
  var writes = 0

  override fun read(): SetupMarks? = text?.let { FileMarks.parse(it) }

  override fun write(marks: SetupMarks) {
    text = FileMarks.text(marks)
    writes += 1
  }
}

/** JSON out, for the welcome changed and the answers written for the screens' contract test. */
object TestJson {
  fun write(value: Any?, indent: String? = null): String = StringBuilder().also { put(it, value, indent, "") }.toString()

  private fun put(out: StringBuilder, value: Any?, indent: String?, at: String) {
    val inner = indent?.let { at + it }
    val nl = if (indent != null) "\n" else ""
    when (value) {
      null -> out.append("null")
      is Boolean -> out.append(value)
      is Int, is Long -> out.append(value)
      is Double -> out.append(value.also { require(it.isFinite()) })
      is String -> quote(out, value)
      is Map<*, *> -> {
        if (value.isEmpty()) {
          out.append("{}")
          return
        }
        out.append('{').append(nl)
        value.entries.forEachIndexed { i, (k, v) ->
          if (i > 0) out.append(',').append(nl)
          out.append(inner.orEmpty())
          quote(out, k as String)
          out.append(if (indent != null) ": " else ":")
          put(out, v, indent, inner.orEmpty())
        }
        out.append(nl).append(if (indent != null) at else "").append('}')
      }
      is List<*> -> {
        if (value.isEmpty()) {
          out.append("[]")
          return
        }
        out.append('[').append(nl)
        value.forEachIndexed { i, v ->
          if (i > 0) out.append(',').append(nl)
          out.append(inner.orEmpty())
          put(out, v, indent, inner.orEmpty())
        }
        out.append(nl).append(if (indent != null) at else "").append(']')
      }
      else -> error("not JSON: ${value.javaClass.simpleName}")
    }
  }

  private fun quote(out: StringBuilder, s: String) {
    out.append('"')
    for (c in s) {
      when {
        c == '"' -> out.append("\\\"")
        c == '\\' -> out.append("\\\\")
        c == '\n' -> out.append("\\n")
        c < ' ' || c.code > 0x7e -> out.append("\\u%04x".format(c.code))
        else -> out.append(c)
      }
    }
    out.append('"')
  }
}

/**
 * The answers written for the screens' contract test (`tests/seamContract.test.ts`
 * in the app): each a small JSON file in the app's `tests/seam-answers/`,
 * `{ "request": ..., "answer": ... }`, written again by every run of the
 * JVM tests, and only when its content changed.
 */
object SeamAnswerFiles {
  val folder: File by lazy {
    var dir: File? = File(System.getProperty("user.dir") ?: ".").absoluteFile
    while (dir != null) {
      val app = File(dir, "modules/limo-child")
      if (app.isDirectory && File(dir, "package.json").isFile) return@lazy File(dir, "tests/seam-answers")
      dir = dir.parentFile
    }
    error("the app was not found from the tests' working folder")
  }

  fun write(name: String, request: String, answer: Any?) {
    folder.mkdirs()
    val text = TestJson.write(linkedMapOf("request" to request, "answer" to answer), indent = "  ") + "\n"
    val file = File(folder, "$name.json")
    if (!file.isFile || file.readText(Charsets.UTF_8) != text) file.writeText(text, Charsets.UTF_8)
  }
}
