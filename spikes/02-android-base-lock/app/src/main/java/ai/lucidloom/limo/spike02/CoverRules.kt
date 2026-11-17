package ai.lucidloom.limo.spike02

/**
 * The pure rules of the spike: no Android calls here, so they run in plain
 * JVM unit tests.
 */
object CoverRules {

    /** Blocking turns itself off this long after it is switched on. */
    const val SAFETY_STOP_MS = 10L * 60L * 1000L

    /**
     * Window events from these packages never mean "the app in front changed":
     * they are drawn over whatever app is in front and then go away. System UI
     * draws the notification shade, quick settings, the volume panel, the
     * power menu and the lock screen. The Samsung names are One UI's Edge
     * panel, fingerprint overlay, screenshot toolbar and Game Booster panel;
     * they are best guesses from memory, and the "event" lines in the log
     * show which packages really fire on the test phone.
     * Keyboards are added at run time from the list of installed input methods.
     */
    val TRANSIENT_PACKAGES: Set<String> = setOf(
        "com.android.systemui",
        "com.samsung.android.app.cocktailbarservice",
        "com.samsung.android.biometrics.app.setting",
        "com.samsung.android.app.smartcapture",
        "com.samsung.android.game.gametools",
    )

    /**
     * Never covered, whatever the blocked list says, so the spike can never
     * trap the phone. Launchers, dialers and Settings are also looked up at
     * run time; these fixed names catch the in call screen, the emergency
     * dialer and Samsung's own names even if a lookup misses them.
     */
    val NEVER_COVER_FIXED: Set<String> = setOf(
        "com.android.settings",
        "com.samsung.accessibility",
        "com.android.phone",
        "com.android.server.telecom",
        "com.android.dialer",
        "com.google.android.dialer",
        "com.samsung.android.dialer",
        "com.android.incallui",
        "com.samsung.android.incallui",
        "com.android.emergency",
        "com.samsung.android.emergency",
        "com.sec.android.app.launcher",
    )

    /**
     * What a window state event means. Returns the package that is now in
     * front, or null when the event must be ignored (it changes nothing).
     *
     * @param ownPackage this app. Its own events are ignored here; the
     *   activity tells the service directly when it comes to the front, so the
     *   cover's own window can never be mistaken for a front change.
     * @param extraIgnored keyboards (input methods), found at run time.
     */
    fun frontPackageOf(eventPackage: String?, ownPackage: String, extraIgnored: Set<String>): String? {
        if (eventPackage.isNullOrBlank()) return null
        if (eventPackage == ownPackage) return null
        if (eventPackage in TRANSIENT_PACKAGES) return null
        if (eventPackage in extraIgnored) return null
        return eventPackage
    }

    /** Whether a package may ever be covered. */
    fun mayCover(pkg: String, ownPackage: String, neverCover: Set<String>): Boolean {
        if (pkg == ownPackage) return false
        if (pkg in NEVER_COVER_FIXED) return false
        if (pkg in neverCover) return false
        if (pkg in TRANSIENT_PACKAGES) return false
        return true
    }

    /** The whole decision: cover the app in front, or not. */
    fun shouldCover(
        front: String?,
        blocked: Set<String>,
        blockingActive: Boolean,
        keyguardLocked: Boolean,
        inPhoneCall: Boolean,
        ownPackage: String,
        neverCover: Set<String>,
    ): Boolean {
        if (!blockingActive) return false
        if (keyguardLocked) return false
        if (inPhoneCall) return false
        if (front == null) return false
        if (front !in blocked) return false
        return mayCover(front, ownPackage, neverCover)
    }

    /**
     * The deadline rule. Blocking is active only while the deadline is in the
     * future and no more than 10 minutes away. A deadline further away can
     * only come from a clock change, and then blocking is treated as off: it
     * fails open, never closed.
     */
    fun isBlockingActive(untilWallMs: Long, nowWallMs: Long): Boolean {
        val left = untilWallMs - nowWallMs
        return left in 1..SAFETY_STOP_MS
    }

    /** Milliseconds left before the safety stop, or 0 when not active. */
    fun msLeft(untilWallMs: Long, nowWallMs: Long): Long =
        if (isBlockingActive(untilWallMs, nowWallMs)) untilWallMs - nowWallMs else 0L

    /** "9 min 41 s", or "off". */
    fun formatLeft(ms: Long): String {
        if (ms <= 0L) return "off"
        val totalSeconds = (ms + 999L) / 1000L
        return "${totalSeconds / 60L} min ${totalSeconds % 60L} s"
    }

    private val PACKAGE_NAME = Regex("^[A-Za-z][A-Za-z0-9_]*(\\.[A-Za-z][A-Za-z0-9_]*)+$")

    /** A cheap syntax check: two or more dot separated Java style words. */
    fun isPlausiblePackageName(s: String): Boolean = PACKAGE_NAME.matches(s)

    /**
     * Splits "a.b, c.d e.f" on commas and white space. Returns the good names
     * (in order, no repeats) and the rejected pieces.
     */
    fun parsePackageList(raw: String?): Pair<List<String>, List<String>> {
        if (raw == null) return Pair(emptyList(), emptyList())
        val good = LinkedHashSet<String>()
        val bad = ArrayList<String>()
        for (piece in raw.split(',', ' ', '\t', '\n', ';')) {
            val p = piece.trim()
            if (p.isEmpty()) continue
            if (isPlausiblePackageName(p)) good.add(p) else bad.add(p)
        }
        return Pair(good.toList(), bad)
    }

    /**
     * Median in whole milliseconds. With an even count it is the mean of the
     * two middle values, rounded down. Null for an empty list.
     */
    fun median(values: List<Long>): Long? {
        if (values.isEmpty()) return null
        val sorted = values.sorted()
        val mid = sorted.size / 2
        return if (sorted.size % 2 == 1) sorted[mid] else (sorted[mid - 1] + sorted[mid]) / 2L
    }

    /** Largest value, or null for an empty list. */
    fun slowest(values: List<Long>): Long? = values.maxOrNull()
}
