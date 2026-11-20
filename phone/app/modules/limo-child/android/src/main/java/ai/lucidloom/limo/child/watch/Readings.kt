package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.CallState

// What the watch reads from Android, and the small pure rules that turn a
// reading into a fact. Nothing in this file calls Android: AndroidPhone does,
// and the JVM tests fake it.

/**
 * Everything the watch reads from Android, one reading per call.
 * [AndroidPhone] makes each with Android's own call, in a line or two; the
 * JVM tests fake it. Any reading may throw: [WatchCore] turns a failure into
 * that fact's fallback and a log line.
 *
 * No reading returns a window's title, its text, a class name or anything
 * else on the screen. A package name is the most any of them knows about an
 * app, and a label (the app's name under its icon) is only ever read for an
 * app, by its package.
 */
internal interface PhoneSource {
  /** LiMo's own package. */
  val ownPackage: String

  /** Android reports an activity stopped as well as paused (Android 10 and later). */
  val stopsReported: Boolean

  /** `System.currentTimeMillis()`. */
  fun wallMs(): Long

  /** `SystemClock.elapsedRealtime()`: since the phone started, counting deep sleep. */
  fun elapsedMs(): Long

  /** `Settings.Global.BOOT_COUNT`. */
  fun bootCount(): Int

  /** The phone's time zone at [wallMs], as seconds from UTC. */
  fun utcOffsetSecs(wallMs: Long): Int

  /** `DateFormat.is24HourFormat`. */
  fun hours24(): Boolean

  /** `Settings.Global.AUTO_TIME`: the network sets the clock. */
  fun autoTime(): Boolean

  /** `PowerManager.isInteractive`. */
  fun interactive(): Boolean

  /** `KeyguardManager.isKeyguardLocked`. */
  fun keyguardLocked(): Boolean

  /** `AudioManager.getMode()`. */
  fun audioMode(): Int

  /**
   * Android tells the telephony call state only to an app that holds the
   * Phone permission (`READ_PHONE_STATE`): Android 12 and later, for an app
   * aimed at them, as LiMo is. Before Android 12 it tells any app.
   */
  val callStateNeedsPhone: Boolean

  /**
   * `TelephonyManager.getCallState()`: ringing, or off the hook, for every
   * call on the phone, a call that rings with the ringer silenced among
   * them. Where [callStateNeedsPhone] and the Phone permission is not
   * granted, Android refuses it, so [WatchCore] asks only when it may.
   */
  fun telephonyCallState(): Int

  /** LiMo may read usage access (`AppOpsManager`, `OPSTR_GET_USAGE_STATS`). */
  fun usageAccess(): Boolean

  /** LiMo's accessibility service is switched on in Settings. */
  fun accessibilityEnabled(): Boolean

  /** `NotificationManager.areNotificationsEnabled`. */
  fun notificationsEnabled(): Boolean

  /**
   * Android's Phone permission is granted to LiMo: `checkSelfPermission`
   * for `READ_PHONE_STATE`. With it the telephony call state is read on
   * Android 12 and later too; without it the audio mode alone says there.
   */
  fun phoneGranted(): Boolean

  /** `PowerManager.isIgnoringBatteryOptimizations`. */
  fun batteryExempt(): Boolean

  /** `DevicePolicyManager.isAdminActive` for LiMo's device admin. */
  fun deviceAdmin(): Boolean

  /**
   * Android's usage events from [fromWallMs] up to [toWallMs] (not
   * including it), oldest first, only the kinds in [UsageKind]; null while
   * usage access is off.
   */
  fun usageEvents(fromWallMs: Long, toWallMs: Long): List<UsageEvent>?

  /** The home app: the one chosen, or every preinstalled one while none is. */
  fun homePackages(): Set<String>

  /** The default dialler and the system's dialler, from Android's telecom service. */
  fun diallerPackages(): Set<String>

  /** Preinstalled apps that draw the screen of a call (an in call service with a screen). */
  fun inCallScreens(): Set<String>

  /** The emergency dialler: preinstalled apps that handle the emergency dial actions. */
  fun emergencyPackages(): Set<String>

  /** Every keyboard (input method), and the one in use. */
  fun keyboardPackages(): Set<String>

  /** Every package with an icon a child can tap to open it. */
  fun launchablePackages(): Set<String>

  /** The same apps, each with its label as Android has it (null when it has none). Slow: it loads every label. */
  fun launchableApps(): Map<String, String?>

  /** An app's label from Android's package manager; null when Android does not know the package. */
  fun label(packageName: String): String?
}

