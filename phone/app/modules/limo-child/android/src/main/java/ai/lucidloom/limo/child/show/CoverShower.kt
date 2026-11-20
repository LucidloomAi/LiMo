package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.Cover
import ai.lucidloom.limo.child.ffi.Way
import ai.lucidloom.limo.child.ffi.WarningShown
import ai.lucidloom.limo.child.host.Host
import ai.lucidloom.limo.child.host.Shower
import ai.lucidloom.limo.child.watch.Watch
import ai.lucidloom.limo.child.watch.WatchService
import android.Manifest
import android.accessibilityservice.AccessibilityService
import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.graphics.PixelFormat
import android.graphics.drawable.Drawable
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.telecom.TelecomManager
import android.text.format.DateFormat
import android.util.Log
import android.view.ContextThemeWrapper
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowManager
import android.widget.FrameLayout
import java.time.ZoneId

/**
 * The covers on a phone: the host's [Shower]. Main thread only, as the host
 * calls it.
 *
 * The block screen and the lock are one window of the accessibility service
 * (`TYPE_ACCESSIBILITY_OVERLAY`, added through [WatchService.current]), as
 * proof two drew it: no permission of its own, opaque, over the whole
 * screen with Android's bars, not focusable (the app under it keeps the keys
 * and the keyboard). The warning is a second, small window at the top that
 * lets every touch outside it through. [ShowerCore] decides what is drawn;
 * this class only draws it.
 *
 * Made once, by [install]; [childName] gives the child's name for the lock's
 * label ("MIRA · THIS PHONE"), or null.
 */
class CoverShower private constructor(private val app: Context, private val childName: () -> String?) : Shower {
  private val main = Handler(Looper.getMainLooper())
  private val apps = AndroidApps(app)
  private val actions = Actions()
  private val coverWindow = CoverWindow()
  private val cardWindow = CardWindow()
  internal val core = ShowerCore(coverWindow, cardWindow, MainTimer(main))

  /** When the order being shown came, for the log of how long a cover took. */
  private var orderAt = 0L

  override fun show(cover: Cover) {
    orderAt = SystemClock.uptimeMillis()
    val name = try {
      childName()
    } catch (e: Exception) {
      null
    }
    core.show(Plans.of(cover, name, apps))
  }

  override fun warn(warning: WarningShown) {
    core.warn(WarningPlan.of(warning))
  }

  // The cover's window

  private inner class CoverWindow : CoverSurface {
    private var service: AccessibilityService? = null
    private var root: CoverRoot? = null
    private var plan: CoverPlan = CoverPlan.None

    override val up: Boolean
      get() = root?.let { it.attached && service === WatchService.current } ?: false

    override fun draw(plan: CoverPlan): Boolean {
      val svc = WatchService.current
      if (svc == null) {
        remove()
        return false
      }
      val r = root
      if (r != null && service === svc && r.attached) {
        r.show(plan)
        line(plan)
        this.plan = plan
        return true
      }
      remove()
      val fresh = CoverRoot(svc)
      fresh.show(plan)
      val lp = overlay(WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT, cover = true)
      fresh.timeFirstFrame(orderAt)
      try {
        svc.getSystemService(WindowManager::class.java).addView(fresh, lp)
      } catch (e: Exception) {
        Log.i(TAG, "show cover failed error=${e.javaClass.simpleName}")
        return false
      }
      service = svc
      root = fresh
      fresh.attached = true
      line(plan)
      this.plan = plan
      return true
    }

    override fun remove() {
      val r = root ?: return
      val svc = service
      root = null
      service = null
      plan = CoverPlan.None
      r.attached = false
      try {
        svc?.getSystemService(WindowManager::class.java)?.removeViewImmediate(r)
      } catch (e: Exception) {
        Log.i(TAG, "show uncover failed error=${e.javaClass.simpleName}")
      }
      Log.i(TAG, "show cover now nothing")
    }

    override fun refresh() {
      root?.tickClock()
    }

    private fun line(p: CoverPlan) {
      if (p == plan) return
      when (p) {
        is CoverPlan.Block -> Log.i(TAG, "show cover now block package=${p.packageName} call=${p.backToCall != null}")
        is CoverPlan.Lock -> Log.i(TAG, "show cover now lock why=${p.why.toString().lowercase()} rows=${p.rows.size} tiles=${p.tiles.size} call=${p.backToCall != null}")
        CoverPlan.None -> Unit
      }
    }
  }

