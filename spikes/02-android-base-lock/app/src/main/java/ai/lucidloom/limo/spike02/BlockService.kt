package ai.lucidloom.limo.spike02

import android.accessibilityservice.AccessibilityService
import android.app.KeyguardManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.PixelFormat
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.util.TypedValue
import android.view.Choreographer
import android.view.ContextThemeWrapper
import android.view.Gravity
import android.view.View
import android.view.ViewTreeObserver
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView

/**
 * The accessibility service. It listens only for window state changes, reads
 * only the package name of each event and its time stamp, and covers a
 * blocked app with an opaque TYPE_ACCESSIBILITY_OVERLAY window.
 *
 * It never reads event text, content descriptions, class names or node trees,
 * and its config sets canRetrieveWindowContent to false.
 */
class BlockService : AccessibilityService() {

    companion object {
        private const val TAG = SpikeState.TAG
        private const val HOME_FALLBACK_MS = 3000L

        /** The connected service, or null. Main thread only. */
        var instance: BlockService? = null
            private set
    }

    private val main = Handler(Looper.getMainLooper())
    private lateinit var windowManager: WindowManager
    private lateinit var keyguard: KeyguardManager
    private var system = SystemPackages(emptySet(), emptySet())

    /** The package we believe is in front, from the last event that counted. */
    private var frontPackage: String? = null

    private var cover: View? = null
    private var coverLabel: TextView? = null
    private var coverPackage: String? = null
    /** Bumped for every cover added, so a late frame callback for an old cover is ignored. */
    private var coverSerial = 0

    private var receiverRegistered = false

