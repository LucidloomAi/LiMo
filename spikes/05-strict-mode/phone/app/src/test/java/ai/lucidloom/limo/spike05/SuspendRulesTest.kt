package ai.lucidloom.limo.spike05

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SuspendRulesTest {

    private val own = "ai.lucidloom.limo.spike05"
    private val runtimeNever = setOf(own, "com.example.launcher", "com.example.keyboard", "com.example.sms")
    private val launchable = setOf(
        "com.sec.android.app.popupcalculator",
        "com.android.chrome",
        "com.google.android.youtube",
        "com.example.launcher",
        "com.android.settings",
        own,
    ) + (1..12).map { "com.example.app$it" }

    @Test
    fun parsesPackageLists() {
        val (good, bad) = SuspendRules.parse(" com.android.chrome,com.google.android.youtube ;com.android.chrome x 1bad.name ")
        assertEquals(listOf("com.android.chrome", "com.google.android.youtube"), good)
        assertEquals(listOf("x", "1bad.name"), bad)
        assertEquals(Pair(emptyList<String>(), emptyList<String>()), SuspendRules.parse(null))
        assertEquals(Pair(emptyList<String>(), emptyList<String>()), SuspendRules.parse(""))
        assertEquals(listOf("a.b", "c.d"), SuspendRules.parse("a.b\tc.d\n").first)
    }

    @Test
    fun neverSuspendsProtectedPackages() {
        fun why(p: String) = SuspendRules.refusal(p, own, runtimeNever, launchable)
        assertEquals("this_app", why(own))
        assertEquals("never_suspend", why("com.android.settings"))
        assertEquals("never_suspend", why("com.android.shell"))
        assertEquals("never_suspend", why("com.android.systemui"))
        assertEquals("never_suspend", why("com.samsung.android.dialer"))
        assertEquals("never_suspend", why("com.sec.android.app.launcher"))
        assertEquals("never_suspend", why("com.samsung.android.honeyboard"))
        assertEquals("never_suspend", why("com.google.android.gms"))
        assertEquals("never_suspend", why("android"))
        // Looked up on the phone at run time.
        assertEquals("never_suspend", why("com.example.launcher"))
        assertEquals("never_suspend", why("com.example.keyboard"))
        // No icon in the app drawer.
        assertEquals("no_launcher_entry", why("com.example.background"))
        assertNull(why("com.sec.android.app.popupcalculator"))
        assertNull(why("com.android.chrome"))
    }

    @Test
    fun chooseAppliesEveryRuleAndTheLimit() {
        val c = SuspendRules.choose(
            "com.android.chrome,com.android.settings,bad,$own,com.example.background",
            own,
            runtimeNever,
            launchable,
        )
        assertEquals(listOf("com.android.chrome"), c.accepted)
        assertEquals(
            listOf(
                "bad" to "bad_name",
                "com.android.settings" to "never_suspend",
                own to "this_app",
                "com.example.background" to "no_launcher_entry",
            ),
            c.refused,
        )

        val many = (1..12).joinToString(",") { "com.example.app$it" }
        val m = SuspendRules.choose(many, own, runtimeNever, launchable)
        assertEquals(SuspendRules.MAX_PACKAGES, m.accepted.size)
        assertEquals(listOf("com.example.app11" to "too_many", "com.example.app12" to "too_many"), m.refused)

        assertTrue(SuspendRules.choose("", own, runtimeNever, launchable).accepted.isEmpty())
    }

    @Test
    fun liftsWhatIsNoLongerWanted() {
        assertEquals(listOf("a.b"), SuspendRules.toLift(listOf("a.b", "c.d"), listOf("c.d", "e.f")))
        assertEquals(emptyList<String>(), SuspendRules.toLift(emptyList(), listOf("c.d")))
        assertEquals(listOf("a.b", "c.d"), SuspendRules.toLift(listOf("a.b", "c.d", "a.b"), emptyList()))
    }

    @Test
    fun storedListsRoundTrip() {
        assertEquals("none", SuspendRules.joinList(emptyList()))
        assertEquals("a.b,c.d", SuspendRules.joinList(listOf("a.b", "c.d")))
        assertEquals(listOf("a.b", "c.d"), SuspendRules.splitStored("a.b, c.d,,a.b,not a name"))
        assertEquals(emptyList<String>(), SuspendRules.splitStored(null))
        assertEquals(emptyList<String>(), SuspendRules.splitStored(""))
    }
}