  /**
   * The cover's root view: the screen of the plan, built again when the
   * plan changes or the configuration does (a turn, a fold, the font size,
   * the density), with Android's bars and cut out kept clear.
   */
  @SuppressLint("ViewConstructor")
  private inner class CoverRoot(service: AccessibilityService) : FrameLayout(themed(service)) {
    var attached = false
    private var plan: CoverPlan = CoverPlan.None
    private var screen: Screen? = null
    private var page: CoverPage? = null
    private var bars = intArrayOf(0, 0, 0, 0)

    init {
      setOnApplyWindowInsetsListener { _, insets ->
        bars = Bars.of(this, insets)
        screen?.insets(bars[0], bars[1], bars[2], bars[3])
        insets
      }
      addOnAttachStateChangeListener(object : OnAttachStateChangeListener {
        override fun onViewAttachedToWindow(v: View) = Unit

        override fun onViewDetachedFromWindow(v: View) {
          // Android took the window away (the service was switched off): the next order draws again.
          attached = false
        }
      })
    }

    fun show(plan: CoverPlan) {
      if (plan == this.plan && screen != null) return
      val keepScroll = this.plan::class == plan::class
      this.plan = plan
      build(keepScroll)
    }

    private fun build(keepScroll: Boolean) {
      val y = if (keepScroll) screen?.scroll?.scrollY ?: 0 else 0
      val parts = Parts(context, ShowAssets.of(context))
      val p = CoverPage(parts, actions)
      val (time, date) = clock()
      val s = p.build(plan, time, date)
      s.insets(bars[0], bars[1], bars[2], bars[3])
      if (bars.all { it == 0 }) {
        val f = Bars.fallback(context)
        s.insets(0, f[1], 0, f[3])
      }
      removeAllViews()
      addView(s, LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
      screen = s
      page = p
      if (y > 0) s.scroll.post { s.scroll.scrollTo(0, y) }
      requestApplyInsets()
    }

    fun tickClock() {
      val (time, date) = clock()
      page?.top?.set(time, date)
    }

    override fun onConfigurationChanged(newConfig: Configuration?) {
      super.onConfigurationChanged(newConfig)
      if (screen != null) build(keepScroll = true)
    }

    /**
     * The time from the order to the cover's first frame handed to the
     * display, for the log (Android 10 and later), as proof two measured it.
     */
    fun timeFirstFrame(started: Long) {
      if (Build.VERSION.SDK_INT < 29) return
      addOnAttachStateChangeListener(object : OnAttachStateChangeListener {
        override fun onViewAttachedToWindow(v: View) {
          removeOnAttachStateChangeListener(this)
          viewTreeObserver.registerFrameCommitCallback {
            Log.i(TAG, "show cover drawn ms=${SystemClock.uptimeMillis() - started}")
          }
        }

        override fun onViewDetachedFromWindow(v: View) = Unit
      })
    }
  }

  // The warning's window

  private inner class CardWindow : CardSurface {
    private var service: AccessibilityService? = null
    private var view: View? = null

    override val up: Boolean get() = view != null && service === WatchService.current

    override fun float(warning: WarningPlan): Boolean {
      remove()
      val svc = WatchService.current ?: return false
      val ctx = themed(svc)
      val parts = Parts(ctx, ShowAssets.of(ctx))
      val card = WarningView.build(parts, warning) { core.dismissWarning() }
      val lp = overlay(0, WindowManager.LayoutParams.WRAP_CONTENT, cover = false)
      // Room for the card's hard shadow, inside the window, and no more: every touch outside it goes to the app.
      val holder = object : FrameLayout(ctx) {
        override fun onConfigurationChanged(newConfig: Configuration?) {
          super.onConfigurationChanged(newConfig)
          place(svc, parts, lp)
          try {
            svc.getSystemService(WindowManager::class.java).updateViewLayout(this, lp)
          } catch (e: Exception) {
            Log.i(TAG, "show warning_moved failed error=${e.javaClass.simpleName}")
          }
        }
      }
      holder.clipChildren = false
      holder.clipToPadding = false
      holder.setPadding(0, 0, parts.px(Look.CARD_SHADOW), parts.px(Look.CARD_SHADOW))
      holder.addView(card, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
      place(svc, parts, lp)
      try {
        svc.getSystemService(WindowManager::class.java).addView(holder, lp)
      } catch (e: Exception) {
        Log.i(TAG, "show warning failed error=${e.javaClass.simpleName}")
        return false
      }
      service = svc
      view = holder
      Log.i(TAG, "show warning floated")
      return true
    }

    override fun remove() {
      val v = view ?: return
      val svc = service
      view = null
      service = null
      try {
        svc?.getSystemService(WindowManager::class.java)?.removeViewImmediate(v)
      } catch (e: Exception) {
        Log.i(TAG, "show warning_gone failed error=${e.javaClass.simpleName}")
      }
    }

    /** 12 from each side (at most 600 wide, in the middle), 8 under the status bar. */
    private fun place(svc: AccessibilityService, parts: Parts, lp: WindowManager.LayoutParams) {
      val screenWidth = svc.resources.displayMetrics.widthPixels
      val width = minOf(screenWidth - 2 * parts.px(Look.WARN_SIDE), parts.px(Look.PAGE_MOST))
      lp.width = width + parts.px(Look.CARD_SHADOW)
      lp.x = (screenWidth - width) / 2
      lp.y = Bars.top(svc) + parts.px(Look.WARN_TOP)
    }
  }

  // What the taps do

  private inner class Actions : CoverActions {
    // Each steps aside only once something opened: a start Android refused leaves the cover as it is.

    override fun home() {
      val svc = WatchService.current
      val done = svc?.performGlobalAction(AccessibilityService.GLOBAL_ACTION_HOME) ?: false
      val opened = done || start(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME))
      if (opened) core.stepAside()
      Log.i(TAG, "show home by=${if (done) "service" else if (opened) "intent" else "none"}")
    }

    override fun emergency() {
      // The watch learns which screen opens next, so it is never covered.
      try {
        Watch.of(app).emergencyDiallerOpening()
      } catch (e: Exception) {
        Log.i(TAG, "show failed what=emergency_watch error=${e.javaClass.simpleName}")
      }
      val how = Emergency.open(app)
      if (how != Emergency.NONE) core.stepAside()
      Log.i(TAG, "show emergency opened how=$how")
    }

    override fun backToCall() {
      val how = Emergency.backToCall(app)
      if (how != Emergency.NONE) core.stepAside()
      Log.i(TAG, "show back_to_call how=$how")
    }

    override fun openApp(packageName: String) {
      val launch = app.packageManager.getLaunchIntentForPackage(packageName) ?: return
      if (start(launch)) core.stepAside()
    }

    override fun openWay(way: Way, lock: CoverPlan.Lock) {
      if (start(WaysOutActivity.intent(app, way, lock))) core.stepAside()
    }

    override fun appIcon(packageName: String): Drawable? = apps.icon(packageName)

    private fun start(intent: Intent): Boolean = try {
      app.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
      true
    } catch (e: Exception) {
      Log.i(TAG, "show failed what=start error=${e.javaClass.simpleName}")
      false
    }
  }

  private fun overlay(width: Int, height: Int, cover: Boolean): WindowManager.LayoutParams {
    var flags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
      WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
      WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED
    if (!cover) flags = flags or WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL
    val lp = WindowManager.LayoutParams(
      width,
      height,
      WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
      flags,
      if (cover) PixelFormat.OPAQUE else PixelFormat.TRANSLUCENT,
    )
    lp.gravity = Gravity.TOP or Gravity.START
    lp.windowAnimations = 0
    // The window's title, which a screen reader may say: LiMo's name.
    lp.title = Drawn.LIMO
    if (Build.VERSION.SDK_INT >= 28) {
      lp.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
    }
    if (Build.VERSION.SDK_INT >= 30) {
      // Over Android's bars too, and laid out by LiMo itself around them.
      lp.fitInsetsTypes = 0
      lp.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
    }
    return lp
  }

  private fun clock(): Pair<String, String> {
    val now = System.currentTimeMillis()
    val zone = ZoneId.systemDefault()
    val h24 = try {
      DateFormat.is24HourFormat(app)
    } catch (e: Exception) {
      false
    }
    return ClockWords.time(now, zone, h24) to ClockWords.date(now, zone)
  }

  companion object {
    private const val TAG = "LiMo"

    @Volatile
    private var shower: CoverShower? = null

    /**
     * Makes the process's shower and gives it to the engine's host. Call it
     * once the host exists (where the keeper starts it); calling it again
     * does nothing. [childName] is read at every order and never logged.
     */
    @JvmStatic
    fun install(context: Context, childName: () -> String?) {
      val app = context.applicationContext ?: context
      val made = synchronized(this) {
        if (shower != null) return
        CoverShower(app, childName).also { shower = it }
      }
      Host.of(app).useShower(made)
    }

    /**
     * True when a warning can float over the app in front: the accessibility
     * service is connected, so the card is drawn. keep/ then posts the
     * warning's notification quietly, so the warning is not shown twice.
     */
    @JvmStatic
    fun canFloatWarning(): Boolean = WatchService.current != null

    /** From [WaysOutActivity], on the main thread. */
    internal fun limoScreen(up: Boolean) {
      shower?.core?.limoScreen(up)
    }

    internal fun themed(context: Context): Context = ContextThemeWrapper(context, android.R.style.Theme_Material_Light_NoActionBar)
  }
}

/** Android's bars and cut out, as the cover keeps clear of them: left, top, right, bottom in pixels. */
internal object Bars {
  fun of(view: View, insets: WindowInsets): IntArray {
    if (Build.VERSION.SDK_INT >= 30) {
      // The room of the bars even while an app in front hides them, and the cut out.
      val bars = insets.getInsetsIgnoringVisibility(WindowInsets.Type.systemBars())
      val cut = insets.getInsets(WindowInsets.Type.displayCutout())
      return intArrayOf(maxOf(bars.left, cut.left), maxOf(bars.top, cut.top), maxOf(bars.right, cut.right), maxOf(bars.bottom, cut.bottom))
    }
    @Suppress("DEPRECATION")
    val out = intArrayOf(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
    if (Build.VERSION.SDK_INT >= 28) {
      insets.displayCutout?.let { c ->
        out[0] = maxOf(out[0], c.safeInsetLeft)
        out[1] = maxOf(out[1], c.safeInsetTop)
        out[2] = maxOf(out[2], c.safeInsetRight)
        out[3] = maxOf(out[3], c.safeInsetBottom)
      }
    }
    if (out[1] == 0) out[1] = fallback(view.context)[1]
    return out
  }

  /** Android's own sizes of its bars, for a window that was told none. */
  @SuppressLint("DiscouragedApi", "InternalInsetResource")
  fun fallback(context: Context): IntArray {
    val res = context.resources
    fun dimen(name: String): Int {
      val id = res.getIdentifier(name, "dimen", "android")
      return if (id == 0) 0 else res.getDimensionPixelSize(id)
    }
    return intArrayOf(0, dimen("status_bar_height"), 0, dimen("navigation_bar_height"))
  }

  /** The top of the screen that LiMo keeps clear: the status bar and the cut out. */
  fun top(context: Context): Int {
    if (Build.VERSION.SDK_INT >= 30) {
      val wm = context.getSystemService(WindowManager::class.java)
      if (wm != null) {
        val i = wm.currentWindowMetrics.windowInsets
        return maxOf(
          i.getInsetsIgnoringVisibility(WindowInsets.Type.statusBars()).top,
          i.getInsets(WindowInsets.Type.displayCutout()).top,
        )
      }
    }
    return fallback(context)[1]
  }
}

/** The phone's apps, for the lock's tiles: names and icons as Android gives them, asked at most once a minute each. */
internal class AndroidApps(private val app: Context) : AppCatalog {
  private class Seen(val tile: TilePlan?, val atMs: Long)

