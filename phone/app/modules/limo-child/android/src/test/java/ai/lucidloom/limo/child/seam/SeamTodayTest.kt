package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.ffi.ALL_PERMISSIONS
import ai.lucidloom.limo.child.ffi.CASINO
import ai.lucidloom.limo.child.ffi.CLOCK
import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.ffi.GAME
import ai.lucidloom.limo.child.ffi.PIN
import ai.lucidloom.limo.child.ffi.PinAction
import ai.lucidloom.limo.child.ffi.PinActionKind
import ai.lucidloom.limo.child.ffi.TryKind
import ai.lucidloom.limo.child.ffi.WEDNESDAY_4_PM
import ai.lucidloom.limo.child.ffi.app
import ai.lucidloom.limo.child.ffi.screen
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The engine's REAL view, from a made up family through the bridge, turned
 * into the seam's answers for Today and How it works: the engine's words as
 * they are, its kinds and keys the seam's, each package its app's name, and
 * what is switched off as the reader wants it. Each answer is also written
 * for the screens' contract test.
 */
class SeamTodayTest {
  private fun rig(edit: RulesEdit? = null) = SeamPhoneRig(Files.createTempDirectory("limo-seam").toFile(), WEDNESDAY_4_PM, edit = edit)

  @Suppress("UNCHECKED_CAST")
  private fun apps(answer: Map<String, Any?>) = answer["apps"] as List<Map<String, Any?>>

  @Test
  fun aDayWithALimitInTheEnginesWordsWithEachAppByItsName() {
    rig().use { rig ->
      rig.use(app(GAME, "Game"), 12 * 60)
      rig.use(app(CLOCK, "Clock"), 2 * 60)
      // An app with no name counts all the same; it is only never listed.
      rig.use(app(NAMELESS, ""), 3 * 60)
      rig.use(screen(FrontKind.HOME), 5)
      val view = SeamAnswers.readView(rig.hostView().json!!)
      val today = SeamAnswers.today(view, NAMES)
      val raw = rig.raw("today")

      @Suppress("UNCHECKED_CAST")
      val rawLeft = raw["left"] as Map<String, Any?>
      assertEquals(mapOf("kind" to "left", "words" to rawLeft["words"]), today["left"])
      assertEquals("15 min", rawLeft["words"])
      assertEquals(raw["used"], today["used"])
      assertEquals("15 min used of 30 min", today["used"])
      assertEquals(raw["used_part"], today["usedPart"])
      assertEquals(0.5, today["usedPart"] as Double, 0.001)
      assertEquals(raw["where_used"], today["where"])
      assertNull(today["bedtime"])
      // Longest first, as the engine orders them; the nameless app left out, never shown as a package.
      assertEquals(listOf(mapOf("name" to "Game", "minutes" to "12 min"), mapOf("name" to "Clock", "minutes" to "2 min")), apps(today))
      @Suppress("UNCHECKED_CAST")
      val rawApps = (raw["apps"] as List<Map<String, Any?>>).map { it["package"] }
      assertEquals(listOf(GAME, NAMELESS, CLOCK), rawApps)
      assertTrue(today.containsKey("guardianNeeded"))
      assertNull(today["guardianNeeded"])
      SeamAnswerFiles.write("today-limit", "child.today", today)

      // The casino is not installed, so it has no name, and How it works leaves it out.
      val rules = SeamAnswers.rules(view, NAMES)
      assertEquals(linkedMapOf("timeADay" to "30 min", "screensOff" to null, "alwaysOpen" to listOf("Clock"), "notAllowed" to emptyList<String>()), rules)
      SeamAnswerFiles.write("rules-limit", "child.rules", rules)
      val withCasino = SeamAnswers.rules(view, NAMES + (CASINO to "Casino"))
      assertEquals(listOf("Casino"), withCasino["notAllowed"])
      SeamAnswerFiles.write("rules-limit-named", "child.rules", withCasino)
    }
  }

  @Test
  fun aDayWithNoLimitHasNoBarAndTheEnginesWords() {
    rig(NO_LIMIT).use { rig ->
      rig.use(app(GAME, "Game"), 5 * 60)
      val view = SeamAnswers.readView(rig.hostView().json!!)
      val today = SeamAnswers.today(view, NAMES)
      assertEquals(mapOf("kind" to "no-limit", "words" to "No limit today"), today["left"])
      assertTrue(today.containsKey("usedPart"))
      assertNull(today["usedPart"])
      assertEquals("5 min used today", today["used"])
      assertEquals(listOf(mapOf("name" to "Game", "minutes" to "5 min")), apps(today))
      SeamAnswerFiles.write("today-no-limit", "child.today", today)
      val rules = SeamAnswers.rules(view, NAMES)
      assertEquals("No limit", rules["timeADay"])
      SeamAnswerFiles.write("rules-no-limit", "child.rules", rules)
    }
  }

