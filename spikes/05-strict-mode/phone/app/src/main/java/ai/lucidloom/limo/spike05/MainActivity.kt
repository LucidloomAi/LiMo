package ai.lucidloom.limo.spike05

import android.app.Activity
import android.app.AlertDialog
import android.app.admin.DevicePolicyManager
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.graphics.Typeface
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.UserManager
import android.text.InputType
import android.util.TypedValue
import android.view.View
import android.view.WindowInsets
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch
import android.widget.TextView
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/**
 * The one screen. Plain framework views built in code.
 *
 * Intent extras, so everything can be done from a PC (see README.md):
 *   --ez apply true                    Apply strict mode (all on, safe order)
 *   --es suspend "com.a,com.b"         set the list to suspend, and suspend it
 *   --ez release true --es pin 2468    release this phone
 *   --ei deadline_minutes N            lower the dead man's timer to N minutes
 *   --ez extend true                   extend the timer by 30 minutes
 *   --ez block_uninstall true|false    and one switch per item:
 *   --ez no_config_date_time, no_add_user, no_safe_boot, suspend_apps,
 *        no_factory_reset true|false
 */
class MainActivity : Activity() {

    private val handler = Handler(Looper.getMainLooper())

    private lateinit var ownerText: TextView
    private lateinit var timerText: TextView
    private lateinit var listEdit: EditText
    private lateinit var suspendedText: TextView
    private lateinit var phoneText: TextView
    private lateinit var logText: TextView
    private val switches = LinkedHashMap<Step, Switch>()
    private val switchNotes = LinkedHashMap<Step, TextView>()

    private var syncing = false
    private val pending = AtomicInteger(0)
    private val checkQueued = AtomicBoolean(false)
    private var ticks = 0
    private var shownLog = -1L

    private val ticker = object : Runnable {
        override fun run() {
            ticks += 1
            if (ticks % 5 == 0) queueCheck(Strict.SOURCE_TICK)
            refresh()
            handler.postDelayed(this, 1000L)
        }
    }

