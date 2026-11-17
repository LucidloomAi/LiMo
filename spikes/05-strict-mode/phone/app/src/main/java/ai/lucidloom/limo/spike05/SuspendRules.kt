package ai.lucidloom.limo.spike05

/**
 * Which packages may be suspended. Pure, so it runs in plain JVM unit tests.
 *
 * These rules apply only to suspending. Lifting a suspension is always
 * allowed, for any package.
 */
object SuspendRules {

    /** "A short list". More than this is refused. */
    const val MAX_PACKAGES = 10

    /**
     * Never suspended, whatever the list says. Launchers, dialers, Settings,
     * keyboards and a few more are also looked up on the phone at run time
     * ([SystemPackages]); these fixed names catch the common ones even if a
     * lookup misses them. Suspending any of these could leave the phone hard
     * to use or hard to recover: the adb shell, for one, is how the PC gets
     * the phone back.
     */
    val NEVER_SUSPEND_FIXED: Set<String> = setOf(
        // Android itself, System UI and the adb shell.
        "android",
        "com.android.systemui",
        "com.android.shell",
        // Settings.
        "com.android.settings",
        // Calls and emergency.
        "com.android.phone",
        "com.android.server.telecom",
        "com.android.dialer",
        "com.google.android.dialer",
        "com.samsung.android.dialer",
        "com.android.incallui",
        "com.samsung.android.incallui",
        "com.android.emergency",
        "com.samsung.android.emergency",
        // Launchers.
        "com.sec.android.app.launcher",
        "com.android.launcher3",
        "com.google.android.apps.nexuslauncher",
        // Keyboards.
        "com.samsung.android.honeyboard",
        "com.google.android.inputmethod.latin",
        // Installing apps and granting permissions.
        "com.android.packageinstaller",
        "com.google.android.packageinstaller",
        "com.android.permissioncontroller",
        "com.google.android.permissioncontroller",
        // Google Play services.
        "com.google.android.gms",
    )

    private val PACKAGE_NAME = Regex("^[A-Za-z][A-Za-z0-9_]*(\\.[A-Za-z][A-Za-z0-9_]*)+$")

    /** A cheap syntax check: two or more dot separated Java style words. */
    fun isPlausiblePackageName(s: String): Boolean = PACKAGE_NAME.matches(s)

    /**
     * Splits "a.b, c.d e.f" on commas, semicolons and white space. Returns the
     * good names (in order, no repeats) and the rejected pieces.
     */
    fun parse(raw: String?): Pair<List<String>, List<String>> {
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
     * Why [pkg] may not be suspended, or null if it may.
     *
     * @param neverRuntime packages looked up on the phone that are never suspended.
     * @param launchable packages with an icon in the app drawer. Only those can
     *   be suspended: the point is that a blocked app cannot be opened, and a
     *   package with no icon is some part of the system.
     */
    fun refusal(pkg: String, ownPackage: String, neverRuntime: Set<String>, launchable: Set<String>): String? =
        when {
            pkg == ownPackage -> "this_app"
            pkg in NEVER_SUSPEND_FIXED -> "never_suspend"
            pkg in neverRuntime -> "never_suspend"
            pkg !in launchable -> "no_launcher_entry"
            else -> null
        }

    data class Choice(val accepted: List<String>, val refused: List<Pair<String, String>>)

    /** Parses [raw] and applies every rule. At most [MAX_PACKAGES] are accepted. */
    fun choose(raw: String?, ownPackage: String, neverRuntime: Set<String>, launchable: Set<String>): Choice {
        val (good, bad) = parse(raw)
        val accepted = ArrayList<String>()
        val refused = ArrayList<Pair<String, String>>()
        for (b in bad) refused += b to "bad_name"
        for (p in good) {
            val why = refusal(p, ownPackage, neverRuntime, launchable)
            when {
                why != null -> refused += p to why
                accepted.size >= MAX_PACKAGES -> refused += p to "too_many"
                else -> accepted += p
            }
        }
        return Choice(accepted, refused)
    }

    /** Packages this app suspended that are no longer on the wanted list. */
    fun toLift(suspendedByMe: Collection<String>, wanted: Collection<String>): List<String> =
        suspendedByMe.filter { it !in wanted }.distinct()

    /** "a,b" or "none", for the log. */
    fun joinList(list: Collection<String>): String = if (list.isEmpty()) "none" else list.joinToString(",")

    /** Reads a stored comma list back, dropping anything that is not a package name. */
    fun splitStored(s: String?): List<String> =
        s.orEmpty().split(',').map { it.trim() }.filter { it.isNotEmpty() && isPlausiblePackageName(it) }.distinct()
}
