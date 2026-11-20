package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.ffi.GAME
import ai.lucidloom.limo.child.ffi.LoadProblemKind
import ai.lucidloom.limo.child.ffi.WEDNESDAY_4_PM
import ai.lucidloom.limo.child.ffi.app
import ai.lucidloom.limo.child.host.HostView
import ai.lucidloom.limo.child.host.WayOut
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The requests the seam answers with data, as `LimoChildModule` hands them
 * over: the pairing and the child's name, the guardian's agreement and the
 * introduction kept apart from the engine's state, the steps as Android
 * reads them now, and what happens when the phone is not paired.
 */
class SeamRequestsTest {
  private fun rig() = SeamPhoneRig(Files.createTempDirectory("limo-seam").toFile(), WEDNESDAY_4_PM)

  private class Told : (SeamChange) -> Unit {
    val all = mutableListOf<SeamChange>()

    override fun invoke(change: SeamChange) {
      all += change
    }
  }

  private fun refused(seam: ChildSeam, request: String): String = try {
    seam.answer(request)
    error("$request was answered")
  } catch (e: SeamProblem) {
    e.code
  }

  @Test
  fun notPairedThePhoneIsNotSetUpAndOnlyAndroidsOwnAnswersAreGiven() {
    val marks = MemoryMarks()
    val told = Told()
    val seam = ChildSeam(RigHost(null), RigPhone(), marks, changed = told)
    val pairing = seam.answer("child.pairing")
    assertEquals(mapOf("paired" to false), pairing)
    SeamAnswerFiles.write("pairing-not-paired", "child.pairing", pairing)
    assertEquals(false, seam.answer("child.agreed"))
    assertEquals(false, seam.answer("child.introSeen"))
    for (request in listOf("child.agree", "child.markIntroSeen", "child.today", "child.rules")) {
      assertEquals(request, SeamProblem.NOT_PAIRED, refused(seam, request))
    }
    assertEquals(0, marks.writes)
    assertTrue(told.all.isEmpty())
    // What Android says needs no pairing.
    assertEquals("on", (seam.answer("child.steps") as Map<*, *>)["battery"])
    assertEquals("samsung", seam.answer("child.maker"))
    SeamAnswerFiles.write("maker", "child.maker", seam.answer("child.maker"))
    assertEquals(SeamProblem.UNKNOWN_REQUEST, refused(seam, "child.later"))
  }

  @Test
  fun pairedTheChildIsNamedAndSetupIsKeptForThisPairingOnly() {
    rig().use { rig ->
      val host = RigHost(rig, name = "Mira")
      val marks = MemoryMarks()
      val told = Told()
      val seam = ChildSeam(host, RigPhone(), marks, changed = told)
      val pairing = seam.answer("child.pairing")
      assertEquals(mapOf("paired" to true, "childName" to "Mira"), pairing)
      SeamAnswerFiles.write("pairing-paired", "child.pairing", pairing)
      assertEquals(false, seam.answer("child.agreed"))
      SeamAnswerFiles.write("agreed-not-yet", "child.agreed", seam.answer("child.agreed"))

      assertEquals(null, seam.answer("child.agree"))
      assertEquals(true, seam.answer("child.agreed"))
      SeamAnswerFiles.write("agreed", "child.agreed", seam.answer("child.agreed"))
      assertEquals(false, seam.answer("child.introSeen"))
      assertEquals(null, seam.answer("child.markIntroSeen"))
      assertEquals(true, seam.answer("child.introSeen"))
      SeamAnswerFiles.write("intro-seen", "child.introSeen", seam.answer("child.introSeen"))
      assertEquals(listOf(SeamChange.AGREEMENT, SeamChange.INTRO), told.all)
      // Kept in the seam's own file, by this pairing's device id, apart from the engine's state and the name.
      assertEquals("""{"device":"${rig.deviceId}","agreed":true,"intro_seen":true}""", marks.text)

      // Paired again (another welcome, another device id, another name): setup starts again.
      rig().use { again ->
        host.rig = again
        host.name = "Asha"
        assertNotEquals(rig.deviceId, again.deviceId)
        assertEquals(mapOf("paired" to true, "childName" to "Asha"), seam.answer("child.pairing"))
        assertEquals(false, seam.answer("child.agreed"))
        assertEquals(false, seam.answer("child.introSeen"))
      }
    }
  }