  @Test
  fun aGuardiansUnlockSaysOpenUntil() {
    rig().use { rig ->
      rig.use(app(GAME, "Game"), 30 * 60 + 5)
      // The day is used up: a guardian unlocks it until 6:00 pm with the PIN.
      val until = WEDNESDAY_4_PM + 2 * 3_600
      val answer = rig.engine.tryPin(PIN.toByteArray(), PinAction(PinActionKind.UNLOCK_UNTIL, until = until), rig.wall)
      assertEquals(TryKind.UNLOCKED, answer.kind)
      rig.use(app(GAME, "Game"), 2)
      val today = SeamAnswers.today(SeamAnswers.readView(rig.hostView().json!!), NAMES)
      assertEquals(mapOf("kind" to "open", "words" to "Open until 6:00 pm"), today["left"])
      SeamAnswerFiles.write("today-open", "child.today", today)
    }
  }

  @Test
  fun hoursOffAreSaidOnTodayAndInTheRules() {
    rig(BEDTIME).use { rig ->
      rig.use(app(GAME, "Game"), 60)
      val view = SeamAnswers.readView(rig.hostView().json!!)
      val today = SeamAnswers.today(view, NAMES)
      assertEquals("Screens are off from 9:00 pm to 7:00 am.", today["bedtime"])
      SeamAnswerFiles.write("today-bedtime", "child.today", today)
      val rules = SeamAnswers.rules(view, NAMES)
      assertEquals("9:00 pm to 7:00 am", rules["screensOff"])
      SeamAnswerFiles.write("rules-bedtime", "child.rules", rules)
      // On a phone that shows 24 hours, the engine says so itself.
      rig.hours24 = true
      rig.use(app(GAME, "Game"), 1)
      assertEquals("21:00 to 07:00", SeamAnswers.rules(SeamAnswers.readView(rig.hostView().json!!), NAMES)["screensOff"])
    }
  }

  @Test
  fun whatIsSwitchedOffComesAsTheStepsInSetupOrderWithTheEnginesSentences() {
    rig().use { rig ->
      rig.permissions = ALL_PERMISSIONS.copy(battery = false, phone = false, usageAccess = false)
      rig.use(app(GAME, "Game"), 90)
      val view = SeamAnswers.readView(rig.hostView().json!!)
      val today = SeamAnswers.today(view, NAMES)
      assertEquals(
        linkedMapOf(
          "off" to listOf("phone", "usage", "battery"),
          "words" to listOf(
            "Phone access is switched off, so a lock could hide a call.",
            "Usage access is switched off, so LiMo cannot count the time in each app.",
            "Android may put LiMo to sleep to save battery.",
          ),
        ),
        today["guardianNeeded"],
      )
      SeamAnswerFiles.write("today-switched-off", "child.today", today)

      // All six off: each in setup order, each with its own sentence.
      rig.permissions = ALL_PERMISSIONS.copy(
        notifications = false, phone = false, usageAccess = false, accessibility = false, battery = false, protection = false,
      )
      rig.use(app(GAME, "Game"), 1)
      @Suppress("UNCHECKED_CAST")
      val all = SeamAnswers.today(SeamAnswers.readView(rig.hostView().json!!), NAMES)["guardianNeeded"] as Map<String, List<String>>
      assertEquals(Step.entries.map { it.id }, all["off"])
      assertEquals(6, all["words"]!!.toSet().size)
      SeamAnswerFiles.write("today-all-off", "child.today", SeamAnswers.today(SeamAnswers.readView(rig.hostView().json!!), NAMES))

      // On again: nothing needed.
      rig.permissions = ALL_PERMISSIONS
      rig.use(app(GAME, "Game"), 1)
      assertNull(SeamAnswers.today(SeamAnswers.readView(rig.hostView().json!!), NAMES)["guardianNeeded"])
    }
  }

  @Test
  fun longNamesArePassedAsTheyAre() {
    rig().use { rig ->
      val longName = "The Very Long Name Of A Drawing App For Children Who Like Colours"
      rig.use(app(GAME, longName), 7 * 60)
      val today = SeamAnswers.today(SeamAnswers.readView(rig.hostView().json!!), mapOf(GAME to longName))
      assertEquals(listOf(mapOf("name" to longName, "minutes" to "7 min")), apps(today))
      SeamAnswerFiles.write("today-long-names", "child.today", today)
    }
  }

  @Test
  fun aViewThatIsNotTheEnginesIsRefusedNamingTheField() {
    fun problem(json: String): SeamProblem = try {
      SeamAnswers.today(SeamAnswers.readView(json), NAMES)
      error("not refused")
    } catch (e: SeamProblem) {
      e
    }
    assertEquals(SeamProblem.BAD_VIEW, problem("not json").code)
    assertEquals("The engine's view has no usable view.today.", problem("{}").message)
    val newKind = """{"today":{"left":{"kind":"later","words":"x"},"used":"","used_part":null,"where_used":"","bedtime":null,"apps":[],"guardian_needed":null}}"""
    assertEquals("The engine's view has no usable today.left.kind.", problem(newKind).message)
    val emptySentence = newKind.replace("later", "left").replace("\"guardian_needed\":null", "\"guardian_needed\":{\"off\":[{\"permission\":\"phone\",\"words\":\" \"}]}")
    assertEquals("The engine's view has no usable today.guardian_needed.off[0].words.", problem(emptySentence).message)
    // A permission a newer engine knows and this seam does not is left out with its sentence.
    val newer = emptySentence.replace("\"words\":\" \"", "\"words\":\"Something new.\"").replace("\"phone\"", "\"camera\"")
    assertNull(SeamAnswers.today(SeamAnswers.readView(newer), NAMES)["guardianNeeded"])
  }
}
