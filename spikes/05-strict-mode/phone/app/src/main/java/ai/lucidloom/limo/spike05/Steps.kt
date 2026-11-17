package ai.lucidloom.limo.spike05

/**
 * One thing strict mode turns on. The four user restrictions use Android's
 * own restriction names as their keys (the values of UserManager.DISALLOW_*),
 * and every key is also the name of its adb extra.
 */
enum class Step(val key: String, val label: String) {
    BLOCK_UNINSTALL("block_uninstall", "Uninstalling this app blocked"),
    NO_CONFIG_DATE_TIME("no_config_date_time", "Date and time locked"),
    NO_ADD_USER("no_add_user", "Adding users blocked"),
    NO_SAFE_BOOT("no_safe_boot", "Safe mode blocked"),
    SUSPEND_APPS("suspend_apps", "Listed apps suspended"),
    NO_FACTORY_RESET("no_factory_reset", "Factory reset from Settings blocked");

    /** True for the four Android user restrictions. */
    val isUserRestriction: Boolean get() = key.startsWith("no_")
}

/** The safe order of things. Pure, so it runs in plain JVM unit tests. */
object Steps {

    /**
     * Turning things on. Factory reset comes last: until the very end, a
     * tester who gets into trouble can still reset the phone from Settings.
     */
    val APPLY_ORDER: List<Step> = listOf(
        Step.BLOCK_UNINSTALL,
        Step.NO_CONFIG_DATE_TIME,
        Step.NO_ADD_USER,
        Step.NO_SAFE_BOOT,
        Step.SUSPEND_APPS,
        Step.NO_FACTORY_RESET,
    )

    /** Turning things off: the exact reverse, so factory reset comes back first. */
    val UNDO_ORDER: List<Step> = APPLY_ORDER.reversed()

    /** The names one step has in the release log. */
    fun releaseNames(step: Step): List<String> = when (step) {
        Step.SUSPEND_APPS -> listOf("unsuspend", "unsuspend_sweep")
        Step.BLOCK_UNINSTALL -> listOf("uninstall_block")
        else -> listOf(step.key)
    }

    /**
     * Every step of releaseEverything(), in order, as logged: everything this
     * app set, undone in reverse order, then Device Owner is cleared, then
     * the app removes itself as an active admin. Device Owner must go before
     * the admin: Android ignores a request to remove an admin that is still
     * owner.
     */
    val RELEASE_STEPS: List<String> = UNDO_ORDER.flatMap { releaseNames(it) } + listOf("device_owner", "active_admin")

    /**
     * The order to carry out a set of switch changes in. [applyAll] turns
     * everything on ("Apply strict mode"); a switch in [switches] overrides it
     * for that one item. Everything turned off comes first, in [UNDO_ORDER];
     * then everything turned on, in [APPLY_ORDER].
     */
    fun plan(applyAll: Boolean, switches: Map<Step, Boolean>): List<Pair<Step, Boolean>> {
        val wanted = LinkedHashMap<Step, Boolean>()
        if (applyAll) for (s in APPLY_ORDER) wanted[s] = true
        wanted.putAll(switches)
        val offs = UNDO_ORDER.filter { wanted[it] == false }.map { it to false }
        val ons = APPLY_ORDER.filter { wanted[it] == true }.map { it to true }
        return offs + ons
    }

    /**
     * A new list of packages to suspend also switches suspension on (or off,
     * when no package on it was accepted), unless the same request set that
     * switch itself.
     */
    fun withListChange(switches: Map<Step, Boolean>, listGiven: Boolean, acceptedCount: Int): Map<Step, Boolean> {
        if (!listGiven || Step.SUSPEND_APPS in switches) return switches
        return switches + (Step.SUSPEND_APPS to (acceptedCount > 0))
    }
}

/** The spike's fixed PIN. The real product uses the guardian's own PIN. */
object PinRule {
    const val SPIKE_PIN = "2468"

    fun matches(given: String?): Boolean = given != null && given.trim() == SPIKE_PIN
}

/** Log line shapes that the PC program reads. Pure, so they are unit tested. */
object LogText {
    fun yesNo(b: Boolean): String = if (b) "yes" else "no"

    fun state(admin: Boolean, owner: Boolean, leftMs: Long?): String =
        "state admin=${yesNo(admin)} owner=${yesNo(owner)} deadline_min=${TimerRules.minutesText(leftMs)}"

    fun releaseDone(owner: Boolean, admin: Boolean): String =
        "release done owner=${yesNo(owner)} admin=${yesNo(admin)}"

    /** An exception on one line: its class and message, no line breaks. */
    fun oneLine(e: Throwable): String =
        "${e.javaClass.simpleName}: ${e.message.orEmpty()}".replace('\n', ' ').replace('\r', ' ').trim()
}
