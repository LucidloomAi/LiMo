package ai.lucidloom.limo.spike02

import android.app.Activity
import android.content.ComponentName
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInstaller
import android.graphics.Typeface
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.util.Log
import android.util.TypedValue
import android.view.View
import android.view.WindowInsets
import android.widget.Button
import android.widget.CheckBox
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch
import android.widget.TextView
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * The one screen. Plain framework views built in code.
 *
 * Intent extras, so tests can be driven from a PC (see README.md):
 *   --es block "pkg1,pkg2"   replace the blocked list
 *   --ez clear true          empty the blocked list
 *   --ez clear_timings true  empty the list of cover timings
 *   --ez on true|false       switch blocking on (fresh 10 minutes) or off
 */
class MainActivity : Activity() {

    private val tag = SpikeState.TAG
    private val handler = Handler(Looper.getMainLooper())

    private lateinit var serviceText: TextView
    private lateinit var blockSwitch: Switch
    private lateinit var leftText: TextView
    private lateinit var statsText: TextView
    private lateinit var timingsText: TextView
    private lateinit var phoneText: TextView
    private lateinit var appsBox: LinearLayout

    private val boxes = LinkedHashMap<String, CheckBox>()
    private var syncing = false
    private var shownMeasurements = -1L

    private val ticker = object : Runnable {
        override fun run() {
            refreshLive()
            handler.postDelayed(this, 1000L)
        }
    }

