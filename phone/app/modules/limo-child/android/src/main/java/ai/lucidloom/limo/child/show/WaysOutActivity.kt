package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.Way
import ai.lucidloom.limo.child.host.Host
import ai.lucidloom.limo.child.host.PhoneHost
import ai.lucidloom.limo.child.host.WayOut
import ai.lucidloom.limo.child.watch.Watch
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.View
import android.view.WindowInsets
import android.view.WindowManager
import android.window.OnBackInvokedDispatcher
import java.time.ZoneId

/**
 * The ways out of a lock, each a screen of its own (B1, B2, B3): LiMo's own
 * activity, because a keyboard cannot type into an overlay. It talks to the
 * engine only through the host's three calls ([PhoneHost.askForTime],
 * [PhoneHost.tryCode], [PhoneHost.tryPin]) and shows each answer in the
 * engine's words ([WaysOutModel]).
 *
 * - **Secrets.** The window is secure: it cannot be captured, and Android
 *   shows no picture of it in the recent apps list (where the manifest also
 *   leaves it out). A typed code or PIN is held only until the call, then
 *   wiped; never logged; never saved with the screen's state.
 * - **Back** goes to the lock. Leaving the screen any other way closes it.
 * - **The watch** is told when it is shown and hidden, and so is the cover,
 *   which stands aside while it is up.
 */