/** What happened to an app's activity, in Android's usage events. Nothing else is read. */
internal enum class UsageKind {
  /** `ACTIVITY_RESUMED` (before Android 10, `MOVE_TO_FOREGROUND`, the same number). */
  RESUMED,

  /** `ACTIVITY_PAUSED` (before Android 10, `MOVE_TO_BACKGROUND`, which also meant stopped). */
  PAUSED,

  /** `ACTIVITY_STOPPED` (Android 10 and later): no longer on the screen. */
  STOPPED,

  /** `DEVICE_STARTUP` (Android 10 and later): the phone started. */
  STARTUP,
}

/**
 * One of Android's usage events as LiMo keeps it: the package, what
 * happened, and when, on the wall clock in milliseconds. Never the class
 * name of the activity.
 */
internal data class UsageEvent(val packageName: String, val kind: UsageKind, val atWallMs: Long)

/** The kind of a usage event from Android's number for it; null for the kinds LiMo does not read. */
internal fun usageKindOf(type: Int): UsageKind? = when (type) {
  1 -> UsageKind.RESUMED
  2 -> UsageKind.PAUSED
  23 -> UsageKind.STOPPED
  27 -> UsageKind.STARTUP
  else -> null
}

/**
 * The call state, from two readings.
 *
 * The audio mode (`AudioManager.getMode`, any Android, no permission):
 * Android's telecom service sets it to "ringtone" while a call rings aloud,
 * "in call" while a call is dialled, on or held, and "call screening" while a
 * screening service answers it. "In communication" is an internet call, such
 * as a voice chat in a game: it is not a call here, so a game's chat never
 * keeps a cover away.
 *
 * The telephony call state catches a call that rings silently, which the
 * audio mode may not. Android 8 to 11 tell it to any app; Android 12 and
 * later only with the Phone permission, so there the audio mode alone is the
 * fallback while the permission is not granted.
 */
internal object Calls {
  /** From `AudioManager.getMode()`. */
  fun fromAudioMode(mode: Int): CallState = when (mode) {
    1 -> CallState.RINGING // MODE_RINGTONE
    2 -> CallState.ONGOING // MODE_IN_CALL
    4 -> CallState.RINGING // MODE_CALL_SCREENING: not answered yet
    5 -> CallState.ONGOING // MODE_CALL_REDIRECT
    else -> CallState.IDLE // MODE_NORMAL, MODE_IN_COMMUNICATION, MODE_COMMUNICATION_REDIRECT, unknown
  }

  /** From `TelephonyManager.getCallState()`. */
  fun fromTelephony(state: Int): CallState = when (state) {
    1 -> CallState.RINGING // CALL_STATE_RINGING
    2 -> CallState.ONGOING // CALL_STATE_OFFHOOK
    else -> CallState.IDLE
  }

  /**
   * Both readings in one: a call ringing in either rings (so it can always
   * be answered), then a call on in either is on. Null when neither could
   * be read.
   */
  fun both(audio: CallState?, telephony: CallState?): CallState? = when {
    audio == null && telephony == null -> null
    audio == CallState.RINGING || telephony == CallState.RINGING -> CallState.RINGING
    audio == CallState.ONGOING || telephony == CallState.ONGOING -> CallState.ONGOING
    else -> CallState.IDLE
  }
}

/** Reading `Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES`. */
internal object AccessibilitySetting {
  /**
   * True when [value] (services separated by colons, each a package, a
   * slash and a class, the class written in full or starting with a dot)
   * names [className] in [packageName].
   */
  fun names(value: String?, packageName: String, className: String): Boolean {
    if (value.isNullOrBlank()) return false
    return value.split(':').any { entry ->
      val slash = entry.indexOf('/')
      if (slash <= 0) return@any false
      val pkg = entry.substring(0, slash).trim()
      val cls = entry.substring(slash + 1).trim()
      pkg == packageName && (cls == className || (cls.startsWith('.') && pkg + cls == className))
    }
  }
}

/**
 * An app's label as LiMo passes it on: trimmed, and null when it is empty or
 * is only the package name again (Android gives the package name for an app
 * with no label). The engine cleans it further before showing it.
 */
internal fun cleanLabel(raw: CharSequence?, packageName: String): String? {
  val label = raw?.toString()?.trim().orEmpty()
  return if (label.isEmpty() || label == packageName) null else label
}

/**
 * Where the watch writes its log lines. Package names may appear in them;
 * never a label, a window's title or text, nor an exception's message.
 */
internal fun interface WatchLog {
  fun line(text: String)
}