  @Test
  fun pairedWithNoNameKeptTheChildIsYourChildAndTheLogSaysSoOnce() {
    rig().use { rig ->
      val lines = mutableListOf<String>()
      val host = RigHost(rig, name = null)
      val seam = ChildSeam(host, RigPhone(), MemoryMarks(), log = SeamLog { lines += it })
      // Never "not paired" while an engine runs: the screens still read, with plain words for the name.
      val pairing = seam.answer("child.pairing")
      assertEquals(mapOf("paired" to true, "childName" to "your child"), pairing)
      SeamAnswerFiles.write("pairing-no-name", "child.pairing", pairing)
      assertEquals(pairing, seam.answer("child.pairing"))
      assertEquals(listOf("seam pairing no_child_name fallback"), lines)
      // Setup goes on as with a name.
      seam.answer("child.agree")
      assertEquals(true, seam.answer("child.agreed"))
      // The name comes back (paired again): the child is named, and a later loss is logged again.
      host.name = "Mira"
      assertEquals(mapOf("paired" to true, "childName" to "Mira"), seam.answer("child.pairing"))
      host.name = null
      seam.answer("child.pairing")
      assertEquals(2, lines.size)
    }
  }

  @Test
  fun aKeptStateThatCannotBeReadIsStillPairedButHasNothingToShow() {
    rig().use { rig ->
      val host = RigHost(rig)
      val marks = MemoryMarks()
      val seam = ChildSeam(host, RigPhone(), marks)
      seam.answer("child.agree")
      host.given = HostView(paired = true, json = null, problem = LoadProblemKind.DAMAGED)
      // The engine runs (and locks the phone): paired, and the marks kept are taken as they are.
      assertEquals(mapOf("paired" to true, "childName" to "Mira"), seam.answer("child.pairing"))
      assertEquals(true, seam.answer("child.agreed"))
      assertEquals(SeamProblem.CANNOT_READ, refused(seam, "child.today"))
      assertEquals(SeamProblem.CANNOT_READ, refused(seam, "child.rules"))
      assertEquals(SeamProblem.CANNOT_READ, refused(seam, "child.markIntroSeen"))
      assertEquals(1, marks.writes)
    }
  }

  @Test
  fun theDoneScreensButtonFinishesSetupOnTheEngineSoCountingStartsThere() {
    SeamPhoneRig(Files.createTempDirectory("limo-seam").toFile(), WEDNESDAY_4_PM, finished = false).use { rig ->
      val host = RigHost(rig)
      val lines = mutableListOf<String>()
      val seam = ChildSeam(host, RigPhone(), MemoryMarks(), log = SeamLog { lines += it })
      seam.answer("child.agree")
      // Two minutes of setup in a game and on Android's pages: Today lists nothing, nothing is used.
      rig.use(app(GAME, "Game"), 60)
      rig.use(app("com.android.settings", "Settings"), 60)
      @Suppress("UNCHECKED_CAST")
      val during = seam.answer("child.today") as Map<String, Any?>
      assertEquals(emptyList<Any>(), during["apps"])
      assertEquals("0 min used of 30 min", during["used"])
      assertFalse(rig.engine.view()!!.setupFinished)

      // "Show Mira how it works": the engine has it, once.
      val answer = seam.answer("child.finishSetup")
      assertEquals(null, answer)
      SeamAnswerFiles.write("setup-finished", "child.finishSetup", answer)
      assertTrue(rig.engine.view()!!.setupFinished)
      assertEquals(listOf("seam setup finished"), lines)
      rig.use(app(GAME, "Game"), 60)
      @Suppress("UNCHECKED_CAST")
      val after = seam.answer("child.today") as Map<String, Any?>
      assertEquals(listOf(mapOf("name" to "Game", "minutes" to "1 min")), after["apps"])

      // Tapped again, or the introduction seen after it: nothing changes, nothing more is logged.
      assertEquals(null, seam.answer("child.finishSetup"))
      assertEquals(null, seam.answer("child.markIntroSeen"))
      assertEquals(3, host.finishes)
      assertEquals(listOf("seam setup finished"), lines)
    }
  }

  @Test
  fun theIntroductionSeenFinishesSetupWhenTheButtonsRequestWasLost() {
    SeamPhoneRig(Files.createTempDirectory("limo-seam").toFile(), WEDNESDAY_4_PM, finished = false).use { rig ->
      val seam = ChildSeam(RigHost(rig), RigPhone(), MemoryMarks())
      seam.answer("child.agree")
      assertFalse(rig.engine.view()!!.setupFinished)
      assertEquals(null, seam.answer("child.markIntroSeen"))
      assertTrue(rig.engine.view()!!.setupFinished)
      assertEquals(true, seam.answer("child.introSeen"))
    }
  }

  @Test
  fun setupCannotBeFinishedWhenNotPairedOrWhenTheKeptStateCannotBeRead() {
    assertEquals(SeamProblem.NOT_PAIRED, refused(ChildSeam(RigHost(null), RigPhone(), MemoryMarks()), "child.finishSetup"))
    rig().use { rig ->
      val host = RigHost(rig)
      val seam = ChildSeam(host, RigPhone(), MemoryMarks())
      host.given = HostView(paired = true, json = null, problem = LoadProblemKind.DAMAGED)
      assertEquals(SeamProblem.CANNOT_READ, refused(seam, "child.finishSetup"))
      // The host's answer that never came.
      val silent = object : SeamHost {
        override fun view(): HostView = rig.hostView()

        override fun childName(): String = "Mira"

        override fun finishSetup(): WayOut<Boolean> = WayOut.Failed
      }
      assertEquals(SeamProblem.FAILED, refused(ChildSeam(silent, RigPhone(), MemoryMarks()), "child.finishSetup"))
    }
  }