  private val seen = HashMap<String, Seen>()
  private var diallerSeen: Pair<String?, Long>? = null

  override val ownPackage: String = app.packageName

  override val dialler: String?
    get() {
      val now = SystemClock.elapsedRealtime()
      diallerSeen?.let { (p, at) -> if (now - at < FRESH_MS) return p }
      val p = try {
        app.getSystemService(TelecomManager::class.java)?.defaultDialerPackage
      } catch (e: Exception) {
        null
      }
      diallerSeen = p to now
      return p
    }

  override fun tile(packageName: String): TilePlan? {
    val now = SystemClock.elapsedRealtime()
    seen[packageName]?.let { if (now - it.atMs < FRESH_MS) return it.tile }
    val pm = app.packageManager
    val tile = try {
      if (pm.getLaunchIntentForPackage(packageName) == null) {
        null
      } else {
        TilePlan(packageName, pm.getApplicationLabel(pm.getApplicationInfo(packageName, 0)).toString())
      }
    } catch (e: PackageManager.NameNotFoundException) {
      null
    }
    if (seen.size > MAX_SEEN) seen.clear()
    seen[packageName] = Seen(tile, now)
    return tile
  }

  fun icon(packageName: String): Drawable? = try {
    app.packageManager.getApplicationIcon(packageName)
  } catch (e: PackageManager.NameNotFoundException) {
    null
  }