    // lifecycle

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(buildUi())
        listEdit.setText(Store.wanted(this).joinToString(","))
        SpikeLog.i(
            "app start android=${Build.VERSION.RELEASE} sdk=${Build.VERSION.SDK_INT} " +
                "model=${Build.MODEL.replace(' ', '_')} test_only=${LogText.yesNo(isTestOnly())}",
        )
        if (savedInstanceState == null) applyExtras(intent) else queueCheck("activity")
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        applyExtras(intent)
    }

    override fun onResume() {
        super.onResume()
        queueCheck("activity")
        handler.removeCallbacks(ticker)
        handler.post(ticker)
    }

    override fun onPause() {
        handler.removeCallbacks(ticker)
        super.onPause()
    }

    private fun queueCheck(source: String) {
        if (!checkQueued.compareAndSet(false, true)) return
        val app = applicationContext
        Strict.worker.execute {
            try {
                Strict.check(app, source)
            } finally {
                checkQueued.set(false)
            }
        }
    }

    private fun onWorker(task: () -> Unit) {
        pending.incrementAndGet()
        Strict.worker.execute {
            try {
                task()
            } finally {
                pending.decrementAndGet()
            }
        }
    }

    // intent extras

    private fun applyExtras(intent: Intent?) {
        if (intent == null) return
        if ((intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0) return
        val extras = intent.extras
        if (extras == null || extras.isEmpty) {
            queueCheck("activity")
            return
        }

        val release = boolExtra(extras, "release")
        val pin = textExtra(extras, "pin")
        val deadline = intExtra(extras, "deadline_minutes")
        val extend = boolExtra(extras, "extend")
        val apply = boolExtra(extras, "apply")
        val suspendRaw = if (extras.containsKey("suspend")) textExtra(extras, "suspend").orEmpty() else null
        val switchesWanted = LinkedHashMap<Step, Boolean>()
        for (s in Step.entries) boolExtra(extras, s.key)?.let { switchesWanted[s] = it }

        // Never log the PIN itself.
        SpikeLog.i(
            "intent release=${release ?: "-"} pin=${if (pin == null) "missing" else "given"} " +
                "deadline_minutes=${deadline ?: "-"} extend=${extend ?: "-"} apply=${apply ?: "-"} " +
                "suspend=${suspendRaw ?: "-"} switches=${switchesWanted.entries.joinToString(",") { "${it.key.key}:${it.value}" }.ifEmpty { "-" }}",
        )

        // Applied once. Never again if the activity is recreated.
        for (k in extras.keySet().toList()) intent.removeExtra(k)

        val app = applicationContext
        onWorker {
            Strict.check(app, "activity")
            if (release == true) {
                if (PinRule.matches(pin)) {
                    Strict.releaseEverything(app, "adb")
                } else {
                    SpikeLog.w("release refused reason=${if (pin == null) "no_pin" else "wrong_pin"} source=adb")
                }
                // A release request does nothing else.
                return@onWorker
            }
            if (deadline != null) Strict.lowerDeadline(app, deadline, "adb")
            if (extend == true) Strict.extendDeadline(app, "adb")
            var switchesFinal: Map<Step, Boolean> = switchesWanted
            if (suspendRaw != null) {
                val accepted = Strict.setWantedList(app, suspendRaw, "adb")
                switchesFinal = Steps.withListChange(switchesWanted, true, accepted)
                runOnUiThread { listEdit.setText(Store.wanted(app).joinToString(",")) }
            }
            val plan = Steps.plan(apply == true, switchesFinal)
            if (plan.isNotEmpty()) Strict.applyPlan(app, plan, "adb")
        }
    }

    @Suppress("DEPRECATION")
    private fun boolExtra(extras: Bundle, key: String): Boolean? = when (val v = extras.get(key)) {
        is Boolean -> v
        is String -> when (v.trim().lowercase(Locale.ROOT)) {
            "true", "1", "yes" -> true
            "false", "0", "no" -> false
            else -> null
        }
        else -> null
    }

    @Suppress("DEPRECATION")
    private fun intExtra(extras: Bundle, key: String): Int? = when (val v = extras.get(key)) {
        is Int -> v
        is Long -> v.toInt()
        is String -> v.trim().toIntOrNull()
        else -> null
    }

    @Suppress("DEPRECATION")
    private fun textExtra(extras: Bundle, key: String): String? = when (val v = extras.get(key)) {
        null -> null
        is String -> v
        else -> v.toString()
    }

    // live parts

    private fun refresh() {
        val dpm = getSystemService(DevicePolicyManager::class.java)
        val um = getSystemService(UserManager::class.java)
        val admin = Strict.isAdmin(this)
        val owner = Strict.isOwner(this)

        ownerText.text =
            "Active admin: ${LogText.yesNo(admin)}\n" +
            "Device Owner: ${LogText.yesNo(owner)}\n" +
            "Installed as test only: ${LogText.yesNo(isTestOnly())}"

        val left = Strict.leftMsNow(this)
        timerText.text = when {
            left == null && (admin || owner) -> "Starting."
            left == null -> "Not running. It starts when this app becomes an admin or Device Owner."
            else -> {
                val at = SimpleDateFormat("HH:mm", Locale.US).format(Date(System.currentTimeMillis() + left.coerceAtLeast(0L)))
                "Releases this phone by itself in ${TimerRules.formatLeft(left)}, at about $at.\n" +
                    "Never more than ${TimerRules.MAX_MINUTES} minutes ahead."
            }
        }

        val setByMe = try {
            if (owner) dpm.getUserRestrictions(Strict.admin(this)) else Bundle()
        } catch (_: Exception) {
            Bundle()
        }
        val mine = Store.mine(this)
        val suspendedNow = mine.filter { Strict.isSuspendedNow(this, it) == true }
        for ((step, sw) in switches) {
            val (isOn, note) = when {
                step == Step.BLOCK_UNINSTALL -> {
                    val blocked = try {
                        dpm.isUninstallBlocked(null, packageName)
                    } catch (_: Exception) {
                        false
                    }
                    blocked to "In force on this phone: ${LogText.yesNo(blocked)}"
                }
                step.isUserRestriction -> {
                    val inForce = try {
                        um.hasUserRestriction(step.key)
                    } catch (_: Exception) {
                        false
                    }
                    setByMe.getBoolean(step.key, false) to
                        "Set by this app: ${LogText.yesNo(setByMe.getBoolean(step.key, false))}, in force on this phone: ${LogText.yesNo(inForce)}"
                }
                else -> suspendedNow.isNotEmpty() to "Suspended by this app now: ${SuspendRules.joinList(suspendedNow)}"
            }
            if (pending.get() == 0 && sw.isChecked != isOn) {
                syncing = true
                sw.isChecked = isOn
                syncing = false
            }
            switchNotes[step]?.text = note
        }

        val listed = (Store.wanted(this) + mine).distinct()
        suspendedText.text = if (listed.isEmpty()) {
            "No apps listed."
        } else {
            listed.joinToString("\n") { p ->
                val s = when (Strict.isSuspendedNow(this, p)) {
                    true -> "suspended"
                    false -> "not suspended"
                    null -> "not installed, or cannot tell"
                }
                "$p: $s"
            }
        }

        if (ticks <= 1 || ticks % 5 == 0) phoneText.text = phoneFacts().joinToString("\n")

        if (SpikeLog.added != shownLog) {
            shownLog = SpikeLog.added
            logText.text = SpikeLog.recentLines().joinToString("\n")
        }
    }

    // phone facts

    private fun isTestOnly(): Boolean = (applicationInfo.flags and ApplicationInfo.FLAG_TEST_ONLY) != 0

    private fun phoneFacts(): List<String> {
        val lines = ArrayList<String>()
        lines += "Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})"
        lines += "Device: ${Build.MANUFACTURER} ${Build.MODEL}"
        lines += "Build: ${Build.DISPLAY}"
        lines += "Users: ${usersText()}"
        lines += try {
            "Profiles of this user, counting itself: ${getSystemService(UserManager::class.java).userProfiles.size}"
        } catch (_: Exception) {
            "Profiles of this user: could not read"
        }
        lines += "Accounts: not read here. An app cannot see other apps' accounts without asking for a permission; " +
            "the PC's check reads them with dumpsys account."
        lines += "Debuggable: ${LogText.yesNo((applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0)}"
        return lines
    }

    private fun usersText(): String {
        if (!Strict.isOwner(this)) return "readable only while this app is Device Owner; the PC's check reads it"
        return try {
            val dpm = getSystemService(DevicePolicyManager::class.java)
            "${1 + dpm.getSecondaryUsers(Strict.admin(this)).size} (read as Device Owner)"
        } catch (e: Exception) {
            "could not read (${e.javaClass.simpleName})"
        }
    }

    // release with the PIN

    private fun askPinAndRelease() {
        val edit = EditText(this)
        edit.inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD
        edit.hint = "PIN"
        val box = FrameLayout(this)
        box.setPadding(dp(24), dp(8), dp(24), 0)
        box.addView(edit)
        val app = applicationContext
        AlertDialog.Builder(this)
            .setTitle("Release this phone")
            .setMessage("Type the PIN. Release undoes everything this app set, then the app stops being Device Owner and admin.")
            .setView(box)
            .setPositiveButton("Release") { _, _ ->
                if (PinRule.matches(edit.text.toString())) {
                    onWorker { Strict.releaseEverything(app, "button") }
                } else {
                    SpikeLog.w("release refused reason=wrong_pin source=button")
                }
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    // views

    private fun buildUi(): View {
        val scroll = ScrollView(this)
        val col = LinearLayout(this)
        col.orientation = LinearLayout.VERTICAL
        val pad = dp(16)
        col.setPadding(pad, pad, pad, pad)
        scroll.addView(col)
        val app = applicationContext

        col.addView(heading("Owner"))
        ownerText = body()
        col.addView(ownerText)

        col.addView(heading("Dead man's timer"))
        timerText = body()
        col.addView(timerText)
        col.addView(button("Extend 30 minutes") { onWorker { Strict.extendDeadline(app, "button") } })

        col.addView(heading("Strict mode"))
        col.addView(button("Apply strict mode") { onWorker { Strict.applyPlan(app, Steps.plan(true, emptyMap()), "button") } })
        for (step in Steps.APPLY_ORDER) {
            val sw = Switch(this)
            sw.text = step.label
            sw.setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
            sw.setPadding(0, dp(10), 0, 0)
            sw.setOnCheckedChangeListener { _, checked ->
                if (!syncing) onWorker { Strict.applyPlan(app, Steps.plan(false, mapOf(step to checked)), "switch") }
            }
            col.addView(sw)
            switches[step] = sw
            val note = body()
            note.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            col.addView(note)
            switchNotes[step] = note
        }

        col.addView(heading("Apps to suspend"))
        col.addView(body().also { it.text = "Package names, separated by commas. At most ${SuspendRules.MAX_PACKAGES}. Launchers, dialers, Settings, keyboards and this app are never suspended." })
        listEdit = EditText(this)
        listEdit.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        listEdit.hint = "com.samsung.android.calendar"
        col.addView(listEdit)
        col.addView(button("Suspend these") {
            val raw = listEdit.text.toString()
            onWorker {
                val accepted = Strict.setWantedList(app, raw, "button")
                Strict.applyPlan(app, Steps.plan(false, Steps.withListChange(emptyMap(), true, accepted)), "button")
            }
        })
        suspendedText = body()
        col.addView(suspendedText)

        col.addView(heading("Release"))
        col.addView(button("Release this phone") { askPinAndRelease() })

        col.addView(heading("This phone"))
        phoneText = body()
        col.addView(phoneText)

        col.addView(heading("Log, newest first"))
        logText = body()
        logText.typeface = Typeface.MONOSPACE
        logText.setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
        col.addView(logText)

        if (Build.VERSION.SDK_INT >= 35) {
            // Android 15 and later draw this app edge to edge: keep clear of
            // the bars. Earlier versions already lay the screen out between them.
            scroll.setOnApplyWindowInsetsListener { v, insets ->
                val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
                v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                insets
            }
        }
        return scroll
    }

    private fun heading(text: String): TextView {
        val t = TextView(this)
        t.text = text
        t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f)
        t.setTypeface(t.typeface, Typeface.BOLD)
        t.setPadding(0, dp(20), 0, dp(6))
        return t
    }

    private fun body(): TextView {
        val t = TextView(this)
        t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
        t.setPadding(0, dp(2), 0, dp(2))
        t.setTextIsSelectable(true)
        return t
    }

    private fun button(text: String, onClick: () -> Unit): Button {
        val b = Button(this)
        b.text = text
        b.isAllCaps = false
        b.setOnClickListener { onClick() }
        return b
    }

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density + 0.5f).toInt()
}