class WaysOutActivity : Activity() {
  private val main = Handler(Looper.getMainLooper())
  private lateinit var model: WaysOutModel
  private var page: WaysOutPage? = null
  private var callId: Int? = null
  private var closing = false
  private var backCallback: Any? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    if (Build.VERSION.SDK_INT >= 33) setRecentsScreenshotEnabled(false)
    val panel = panelOf(intent.getStringExtra(EXTRA_WAY))
    model = WaysOutModel(panel)
    if (panel != Panel.ASK) window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN)
    savedInstanceState?.let { s ->
      restoreFrom(s)
    }
    val reason = intent.getStringExtra(EXTRA_TITLE)?.let { title ->
      WayReason(pictureOf(intent.getStringExtra(EXTRA_PICTURE)), title, intent.getStringExtra(EXTRA_UNTIL))
    }
    val heading = intent.getStringExtra(EXTRA_HEADING) ?: when (panel) {
      Panel.ASK -> Drawn.ASK_TITLE
      Panel.CODE -> Drawn.CODE_TITLE
      Panel.PIN -> Drawn.PIN_TITLE
    }
    title = heading
    val parts = Parts(this, ShowAssets.of(this))
    val p = WaysOutPage(parts, model, reason, heading, Handlers())
    page = p
    edgeToEdge(p.screen)
    setContentView(p.screen)
    if (Build.VERSION.SDK_INT >= 33) {
      val cb = android.window.OnBackInvokedCallback { after(model.back()) }
      onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, cb)
      backCallback = cb
    }
  }

  override fun onResume() {
    super.onResume()
    tell { Watch.of(this).limoScreenShown(true) }
    CoverShower.limoScreen(up = true)
    val known = WaysOutCalls.relay.attach(callId) { id, answer ->
      if (id == callId) {
        callId = null
        after(model.answered(answer))
      }
    }
    if (!known && callId != null) {
      // The process was started again since the call: no answer will come.
      callId = null
      model.restore(model.saved(), waiting = false)
      page?.refresh()
    }
  }

  override fun onPause() {
    WaysOutCalls.relay.detach()
    tell { Watch.of(this).limoScreenShown(false) }
    if (!isChangingConfigurations) CoverShower.limoScreen(up = false)
    super.onPause()
  }

  override fun onStop() {
    super.onStop()
    // Gone from the screen another way than Back (Home, a call): closed, and what was typed wiped.
    if (!isChangingConfigurations && !isFinishing) close()
  }

  override fun onDestroy() {
    main.removeCallbacksAndMessages(null)
    model.wipe()
    if (Build.VERSION.SDK_INT >= 33) {
      (backCallback as? android.window.OnBackInvokedCallback)?.let { onBackInvokedDispatcher.unregisterOnBackInvokedCallback(it) }
    }
    super.onDestroy()
  }

  @Deprecated("Deprecated in Java")
  override fun onBackPressed() {
    after(model.back())
  }

  /** Only what is not secret: never the digits. */
  override fun onSaveInstanceState(outState: Bundle) {
    super.onSaveInstanceState(outState)
    val s = model.saved()
    outState.putString(KEY_ASK, s.ask.name)
    outState.putString(KEY_PIN, s.pin.name)
    s.message?.let {
      outState.putString(KEY_MESSAGE, it.text)
      outState.putBoolean(KEY_RIGHT, it.right)
    }
    callId?.let { outState.putInt(KEY_CALL, it) }
  }

  private fun restoreFrom(s: Bundle) {
    val saved = WaysOutModel.Saved(
      panel = model.panel,
      ask = s.getString(KEY_ASK)?.let { n -> AskChoice.entries.firstOrNull { it.name == n } } ?: AskChoice.MIN30,
      pin = s.getString(KEY_PIN)?.let { n -> PinChoice.entries.firstOrNull { it.name == n } } ?: PinChoice.ADD30,
      message = s.getString(KEY_MESSAGE)?.let { Message(it, s.getBoolean(KEY_RIGHT)) },
    )
    callId = if (s.containsKey(KEY_CALL)) s.getInt(KEY_CALL) else null
    model.restore(saved, waiting = callId != null)
  }

  private inner class Handlers : WaysHandlers {
    override fun back() = after(model.back())

    override fun submit() {
      val call = model.submit(System.currentTimeMillis() / 1_000L, ZoneId.systemDefault()) ?: return
      val id = WaysOutCalls.relay.begin()
      callId = id
      page?.refresh()
      val host: PhoneHost = Host.of(this@WaysOutActivity)
      val relay = WaysOutCalls.relay
      when (call) {
        is WayCall.Ask -> host.askForTime(call.minutes, call.reason) { relay.answered(id, it) }
        is WayCall.Code -> host.tryCode(call.typed) { relay.answered(id, it) }
        is WayCall.Pin -> host.tryPin(call.typed, call.action) { relay.answered(id, it) }
      }
    }

    override fun emergency() {
      tell { Watch.of(this@WaysOutActivity).emergencyDiallerOpening() }
      val how = Emergency.open(this@WaysOutActivity)
      Log.i(TAG, "show emergency opened how=$how from=ways_out")
    }
  }

  private fun after(a: After) {
    when (a) {
      After.Stay -> page?.refresh()
      After.Close -> close()
      After.CloseSoon -> {
        page?.refresh()
        main.postDelayed({ close() }, Look.OPEN_SHOWN_MS)
      }
    }
  }

  private fun close() {
    if (closing) return
    closing = true
    model.wipe()
    finish()
  }

  /** Drawn under Android's bars, keeping clear of them and of the keyboard, so the main button sits on it. */
  private fun edgeToEdge(screen: Screen) {
    if (Build.VERSION.SDK_INT >= 30) {
      // Android 15 and later draw an app aimed at them edge to edge anyway.
      @Suppress("DEPRECATION")
      if (Build.VERSION.SDK_INT < 35) window.setDecorFitsSystemWindows(false)
      screen.setOnApplyWindowInsetsListener { _, insets ->
        val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
        val ime = insets.getInsets(WindowInsets.Type.ime())
        screen.insets(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
        WindowInsets.CONSUMED
      }
    } else {
      @Suppress("DEPRECATION")
      window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
        View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
        View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or
        View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
      screen.setOnApplyWindowInsetsListener { _, insets ->
        @Suppress("DEPRECATION")
        screen.insets(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
        insets
      }
    }
  }

  private inline fun tell(block: () -> Unit) {
    try {
      block()
    } catch (e: Exception) {
      Log.i(TAG, "show failed what=watch error=${e.javaClass.simpleName}")
    }
  }

  companion object {
    private const val TAG = "LiMo"
    private const val EXTRA_WAY = "ai.lucidloom.limo.child.show.WAY"
    private const val EXTRA_PICTURE = "ai.lucidloom.limo.child.show.PICTURE"
    private const val EXTRA_TITLE = "ai.lucidloom.limo.child.show.TITLE"
    private const val EXTRA_UNTIL = "ai.lucidloom.limo.child.show.UNTIL"
    private const val EXTRA_HEADING = "ai.lucidloom.limo.child.show.HEADING"
    private const val KEY_ASK = "limo.ask"
    private const val KEY_PIN = "limo.pin"
    private const val KEY_MESSAGE = "limo.message"
    private const val KEY_RIGHT = "limo.right"
    private const val KEY_CALL = "limo.call"

    /** Opens a way out from the lock, with the lock's reason for the strip and the row's title for the heading. */
    internal fun intent(context: Context, way: Way, lock: CoverPlan.Lock): Intent {
      val i = Intent(context, WaysOutActivity::class.java)
      i.putExtra(EXTRA_WAY, way.name)
      i.putExtra(EXTRA_PICTURE, lock.picture.name)
      i.putExtra(EXTRA_TITLE, lock.title)
      lock.untilWords?.let { i.putExtra(EXTRA_UNTIL, it) }
      lock.rows.firstOrNull { it.way == way }?.let { i.putExtra(EXTRA_HEADING, it.title) }
      return i
    }

    /**
     * Opens asking for more time with no lock behind it (from the child's
     * Today screen): no strip at the top, the drawing's own title.
     */
    @JvmStatic
    fun openAsking(context: Context) {
      val i = Intent(context, WaysOutActivity::class.java).putExtra(EXTRA_WAY, Way.ASK.name)
      if (context !is Activity) i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      context.startActivity(i)
    }

    private fun panelOf(way: String?): Panel = when (way) {
      Way.CODE.name -> Panel.CODE
      Way.PIN.name -> Panel.PIN
      else -> Panel.ASK
    }

    private fun pictureOf(name: String?): Picture = Picture.entries.firstOrNull { it.name == name } ?: Picture.HOURGLASS
  }
}
