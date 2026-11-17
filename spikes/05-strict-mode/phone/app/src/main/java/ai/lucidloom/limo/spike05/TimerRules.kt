package ai.lucidloom.limo.spike05

/**
 * The dead man's timer as pure arithmetic. No Android calls here, so it runs
 * in plain JVM unit tests.
 *
 * The deadline is kept on two clocks:
 *
 * - the wall clock (System.currentTimeMillis). It survives a restart, but a
 *   person can change it;
 * - the elapsed clock (SystemClock.elapsedRealtime). Nobody can change it,
 *   but it starts again from zero at every boot.
 *
 * The time left is always the smaller of the two. Moving the clock forward
 * ends the timer early; moving it back gains nothing. A deadline that seems
 * more than [MAX_MINUTES] ahead can only come from a clock change or a
 * damaged record, and it releases the phone at once: if in doubt, release.
 *
 * After a restart the old elapsed deadline means nothing. Then the time left
 * is the smaller of the wall clock's answer and what was left at the last
 * check before the restart, minus the time since this boot. The record is
 * then moved onto the new boot's elapsed clock.
 */
object TimerRules {

    const val MINUTE_MS = 60_000L
    const val DEFAULT_MINUTES = 30
    const val EXTEND_MINUTES = 30
    const val MAX_MINUTES = 120
    const val MAX_MS = MAX_MINUTES * MINUTE_MS

    /** After a release that did not finish, the next try comes this much later. */
    const val RETRY_MS = 60_000L

    /** The clocks at one moment. [bootCount] is -1 when it could not be read. */
    data class Clocks(val wallMs: Long, val elapsedMs: Long, val bootCount: Int)

    /**
     * A stored deadline. [wallMs] 0 means there is none. [elapsedMs] is the
     * deadline on the elapsed clock of boot [bootCount], and [checkedElapsedMs]
     * the elapsed clock at the last check in that boot.
     */
    data class Record(
        val wallMs: Long,
        val elapsedMs: Long,
        val bootCount: Int,
        val checkedElapsedMs: Long,
    ) {
        val isSet: Boolean get() = wallMs > 0L

        companion object {
            val NONE = Record(0L, 0L, -1, 0L)
        }
    }

    /** A new deadline [ms] from now, kept between 0 and [MAX_MS]. */
    fun startIn(now: Clocks, ms: Long): Record {
        val m = ms.coerceIn(0L, MAX_MS)
        return Record(now.wallMs + m, now.elapsedMs + m, now.bootCount, now.elapsedMs)
    }

    /** Whether [now] is in a later boot than the one the record was made in. */
    fun isNewBoot(r: Record, now: Clocks): Boolean =
        if (r.bootCount >= 0 && now.bootCount >= 0) {
            r.bootCount != now.bootCount
        } else {
            // Boot count unknown: the elapsed clock going backwards means a restart.
            now.elapsedMs < r.checkedElapsedMs
        }

    fun wallLeftMs(r: Record, now: Clocks): Long = r.wallMs - now.wallMs

    fun elapsedLeftMs(r: Record, now: Clocks): Long =
        if (isNewBoot(r, now)) {
            (r.elapsedMs - r.checkedElapsedMs) - now.elapsedMs
        } else {
            r.elapsedMs - now.elapsedMs
        }

    /** Milliseconds left: the smaller of the two clocks. Zero or less means due. */
    fun leftMs(r: Record, now: Clocks): Long = minOf(wallLeftMs(r, now), elapsedLeftMs(r, now))

    sealed class Decision {
        /** Not an admin and nothing stored: nothing to do. */
        data object Idle : Decision()

        /** Not an admin any more: forget the stored deadline. */
        data object Forget : Decision()

        /** Admin or owner and no deadline yet: start one. */
        data class Start(val record: Record) : Decision()

        /** Still running. [record] is the one to store, moved to this boot if needed. */
        data class Keep(val record: Record, val leftMs: Long) : Decision()

