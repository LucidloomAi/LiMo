package ai.lucidloom.limo.child.watch

/**
 * Which packages play which part on this phone, as Android said when the
 * watch last asked. LiMo holds no list of makers' package names for these:
 * each is asked of Android on the phone itself (see [PhoneSource]).
 */
internal data class PhoneRoles(
  /** The home app. Its windows are the home screen, and on most phones the list of recent apps. */
  val home: Set<String> = emptySet(),
  /** The default dialler, the system's dialler and the screens of a call. */
  val diallers: Set<String> = emptySet(),
  /** The emergency dialler. */
  val emergency: Set<String> = emptySet(),
  /** Keyboards: a keyboard's window is never an app coming to the front. */
  val keyboards: Set<String> = emptySet(),
  /** Every package with an icon to open it; null while Android has not said. */
  val launchable: Set<String>? = null,
)

/**
 * The framework's own packages, the same on every Android phone, whose
 * windows draw over the app in front and never mean that it left: System UI
 * (the notification shade, quick settings, the volume panel, the power menu,
 * the lock screen, and on older phones the list of recent apps), Android
 * itself (its dialogs and the app chooser), the chooser of Android 14 and
 * later, and the permission controller (the dialog an app opens to ask for a
 * permission). While one of these is up, the app under it stays in front.
 */
internal object Overlays {
  val FRAMEWORK: Set<String> = setOf(
    "android",
    "com.android.systemui",
    "com.android.intentresolver",
    "com.android.permissioncontroller",
    "com.google.android.permissioncontroller",
  )

  fun isOverlay(packageName: String): Boolean = packageName in FRAMEWORK
}

/** The watch's times and limits, in one place. */
internal object WatchTimes {
  /** After a window event usage has not yet confirmed, usage is read again at these ages. */
  val RECHECKS_MS = longArrayOf(100, 300, 1_000)

  /** A window event usage has not confirmed by this age is dropped: a late event, an overlay, a dialog. */
  const val CONFIRM_GRACE_MS = 1_000L

  /**
   * A guard raised by a window event (a package guarded whole, or a page by
   * its class) holds this long at most while usage shows nothing newer than
   * that window: long enough for a settings app started cold (2.2 seconds on
   * the Galaxy, 21 November 2026) to resume.
   */
  const val GUARD_WINDOW_HOLD_MS = 5_000L

  /**
   * A window event of a package whose activity usage shows paused or stopped
   * this little before it, with another app in front since, is a late one of
   * an app that already left: it raises no guard.
   */
  const val GUARD_LATE_WINDOW_MS = 5_000L

  /** [WatchCore.look] reads usage again when the last read is older than this. */
  const val USAGE_FRESH_MS = 500L

  /** How far back the first read of usage looks, and the read after the clock went back. */
  const val LOOKBACK_MS = 6L * 60 * 60 * 1_000

  /** Each read starts this much before the last one ended, for events Android wrote late. */
  const val OVERLAP_MS = 2_000L

  /** How often the home app, the diallers, the emergency dialler and the keyboards are asked again. */
  const val ROLES_EVERY_MS = 60_000L

  /** How often the launchable apps are asked again, besides when an app is added or removed. */
  const val LAUNCHABLE_EVERY_MS = 10L * 60 * 1_000

  /** How long the permissions read are trusted before they are read again. */
  const val PERMISSIONS_EVERY_MS = 5_000L

  /** For this long after the watch starts, a service switched on but not yet connected counts as on. */
  const val ACCESSIBILITY_START_MS = 60_000L

  /**
   * After a call rings, is dialled or is answered, the first screen to come up decides, if it comes within
   * this: it is learned as the call's screen only when it has no icon and is none of the known kinds.
   */
  const val CALL_SCREEN_WINDOW_MS = 5_000L

  /**
   * After LiMo opens the emergency dialler, the first screen to come up decides, if it comes within
   * this: it is learned as the emergency dialler only when it has no icon and is none of the known kinds.
   */
  const val EMERGENCY_WINDOW_MS = 10_000L

  /** A reading that keeps failing is logged at most this often. */
  const val LOG_REPEAT_MS = 60_000L

  /** At most this many screens learned for calls, and as many for the emergency dialler. */
  const val MAX_LEARNED = 8

  /** At most this many labels kept. */
  const val MAX_LABELS = 200
}