    // lifecycle

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(buildUi())
        if (savedInstanceState == null) applyExtras(intent)
        phoneText.text = phoneInfo().joinToString("\n")
        Log.i(tag, "phone " + phoneInfoForLog())
        loadApps()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        applyExtras(intent)
        loadApps()
    }

    override fun onResume() {
        super.onResume()
        // This app is in front: never covered.
        BlockService.instance?.onOwnUiInFront()
        syncChecks()
        shownMeasurements = -1L
        handler.removeCallbacks(ticker)
        handler.post(ticker)
    }

    override fun onPause() {
        handler.removeCallbacks(ticker)
        super.onPause()
    }

    // intent extras

    private fun applyExtras(intent: Intent?) {
        if (intent == null) return
        if ((intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0) return
        val extras = intent.extras ?: return

        if (intent.getBooleanExtra("clear", false)) {
            Log.i(tag, "intent clear=true")
            SpikeState.setBlocked(this, emptySet())
        }

        if (intent.hasExtra("block")) {
            val raw = intent.getStringExtra("block")
            Log.i(tag, "intent block=${raw ?: ""}")
            val (good, bad) = CoverRules.parsePackageList(raw)
            for (b in bad) Log.w(tag, "refused package=$b reason=bad_name")
            val system = SystemPackages.load(this)
            val allowed = LinkedHashSet<String>()
            for (p in good) {
                if (CoverRules.mayCover(p, packageName, system.neverCover)) {
                    allowed.add(p)
                } else {
                    Log.w(tag, "refused package=$p reason=never_cover")
                }
            }
            SpikeState.setBlocked(this, allowed)
        }

        if (intent.getBooleanExtra("clear_timings", false)) {
            Log.i(tag, "intent clear_timings=true")
            SpikeState.clearMeasurements()
        }

        if (extras.containsKey("on")) {
            @Suppress("DEPRECATION")
            val on = when (val v = extras.get("on")) {
                is Boolean -> v
                is String -> v.trim().equals("true", ignoreCase = true)
                else -> false
            }
            Log.i(tag, "intent on=$on")
            SpikeState.setOn(this, on, "intent")
        }

        // Applied once. Do not apply again if the activity is recreated.
        intent.removeExtra("clear")
        intent.removeExtra("block")
        intent.removeExtra("clear_timings")
        intent.removeExtra("on")
    }

    // live parts

    private fun refreshLive() {
        SpikeState.expireIfDue(this)

        val enabled = isServiceEnabledInSettings()
        val connected = BlockService.instance != null
        serviceText.text =
            "Turned on in Accessibility settings: ${yesNo(enabled)}\n" +
            "Running and connected: ${yesNo(connected)}"

        val on = SpikeState.isOn(this)
        if (blockSwitch.isChecked != on) {
            syncing = true
            blockSwitch.isChecked = on
            syncing = false
        }
        val left = SpikeState.msLeft(this)
        leftText.text = if (on) {
            "Turns itself off in ${CoverRules.formatLeft(left)}."
        } else {
            "Off. Switching on starts a fresh 10 minutes."
        }

        if (SpikeState.measurementsAdded != shownMeasurements) {
            shownMeasurements = SpikeState.measurementsAdded
            showTimings()
        }
    }

    private fun showTimings() {
        val all = SpikeState.measurements.toList()
        val fromEvents = all.filter { it.trigger == "event" }.map { it.shownMs }
        val median = CoverRules.median(fromEvents)
        val slowest = CoverRules.slowest(fromEvents)
        statsText.text = if (median == null) {
            "No covers shown yet."
        } else {
            "Median $median ms, slowest $slowest ms, over ${fromEvents.size} covers shown after an app came to the front.\n" +
                "Covers shown after an unlock, a call or a setting change are listed but not counted."
        }
        val fmt = SimpleDateFormat("HH:mm:ss", Locale.US)
        val lines = all.map { m ->
            val extra = if (m.trigger == "event") "" else "  (${m.trigger})"
            "${fmt.format(Date(m.atWallMs))}  ${m.shownMs.toString().padStart(5)} ms  ${m.pkg}  ${m.method}$extra"
        }
        timingsText.text = if (lines.isEmpty()) "" else "Newest first:\n" + lines.joinToString("\n")
    }

    private fun isServiceEnabledInSettings(): Boolean {
        val raw = Settings.Secure.getString(contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES)
            ?: return false
        val me = ComponentName(this, BlockService::class.java)
        return raw.split(':').any { ComponentName.unflattenFromString(it.trim()) == me }
    }

    // app list

    @Suppress("DEPRECATION")
    private fun loadApps() {
        appsBox.removeAllViews()
        boxes.clear()
        val pm = packageManager
        val system = SystemPackages.load(this)
        val blocked = SpikeState.blocked(this)

        val labels = HashMap<String, String>()
        val launch = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        for (ri in pm.queryIntentActivities(launch, 0)) {
            val pkg = ri.activityInfo?.packageName ?: continue
            if (pkg == packageName || labels.containsKey(pkg)) continue
            labels[pkg] = ri.activityInfo.applicationInfo.loadLabel(pm).toString()
        }
        // A blocked package set from adb that has no launcher entry still gets
        // a row, so it can be unticked here.
        for (pkg in blocked) {
            if (!labels.containsKey(pkg)) labels[pkg] = "(not in the launcher)"
        }

        for ((pkg, label) in labels.entries.sortedBy { it.value.lowercase(Locale.ROOT) }) {
            val never = !CoverRules.mayCover(pkg, packageName, system.neverCover)
            val cb = CheckBox(this)
            cb.text = if (never) "$label\n$pkg\nnever covered" else "$label\n$pkg"
            cb.isEnabled = !never
            cb.isChecked = pkg in blocked
            cb.setPadding(0, dp(6), 0, dp(6))
            cb.setOnCheckedChangeListener { _, checked ->
                if (!syncing) onAppTicked(pkg, checked)
            }
            appsBox.addView(cb)
            boxes[pkg] = cb
        }
    }

    private fun onAppTicked(pkg: String, checked: Boolean) {
        val set = SpikeState.blocked(this).toMutableSet()
        if (checked) set.add(pkg) else set.remove(pkg)
        SpikeState.setBlocked(this, set)
    }

    private fun syncChecks() {
        val blocked = SpikeState.blocked(this)
        syncing = true
        for ((pkg, cb) in boxes) cb.isChecked = pkg in blocked
        syncing = false
        // A package added from adb may need a new row.
        if (blocked.any { it !in boxes }) loadApps()
    }

    // phone facts

    private fun phoneInfo(): List<String> {
        val debuggable = (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
        val lines = ArrayList<String>()
        lines += "Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})"
        lines += "Device: ${Build.MANUFACTURER} ${Build.MODEL}"
        lines += "Build: ${Build.DISPLAY}"
        lines += installerInfo().map { (k, v) -> "$k: $v" }
        lines += "Debuggable: ${yesNo(debuggable)}"
        return lines
    }

    private fun phoneInfoForLog(): String {
        val debuggable = (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
        val installer = installerInfo().joinToString(" ") { (k, v) ->
            k.lowercase(Locale.ROOT).replace(' ', '_') + "=" + v.replace(' ', '_')
        }
        return "android=${Build.VERSION.RELEASE} sdk=${Build.VERSION.SDK_INT} " +
            "model=${Build.MODEL.replace(' ', '_')} $installer debuggable=$debuggable"
    }

    private fun installerInfo(): List<Pair<String, String>> = try {
        if (Build.VERSION.SDK_INT >= 30) {
            val info = packageManager.getInstallSourceInfo(packageName)
            val out = ArrayList<Pair<String, String>>()
            out += "Installing package" to (info.installingPackageName ?: "none")
            out += "Initiating package" to (info.initiatingPackageName ?: "none")
            out += "Originating package" to (info.originatingPackageName ?: "none")
            if (Build.VERSION.SDK_INT >= 33) {
                out += "Package source" to packageSourceName(info.packageSource)
            }
            out
        } else {
            @Suppress("DEPRECATION")
            listOf("Installing package" to (packageManager.getInstallerPackageName(packageName) ?: "none"))
        }
    } catch (e: Exception) {
        listOf("Installer" to "could not read (${e.javaClass.simpleName})")
    }

    private fun packageSourceName(source: Int): String = when (source) {
        PackageInstaller.PACKAGE_SOURCE_UNSPECIFIED -> "unspecified"
        PackageInstaller.PACKAGE_SOURCE_OTHER -> "other"
        PackageInstaller.PACKAGE_SOURCE_STORE -> "store"
        PackageInstaller.PACKAGE_SOURCE_LOCAL_FILE -> "local file"
        PackageInstaller.PACKAGE_SOURCE_DOWNLOADED_FILE -> "downloaded file"
        else -> "unknown $source"
    }

    // views

    private fun buildUi(): View {
        val scroll = ScrollView(this)
        val col = LinearLayout(this)
        col.orientation = LinearLayout.VERTICAL
        val pad = dp(16)
        col.setPadding(pad, pad, pad, pad)
        scroll.addView(col)

        col.addView(heading("Accessibility service"))
        serviceText = body()
        col.addView(serviceText)
        col.addView(button("Open Accessibility settings") {
            startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
        })
        col.addView(button("Open this app's info page") {
            startActivity(
                Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")),
            )
        })

        col.addView(heading("Blocking"))
        blockSwitch = Switch(this)
        blockSwitch.text = "Blocking on"
        blockSwitch.setTextSize(TypedValue.COMPLEX_UNIT_SP, 18f)
        blockSwitch.setOnCheckedChangeListener { _, checked ->
            if (!syncing) {
                SpikeState.setOn(this, checked, "switch")
                refreshLive()
            }
        }
        col.addView(blockSwitch)
        leftText = body()
        col.addView(leftText)

        col.addView(heading("Cover timings, last ${SpikeState.MAX_MEASUREMENTS}"))
        statsText = body()
        col.addView(statsText)
        timingsText = body()
        timingsText.typeface = Typeface.MONOSPACE
        timingsText.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
        col.addView(timingsText)
        col.addView(button("Clear timings") {
            SpikeState.clearMeasurements()
            refreshLive()
        })

        col.addView(heading("This phone"))
        phoneText = body()
        col.addView(phoneText)

        col.addView(heading("Apps: tick to block"))
        appsBox = LinearLayout(this)
        appsBox.orientation = LinearLayout.VERTICAL
        col.addView(appsBox)

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

    private fun yesNo(b: Boolean) = if (b) "yes" else "no"

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density + 0.5f).toInt()
}