        /** Release the phone now. */
        data class Release(val reason: String) : Decision()
    }

    /** The check that runs every time any part of the app starts, and on every alarm. */
    fun decide(isAdmin: Boolean, isOwner: Boolean, r: Record, now: Clocks): Decision {
        if (!isAdmin && !isOwner) return if (r.isSet) Decision.Forget else Decision.Idle
        if (!r.isSet) return Decision.Start(startIn(now, DEFAULT_MINUTES * MINUTE_MS))
        val wallLeft = wallLeftMs(r, now)
        val elapsedLeft = elapsedLeftMs(r, now)
        if (wallLeft > MAX_MS || elapsedLeft > MAX_MS) return Decision.Release("deadline_too_far")
        val left = minOf(wallLeft, elapsedLeft)
        if (left <= 0L) {
            // Within one boot the two clocks agree unless someone moved the
            // wall clock. After a restart the wall clock is the only reference.
            val forward = wallLeft <= 0L && elapsedLeft > 0L && !isNewBoot(r, now)
            return Decision.Release(if (forward) "clock_moved_forward" else "deadline_passed")
        }
        val kept = if (isNewBoot(r, now)) {
            Record(r.wallMs, now.elapsedMs + left, now.bootCount, now.elapsedMs)
        } else {
            r.copy(checkedElapsedMs = now.elapsedMs)
        }
        return Decision.Keep(kept, left)
    }

    /**
     * The app was just made an active admin (Android sends this when
     * set-device-owner runs). A deadline still running is kept as it is, so
     * enabling the admin again can never extend it. Otherwise a fresh
     * [DEFAULT_MINUTES] starts: a stored deadline that has already passed is
     * left over from an earlier run that was ended from the PC.
     */
    fun onAdminEnabled(r: Record, now: Clocks): Record {
        if (r.isSet) {
            val left = leftMs(r, now)
            if (left in 1L..MAX_MS && wallLeftMs(r, now) <= MAX_MS) return r
        }
        return startIn(now, DEFAULT_MINUTES * MINUTE_MS)
    }

    sealed class Change {
        data class Done(val record: Record, val leftMs: Long) : Change()
        data class Refused(val reason: String) : Change()
    }

    /** Sets the time left to [minutes]. Only ever downward. 0 means release at the next check. */
    fun lowerTo(r: Record, now: Clocks, minutes: Int): Change {
        if (!r.isSet) return Change.Refused("no_deadline")
        if (minutes < 0 || minutes > MAX_MINUTES) return Change.Refused("out_of_range")
        val current = leftMs(r, now)
        val wanted = minutes * MINUTE_MS
        if (wanted >= current) return Change.Refused("not_lower")
        return Change.Done(startIn(now, wanted), wanted)
    }

    /** The explicit "extend 30 minutes" action. Never beyond [MAX_MINUTES] from now. */
    fun extend(r: Record, now: Clocks): Change {
        if (!r.isSet) return Change.Refused("no_deadline")
        val current = leftMs(r, now)
        if (current <= 0L) return Change.Refused("already_due")
        if (current >= MAX_MS) return Change.Refused("at_most")
        val wanted = minOf(current + EXTEND_MINUTES * MINUTE_MS, MAX_MS)
        return Change.Done(startIn(now, wanted), wanted)
    }

    /** Whole minutes left, rounded up, for the log; "none" without a deadline. */
    fun minutesText(leftMs: Long?): String =
        if (leftMs == null) "none" else ((leftMs.coerceAtLeast(0L) + MINUTE_MS - 1L) / MINUTE_MS).toString()

    /** "28 min 41 s", or "due now". */
    fun formatLeft(ms: Long): String {
        if (ms <= 0L) return "due now"
        val s = (ms + 999L) / 1000L
        return "${s / 60L} min ${s % 60L} s"
    }
}
