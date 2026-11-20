package ai.lucidloom.limo.child.seam

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** What Android has to show for each step, on each Android from 8.0 (26) to 16 (36). */
class StepPlansTest {
  private val ask = StepWay::Ask
  private fun open(page: Page) = StepWay.Open(page)

  @Test
  fun notificationsAreAQuestionFromAndroid13AndLiMosNotificationSettingsBefore() {
    for (sdk in 26..32) {
      assertEquals(
        listOf(open(Page.APP_NOTIFICATIONS), open(Page.APP_DETAILS)),
        StepPlans.ways(Step.NOTIFICATIONS, sdk, granted = false, blocked = false),
      )
    }
    for (sdk in 33..36) {
      assertEquals(
        listOf(ask(StepPlans.POST_NOTIFICATIONS), open(Page.APP_NOTIFICATIONS), open(Page.APP_DETAILS)),
        StepPlans.ways(Step.NOTIFICATIONS, sdk, granted = false, blocked = false),
      )
      // Refused for good: Android will not ask, so its page.
      assertEquals(
        listOf(open(Page.APP_NOTIFICATIONS), open(Page.APP_DETAILS)),
        StepPlans.ways(Step.NOTIFICATIONS, sdk, granted = false, blocked = true),
      )
      // Granted, yet switched off in Settings: its page.
      assertEquals(
        listOf(open(Page.APP_NOTIFICATIONS), open(Page.APP_DETAILS)),
        StepPlans.ways(Step.NOTIFICATIONS, sdk, granted = true, blocked = false),
      )
    }
  }

  @Test
  fun thePhonePermissionIsAQuestionOnEveryAndroidUnlessRefusedForGood() {
    for (sdk in 26..36) {
      assertEquals(
        listOf(ask(StepPlans.READ_PHONE_STATE), open(Page.APP_DETAILS)),
        StepPlans.ways(Step.PHONE, sdk, granted = false, blocked = false),
      )
      assertEquals(listOf(open(Page.APP_DETAILS)), StepPlans.ways(Step.PHONE, sdk, granted = false, blocked = true))
      // Granted: Android answers at once, showing nothing.
      assertEquals(ask(StepPlans.READ_PHONE_STATE), StepPlans.ways(Step.PHONE, sdk, granted = true, blocked = false).first())
    }
  }

  @Test
  fun usageAccessOpensLiMosOwnPageFromAndroid10AndTheListBefore() {
    for (sdk in 26..28) assertEquals(listOf(open(Page.USAGE_LIST)), StepPlans.ways(Step.USAGE, sdk, granted = false, blocked = false))
    for (sdk in 29..36) {
      assertEquals(listOf(open(Page.USAGE_FOR_LIMO), open(Page.USAGE_LIST)), StepPlans.ways(Step.USAGE, sdk, granted = false, blocked = false))
    }
  }

  @Test
  fun accessibilityBatteryAndProtection() {
    for (sdk in 26..36) {
      assertEquals(listOf(open(Page.ACCESSIBILITY_LIST)), StepPlans.ways(Step.ACCESSIBILITY, sdk, granted = false, blocked = false))
      assertEquals(
        listOf(StepWay.Answer(Question.BATTERY), open(Page.BATTERY_LIST)),
        StepPlans.ways(Step.BATTERY, sdk, granted = false, blocked = false),
      )
      assertEquals(listOf(StepWay.Answer(Question.DEVICE_ADMIN)), StepPlans.ways(Step.PROTECTION, sdk, granted = false, blocked = false))
    }
    // Two request codes of their own, within the 16 bits AndroidX allows.
    val codes = Question.entries.map { StepPlans.requestCode(it) }
    assertEquals(codes.size, codes.toSet().size)
    assertTrue(codes.all { it in 1..0xFFFF })
  }

  @Test
  fun aPageAfterAQuestionOnlyWhenAndroidSaidNoWithoutAsking() {
    // Said no without showing anything: asked before, no reason to ask again was offered, and blocked now.
    assertTrue(StepPlans.pageAfterAnswer(granted = false, askedBefore = true, rationaleBefore = false, blockedAfter = true))
    // The person said no in the question (a second no on Android 11 and later blocks it): left as it is.
    assertFalse(StepPlans.pageAfterAnswer(granted = false, askedBefore = true, rationaleBefore = true, blockedAfter = true))
    // The very first question is always shown.
    assertFalse(StepPlans.pageAfterAnswer(granted = false, askedBefore = false, rationaleBefore = false, blockedAfter = true))
    assertFalse(StepPlans.pageAfterAnswer(granted = false, askedBefore = true, rationaleBefore = false, blockedAfter = false))
    assertFalse(StepPlans.pageAfterAnswer(granted = true, askedBefore = true, rationaleBefore = false, blockedAfter = false))
    assertEquals(Page.APP_DETAILS, StepPlans.pageOf(StepPlans.ways(Step.PHONE, 33, granted = false, blocked = false)))
    assertEquals(Page.APP_NOTIFICATIONS, StepPlans.pageOf(StepPlans.ways(Step.NOTIFICATIONS, 33, granted = false, blocked = false)))
  }

  @Test
  fun everyStepHasAWayOnEveryAndroid() {
    for (step in Step.entries) for (sdk in 26..36) for (granted in listOf(false, true)) for (blocked in listOf(false, true)) {
      assertTrue("$step $sdk", StepPlans.ways(step, sdk, granted, blocked).isNotEmpty())
    }
    assertEquals(listOf("notifications", "phone", "usage", "accessibility", "battery", "protection"), Step.entries.map { it.id })
    assertEquals(Step.USAGE, Step.ofEnginePermission("usage_access"))
    assertEquals(null, Step.ofId("usage_access"))
  }
}