    private lateinit var audio: AudioManager
    /** Android 12 and later only. Typed loosely so older phones never load the class. */
    private var modeListener: Any? = null

    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            when (intent.action) {
                Intent.ACTION_SCREEN_OFF -> hideCover("screen_off")
                // Screen on with no lock screen set, or the lock screen
                // dismissed: the app that was in front is back. evaluate()
                // itself refuses while the lock screen is up.
                Intent.ACTION_SCREEN_ON,
                Intent.ACTION_USER_PRESENT -> evaluate("unlock", SystemClock.uptimeMillis())
                Intent.ACTION_INPUT_METHOD_CHANGED -> system = SystemPackages.load(this@BlockService)
            }
        }
    }

    private val safetyStop = Runnable {
        if (SpikeState.expireIfDue(this)) {
            hideCover("safety_stop")
        }
        evaluate("state", SystemClock.uptimeMillis())
        scheduleSafetyStop()
    }

    // lifecycle

    override fun onServiceConnected() {
        super.onServiceConnected()
        windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
        keyguard = getSystemService(KEYGUARD_SERVICE) as KeyguardManager
        system = SystemPackages.load(this)

        audio = getSystemService(AUDIO_SERVICE) as AudioManager
        if (Build.VERSION.SDK_INT >= 31) {
            val listener = AudioManager.OnModeChangedListener { mode ->
                if (isPhoneCallMode(mode)) {
                    hideCover("phone_call")
                } else {
                    evaluate("state", SystemClock.uptimeMillis())
                }
            }
            audio.addOnModeChangedListener(mainExecutor, listener)
            modeListener = listener
        }

        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_SCREEN_OFF)
            addAction(Intent.ACTION_SCREEN_ON)
            addAction(Intent.ACTION_USER_PRESENT)
            addAction(Intent.ACTION_INPUT_METHOD_CHANGED)
        }
        if (Build.VERSION.SDK_INT >= 33) {
            registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            registerReceiver(receiver, filter)
        }
        receiverRegistered = true

        instance = this
        SpikeState.expireIfDue(this)
        scheduleSafetyStop()
        Log.i(
            TAG,
            "service connected blocking=${if (SpikeState.isOn(this)) "on" else "off"}" +
                " blocked=${SpikeState.blocked(this).sorted().joinToString(",")}" +
                " keyboards=${system.keyboards.sorted().joinToString(",")}",
        )
    }

    override fun onUnbind(intent: Intent?): Boolean {
        shutDown("service_off")
        return super.onUnbind(intent)
    }

    override fun onDestroy() {
        shutDown("service_off")
        super.onDestroy()
    }

    override fun onInterrupt() {
        // Nothing to interrupt: this service gives no spoken or other feedback.
    }

    private fun shutDown(reason: String) {
        hideCover(reason)
        main.removeCallbacksAndMessages(null)
        if (receiverRegistered) {
            try {
                unregisterReceiver(receiver)
            } catch (_: Exception) {
            }
            receiverRegistered = false
        }
        val listener = modeListener
        if (listener != null && Build.VERSION.SDK_INT >= 31) {
            try {
                audio.removeOnModeChangedListener(listener as AudioManager.OnModeChangedListener)
            } catch (_: Exception) {
            }
            modeListener = null
        }
        if (instance === this) {
            instance = null
            Log.i(TAG, "service disconnected")
        }
    }

    // events

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        if (event == null) return
        if (event.eventType != AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED) return
        // Only these two fields are read: the package name and the time stamp.
        val pkg = event.packageName?.toString()
        val eventUptime = event.eventTime

        val front = CoverRules.frontPackageOf(pkg, packageName, system.keyboards)
        if (front == null) {
            Log.d(TAG, "event package=${pkg ?: "none"} counted=no")
            return
        }
        Log.d(TAG, "event package=$front counted=yes")
        // An event that counts settles the question the Home fallback waits on.
        main.removeCallbacks(homeFallback)
        frontPackage = front
        evaluate("event", eventUptime)
    }

    /** Called by the activity in onResume: this app is in front now. */
    fun onOwnUiInFront() {
        frontPackage = packageName
        evaluate("state", SystemClock.uptimeMillis())
    }

    /** Called when the blocked list or the switch changes. */
    fun onSettingsChanged() {
        system = SystemPackages.load(this)
        scheduleSafetyStop()
        evaluate("state", SystemClock.uptimeMillis())
    }

    /**
     * Applies the rule to the current front package. [startUptime] is the
     * moment the measurement runs from: the event's own time for "event",
     * the moment the unlock broadcast arrived for "unlock".
     */
    private fun evaluate(trigger: String, startUptime: Long) {
        if (instance !== this) return
        val front = frontPackage
        val active = SpikeState.isOn(this)
        val locked = keyguard.isKeyguardLocked
        val call = isPhoneCallMode(audio.mode)
        if (CoverRules.shouldCover(front, SpikeState.blocked(this), active, locked, call, packageName, system.neverCover)) {
            showCover(front!!, trigger, startUptime)
        } else if (cover != null) {
            val reason = when {
                !active -> "blocking_off"
                locked -> "keyguard"
                call -> "phone_call"
                else -> "front_changed new=${front ?: "unknown"}"
            }
            hideCover(reason)
        }
    }

    /**
     * A phone call is ringing or in progress. The cover sits above the
     * notification shade, so it could hide a heads up incoming call; it never
     * shows during one. Internet calls (MODE_IN_COMMUNICATION) are not counted.
     */
    private fun isPhoneCallMode(mode: Int): Boolean =
        mode == AudioManager.MODE_RINGTONE ||
            mode == AudioManager.MODE_IN_CALL ||
            mode == AudioManager.MODE_CALL_SCREENING ||
            mode == AudioManager.MODE_CALL_REDIRECT

    private fun scheduleSafetyStop() {
        main.removeCallbacks(safetyStop)
        val left = SpikeState.msLeft(this)
        if (left > 0L) main.postDelayed(safetyStop, left + 50L)
    }

    // the cover

    private fun showCover(pkg: String, trigger: String, startUptime: Long) {
        val existing = cover
        if (existing != null) {
            if (coverPackage != pkg) {
                coverPackage = pkg
                coverLabel?.text = labelOf(pkg)
                Log.i(TAG, "cover package=$pkg relabel")
            }
            return
        }

        val view = buildCover(pkg)
        val lp = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            // Not focusable: the blocked app keeps key focus, so Back and the
            // keyboard still go to it. The cover takes every touch on screen.
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
            PixelFormat.OPAQUE,
        )
        lp.gravity = Gravity.TOP or Gravity.START
        lp.windowAnimations = 0
        lp.title = "LiMoSpikeCover"
        if (Build.VERSION.SDK_INT >= 28) {
            lp.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }
        if (Build.VERSION.SDK_INT >= 30) {
            // Cover the status bar and navigation bar areas too.
            lp.fitInsetsTypes = 0
            lp.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
        }

        try {
            windowManager.addView(view, lp)
        } catch (e: Exception) {
            Log.w(TAG, "cover package=$pkg failed error=${e.javaClass.simpleName}")
            return
        }
        coverSerial += 1
        cover = view
        coverPackage = pkg
        measureFirstFrame(view, coverSerial, pkg, trigger, startUptime)
    }

    private fun hideCover(reason: String) {
        val view = cover ?: return
        val pkg = coverPackage
        cover = null
        coverLabel = null
        coverPackage = null
        main.removeCallbacks(homeFallback)
        try {
            windowManager.removeViewImmediate(view)
        } catch (e: Exception) {
            Log.w(TAG, "uncover failed error=${e.javaClass.simpleName}")
        }
        Log.i(TAG, "uncover package=$pkg reason=$reason")
    }

    /**
     * Times the first frame of a new cover. On Android 10 and later, with
     * hardware drawing, the time is taken when the first frame has been
     * rendered and handed to the display system ("commit"). Otherwise it is
     * taken at the vsync after the first draw ("vsync").
     */
    private fun measureFirstFrame(view: View, serial: Int, pkg: String, trigger: String, startUptime: Long) {
        view.viewTreeObserver.addOnPreDrawListener(object : ViewTreeObserver.OnPreDrawListener {
            override fun onPreDraw(): Boolean {
                val vto = view.viewTreeObserver
                if (vto.isAlive) vto.removeOnPreDrawListener(this)
                if (Build.VERSION.SDK_INT >= 29 && view.isHardwareAccelerated) {
                    view.viewTreeObserver.registerFrameCommitCallback {
                        val now = SystemClock.uptimeMillis()
                        main.post { record(serial, pkg, trigger, startUptime, now, "commit") }
                    }
                } else {
                    Choreographer.getInstance().postFrameCallback {
                        record(serial, pkg, trigger, startUptime, SystemClock.uptimeMillis(), "vsync")
                    }
                }
                return true
            }
        })
    }

    private fun record(serial: Int, pkg: String, trigger: String, startUptime: Long, frameUptime: Long, method: String) {
        // The cover was removed or replaced before its first frame: no measurement.
        if (serial != coverSerial || cover == null) return
        val ms = frameUptime - startUptime
        SpikeState.addMeasurement(
            SpikeState.Measurement(pkg, ms, method, trigger, System.currentTimeMillis()),
        )
        Log.i(TAG, "cover package=$pkg shown_ms=$ms method=$method trigger=$trigger")
    }

    private val homeFallback = Runnable {
        // Home was pressed on the cover, but no event moved the front app away
        // from the blocked one within 3 s. Remove the cover anyway, so a missed
        // launcher event can never leave the cover stuck. Logged, so the test
        // can see it happen.
        if (cover != null) hideCover("home_fallback")
    }

    private fun goHome() {
        Log.i(TAG, "home pressed package=$coverPackage")
        performGlobalAction(GLOBAL_ACTION_HOME)
        main.removeCallbacks(homeFallback)
        main.postDelayed(homeFallback, HOME_FALLBACK_MS)
    }

    private fun labelOf(pkg: String): String = try {
        val pm = packageManager
        @Suppress("DEPRECATION")
        pm.getApplicationLabel(pm.getApplicationInfo(pkg, 0)).toString()
    } catch (_: PackageManager.NameNotFoundException) {
        pkg
    }

    private fun buildCover(pkg: String): View {
        val ctx = ContextThemeWrapper(this, android.R.style.Theme_DeviceDefault)
        val root = FrameLayout(ctx)
        root.setBackgroundColor(Color.rgb(16, 20, 24))

        val column = LinearLayout(ctx)
        column.orientation = LinearLayout.VERTICAL
        column.gravity = Gravity.CENTER_HORIZONTAL
        val pad = dp(24)
        column.setPadding(pad, pad, pad, pad)

        val title = TextView(ctx)
        title.text = "LiMo spike: this app is blocked"
        title.setTextColor(Color.WHITE)
        title.setTextSize(TypedValue.COMPLEX_UNIT_SP, 22f)
        title.gravity = Gravity.CENTER

        val label = TextView(ctx)
        label.text = labelOf(pkg)
        label.setTextColor(Color.rgb(255, 205, 90))
        label.setTextSize(TypedValue.COMPLEX_UNIT_SP, 28f)
        label.gravity = Gravity.CENTER
        label.setPadding(0, dp(16), 0, dp(32))

        val button = Button(ctx)
        button.text = "Go to home screen"
        button.isAllCaps = false
        button.setOnClickListener { goHome() }

        column.addView(title, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        column.addView(label, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        column.addView(button, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT))

        root.addView(
            column,
            FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.WRAP_CONTENT,
                Gravity.CENTER,
            ),
        )
        coverLabel = label
        return root
    }

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density + 0.5f).toInt()
}