  private companion object {
    const val FRESH_MS = 60_000L
    const val MAX_SEEN = 64
  }
}

/** Android's emergency dialler and the call's own screen. Neither ever dials. */
internal object Emergency {
  /** What [open] and [backToCall] say when nothing could be opened. */
  const val NONE = "none"

  /** The emergency dialler: Android's own action, then the older one, then the dialler, where an emergency number can be dialled too. */
  fun open(context: Context): String {
    val tries = listOf(
      "emergency_dialler" to Intent("com.android.phone.EmergencyDialer.DIAL"),
      "dial_emergency" to Intent("android.intent.action.DIAL_EMERGENCY"),
      "dial" to Intent(Intent.ACTION_DIAL),
    )
    for ((how, intent) in tries) {
      try {
        context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        return how
      } catch (e: Exception) {
        // The next way.
      }
    }
    return NONE
  }

  /**
   * The call's own screen where Android allows it (`showInCallScreen`, with
   * the Phone permission), and the dialler otherwise, which shows the call
   * on most phones.
   */
  fun backToCall(context: Context): String {
    if (context.checkSelfPermission(Manifest.permission.READ_PHONE_STATE) == PackageManager.PERMISSION_GRANTED) {
      try {
        context.getSystemService(TelecomManager::class.java)?.showInCallScreen(false)
        return "in_call_screen"
      } catch (e: Exception) {
        // The dialler, below.
      }
    }
    return try {
      context.startActivity(Intent(Intent.ACTION_DIAL).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
      "dial"
    } catch (e: Exception) {
      NONE
    }
  }
}

/** The shower's timer, on the main thread. */
internal class MainTimer(private val main: Handler) : ShowTimer {
  override fun after(ms: Long, block: () -> Unit): AutoCloseable {
    val r = Runnable(block)
    main.postDelayed(r, ms)
    return AutoCloseable { main.removeCallbacks(r) }
  }

  override fun nowMs(): Long = SystemClock.uptimeMillis()
}