  @Test
  fun eachStepIsOnOnlyWhenAndroidSaysSoInThisReading() {
    val phone = RigPhone()
    val seam = ChildSeam(RigHost(null), phone, MemoryMarks())
    val none = allOn().copy(
      notifications = false, phone = false, usageAccess = false, accessibility = false,
      accessibilityRunning = false, batteryExempt = false, deviceAdmin = false,
    )
    val alone = mapOf(
      "notifications" to none.copy(notifications = true),
      "phone" to none.copy(phone = true),
      "usage" to none.copy(usageAccess = true),
      "accessibility" to none.copy(accessibility = true),
      "battery" to none.copy(batteryExempt = true),
      "protection" to none.copy(deviceAdmin = true),
    )
    for ((id, permissions) in alone) {
      phone.permissions = permissions
      val steps = seam.answer("child.steps") as Map<*, *>
      assertEquals(Step.entries.map { it.id }, steps.keys.toList())
      for (step in Step.entries) assertEquals("$id alone, $step", if (step.id == id) "on" else "off", steps[step.id])
    }
    // Accessibility is the switch in Android's settings, which is what the guardian turns on;
    // the service connected without it switched on is not on.
    phone.permissions = none.copy(accessibilityRunning = true)
    assertEquals("off", (seam.answer("child.steps") as Map<*, *>)["accessibility"])
    // Asked of Android every time: a reading that changed is the answer at once.
    phone.permissions = none
    assertTrue((seam.answer("child.steps") as Map<*, *>).values.all { it == "off" })
    SeamAnswerFiles.write("steps-none", "child.steps", seam.answer("child.steps"))
    phone.permissions = allOn().copy(batteryExempt = false, deviceAdmin = false)
    SeamAnswerFiles.write("steps-some", "child.steps", seam.answer("child.steps"))
    phone.permissions = allOn()
    assertTrue((seam.answer("child.steps") as Map<*, *>).values.all { it == "on" })
    SeamAnswerFiles.write("steps-all", "child.steps", seam.answer("child.steps"))
  }

  @Test
  fun theAnswersForTheChangeEventAskTheHostAndTheWatchOnceForAll() {
    rig().use { rig ->
      rig.use(app(GAME, "Game"), 60)
      var views = 0
      val host = object : SeamHost {
        override fun view(): HostView = rig.hostView().also { views += 1 }

        override fun childName(): String = "Mira"

        override fun finishSetup(): WayOut<Boolean> = WayOut.Done(false)
      }
      val phone = RigPhone()
      val seam = ChildSeam(host, phone, MemoryMarks())
      views = 0
      val answers = seam.answersFor(SeamChange.entries)
      assertEquals(SeamChange.entries, answers.keys.toList())
      assertEquals(1, views)
      assertEquals(1, phone.namesAsked)
      assertEquals(seam.answer("child.today"), answers[SeamChange.TODAY])
      assertEquals(seam.answer("child.pairing"), answers[SeamChange.PAIRING])
      // Not paired: Today and the rules are a fixed word, and a failure is another.
      val unpaired = ChildSeam(RigHost(null), phone, MemoryMarks()).answersFor(listOf(SeamChange.TODAY, SeamChange.PAIRING))
      assertTrue(unpaired[SeamChange.TODAY] === ChildSeam.NOT_PAIRED)
      val failing = object : SeamHost {
        override fun view(): HostView = throw SeamProblem(SeamProblem.NO_ANSWER, "no answer")

        override fun childName(): String? = null

        override fun finishSetup(): WayOut<Boolean> = throw SeamProblem(SeamProblem.NO_ANSWER, "no answer")
      }
      val failed = ChildSeam(failing, phone, MemoryMarks()).answersFor(listOf(SeamChange.PAIRING, SeamChange.STEPS))
      assertTrue(failed[SeamChange.PAIRING] === ChildSeam.FAILED)
      assertFalse(failed[SeamChange.STEPS] === ChildSeam.FAILED)
    }
  }

  @Test
  fun theMarksFileReadsBackWhatItWroteAndNothingElse() {
    val marks = SetupMarks("d-1 \"2\" \\ \u00e9", agreed = true, introSeen = false)
    assertEquals(marks, FileMarks.parse(FileMarks.text(marks)))
    assertEquals(SetupMarks("d-1", agreed = false, introSeen = false), FileMarks.parse("""{"device":"d-1"}"""))
    for (bad in listOf("", "not marks", "[]", """{"agreed":true}""", """{"device":7}""")) assertEquals(bad, null, FileMarks.parse(bad))
  }
}
