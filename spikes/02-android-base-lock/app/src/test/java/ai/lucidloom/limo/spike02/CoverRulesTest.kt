package ai.lucidloom.limo.spike02

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CoverRulesTest {

    private val own = "ai.lucidloom.limo.spike02"
    private val launcher = "com.example.launcher"
    private val keyboard = "com.example.keyboard"
    private val neverCover = setOf(own, launcher, "com.example.settings")
    private val blocked = setOf("com.android.chrome", "com.google.android.youtube")

    // which events count

    @Test
    fun ignoresOwnSystemUiKeyboardAndEmpty() {
        assertNull(CoverRules.frontPackageOf(own, own, setOf(keyboard)))
        assertNull(CoverRules.frontPackageOf("com.android.systemui", own, setOf(keyboard)))
        assertNull(CoverRules.frontPackageOf("com.samsung.android.app.cocktailbarservice", own, emptySet()))
        assertNull(CoverRules.frontPackageOf(keyboard, own, setOf(keyboard)))
        assertNull(CoverRules.frontPackageOf(null, own, emptySet()))
        assertNull(CoverRules.frontPackageOf("", own, emptySet()))
        assertNull(CoverRules.frontPackageOf("  ", own, emptySet()))
    }

    @Test
    fun launcherAndOrdinaryAppsCount() {
        assertEquals(launcher, CoverRules.frontPackageOf(launcher, own, setOf(keyboard)))
        assertEquals("com.android.chrome", CoverRules.frontPackageOf("com.android.chrome", own, setOf(keyboard)))
    }

    // what may be covered

    @Test
    fun neverCoversProtectedPackages() {
        assertFalse(CoverRules.mayCover(own, own, neverCover))
        assertFalse(CoverRules.mayCover(launcher, own, neverCover))
        assertFalse(CoverRules.mayCover("com.android.settings", own, emptySet()))
        assertFalse(CoverRules.mayCover("com.samsung.android.dialer", own, emptySet()))
        assertFalse(CoverRules.mayCover("com.samsung.android.incallui", own, emptySet()))
        assertFalse(CoverRules.mayCover("com.android.phone", own, emptySet()))
        assertFalse(CoverRules.mayCover("com.sec.android.app.launcher", own, emptySet()))
        assertFalse(CoverRules.mayCover("com.android.systemui", own, emptySet()))
        assertTrue(CoverRules.mayCover("com.android.chrome", own, neverCover))
    }

    @Test
    fun coversOnlyABlockedAppInFrontWhileActive() {
        fun cover(
            front: String?,
            active: Boolean = true,
            locked: Boolean = false,
            call: Boolean = false,
            list: Set<String> = blocked,
        ) = CoverRules.shouldCover(front, list, active, locked, call, own, neverCover)

        assertTrue(cover("com.android.chrome"))
        assertFalse(cover("com.android.chrome", active = false))
        assertFalse(cover("com.android.chrome", locked = true))
        assertFalse(cover("com.android.chrome", call = true))
        assertFalse(cover("com.example.notes"))
        assertFalse(cover(null))
        // A protected package is never covered, even if it is on the list.
        assertFalse(cover(launcher, list = blocked + launcher))
        assertFalse(cover("com.android.settings", list = blocked + "com.android.settings"))
        assertFalse(cover(own, list = blocked + own))
    }

    // the safety stop and the deadline rule

    @Test
    fun blockingActiveOnlyWithinTenMinutes() {
        val now = 1_000_000_000L
        assertFalse(CoverRules.isBlockingActive(0L, now))
        assertFalse(CoverRules.isBlockingActive(now, now))
        assertFalse(CoverRules.isBlockingActive(now - 1L, now))
        assertTrue(CoverRules.isBlockingActive(now + 1L, now))
        assertTrue(CoverRules.isBlockingActive(now + CoverRules.SAFETY_STOP_MS, now))
        // Further than 10 minutes ahead: only a clock change can do that. Fails open.
        assertFalse(CoverRules.isBlockingActive(now + CoverRules.SAFETY_STOP_MS + 1L, now))
    }

    @Test
    fun msLeftAndFormat() {
        val now = 5_000L
        assertEquals(0L, CoverRules.msLeft(0L, now))
        assertEquals(1_000L, CoverRules.msLeft(now + 1_000L, now))
        assertEquals("off", CoverRules.formatLeft(0L))
        assertEquals("10 min 0 s", CoverRules.formatLeft(CoverRules.SAFETY_STOP_MS))
        assertEquals("9 min 41 s", CoverRules.formatLeft(581_000L))
        // Rounds up, so the last second shows as 1 s, not 0 s.
        assertEquals("0 min 1 s", CoverRules.formatLeft(1L))
    }

    // the adb list

    @Test
    fun parsesPackageLists() {
        val (good, bad) = CoverRules.parsePackageList(" com.android.chrome,com.google.android.youtube ;com.android.chrome x 1bad.name ")
        assertEquals(listOf("com.android.chrome", "com.google.android.youtube"), good)
        assertEquals(listOf("x", "1bad.name"), bad)
        assertEquals(Pair(emptyList<String>(), emptyList<String>()), CoverRules.parsePackageList(null))
        assertEquals(Pair(emptyList<String>(), emptyList<String>()), CoverRules.parsePackageList(""))
    }

    // the numbers

    @Test
    fun medianAndSlowest() {
        assertNull(CoverRules.median(emptyList()))
        assertNull(CoverRules.slowest(emptyList()))
        assertEquals(137L, CoverRules.median(listOf(137L)))
        assertEquals(120L, CoverRules.median(listOf(300L, 100L, 120L)))
        assertEquals(110L, CoverRules.median(listOf(100L, 120L, 300L, 90L)))
        assertEquals(110L, CoverRules.median(listOf(100L, 121L)))
        assertEquals(300L, CoverRules.slowest(listOf(100L, 300L, 120L)))
    }
}
