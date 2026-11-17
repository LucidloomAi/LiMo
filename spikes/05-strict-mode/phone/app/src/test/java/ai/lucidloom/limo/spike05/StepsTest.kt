package ai.lucidloom.limo.spike05

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StepsTest {

    @Test
    fun factoryResetIsRestrictedLastAndReleasedFirst() {
        assertEquals(Step.NO_FACTORY_RESET, Steps.APPLY_ORDER.last())
        assertEquals(Step.NO_FACTORY_RESET, Steps.UNDO_ORDER.first())
        assertEquals("no_factory_reset", Steps.RELEASE_STEPS.first())
    }

    @Test
    fun applyOrderIsExact() {
        assertEquals(
            listOf("block_uninstall", "no_config_date_time", "no_add_user", "no_safe_boot", "suspend_apps", "no_factory_reset"),
            Steps.APPLY_ORDER.map { it.key },
        )
        // Every step exactly once.
        assertEquals(Step.entries.toSet(), Steps.APPLY_ORDER.toSet())
        assertEquals(Step.entries.size, Steps.APPLY_ORDER.size)
    }

    @Test
    fun releaseOrderIsExact() {
        assertEquals(
            listOf(
                "no_factory_reset",
                "unsuspend",
                "unsuspend_sweep",
                "no_safe_boot",
                "no_add_user",
                "no_config_date_time",
                "uninstall_block",
                "device_owner",
                "active_admin",
            ),
            Steps.RELEASE_STEPS,
        )
    }

    @Test
    fun releaseUndoesInReverseThenClearsOwnerThenAdmin() {
        val r = Steps.RELEASE_STEPS
        // Unsuspend before every other restriction except factory reset.
        assertTrue(r.indexOf("unsuspend") < r.indexOf("no_safe_boot"))
        // Every restriction before the uninstall block is lifted.
        for (k in listOf("no_factory_reset", "no_safe_boot", "no_add_user", "no_config_date_time")) {
            assertTrue(k, r.indexOf(k) < r.indexOf("uninstall_block"))
        }
        // Owner cleared after everything it set, and before the admin goes.
        assertTrue(r.indexOf("uninstall_block") < r.indexOf("device_owner"))
        assertEquals(r.size - 2, r.indexOf("device_owner"))
        assertEquals(r.size - 1, r.indexOf("active_admin"))
        // Each name once.
        assertEquals(r.size, r.toSet().size)
    }

    @Test
    fun userRestrictionsUseAndroidNames() {
        val restrictions = Step.entries.filter { it.isUserRestriction }.map { it.key }.toSet()
        // The values of UserManager.DISALLOW_SAFE_BOOT, DISALLOW_FACTORY_RESET,
        // DISALLOW_ADD_USER and DISALLOW_CONFIG_DATE_TIME.
        assertEquals(setOf("no_safe_boot", "no_factory_reset", "no_add_user", "no_config_date_time"), restrictions)
        assertFalse(Step.BLOCK_UNINSTALL.isUserRestriction)
        assertFalse(Step.SUSPEND_APPS.isUserRestriction)
    }

    @Test
    fun applyAllPlansEverythingOnInOrder() {
        assertEquals(Steps.APPLY_ORDER.map { it to true }, Steps.plan(true, emptyMap()))
        assertEquals(emptyList<Pair<Step, Boolean>>(), Steps.plan(false, emptyMap()))
    }

    @Test
    fun offsComeFirstInUndoOrderThenOnsInApplyOrder() {
        val plan = Steps.plan(
            false,
            mapOf(
                Step.NO_SAFE_BOOT to true,
                Step.NO_FACTORY_RESET to false,
                Step.BLOCK_UNINSTALL to true,
                Step.SUSPEND_APPS to false,
            ),
        )
        assertEquals(
            listOf(
                Step.NO_FACTORY_RESET to false,
                Step.SUSPEND_APPS to false,
                Step.BLOCK_UNINSTALL to true,
                Step.NO_SAFE_BOOT to true,
            ),
            plan,
        )
    }

    @Test
    fun aSwitchOverridesApplyAll() {
        val plan = Steps.plan(true, mapOf(Step.NO_FACTORY_RESET to false))
        assertEquals(Step.NO_FACTORY_RESET to false, plan.first())
        assertFalse(plan.contains(Step.NO_FACTORY_RESET to true))
        assertEquals(Steps.APPLY_ORDER.size, plan.size)
    }

    @Test
    fun aNewListSwitchesSuspensionUnlessToldOtherwise() {
        assertEquals(mapOf(Step.SUSPEND_APPS to true), Steps.withListChange(emptyMap(), true, 2))
        assertEquals(mapOf(Step.SUSPEND_APPS to false), Steps.withListChange(emptyMap(), true, 0))
        assertEquals(emptyMap<Step, Boolean>(), Steps.withListChange(emptyMap(), false, 2))
        assertEquals(mapOf(Step.SUSPEND_APPS to false), Steps.withListChange(mapOf(Step.SUSPEND_APPS to false), true, 2))
    }

    @Test
    fun pinIsChecked() {
        assertTrue(PinRule.matches("2468"))
        assertTrue(PinRule.matches(" 2468 "))
        assertFalse(PinRule.matches("1234"))
        assertFalse(PinRule.matches(""))
        assertFalse(PinRule.matches(null))
    }

    @Test
    fun logLineShapes() {
        assertEquals("state admin=yes owner=yes deadline_min=30", LogText.state(true, true, 30 * TimerRules.MINUTE_MS))
        assertEquals("state admin=no owner=no deadline_min=none", LogText.state(false, false, null))
        assertEquals("release done owner=no admin=no", LogText.releaseDone(owner = false, admin = false))
        assertEquals("SecurityException: not owner", LogText.oneLine(SecurityException("not\nowner")))
    }
}
