package ai.lucidloom.limo.child.ffi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * A short day on the real engine, from Kotlin: paired from a made up
 * welcome, counting in an app, a blocked app covered, the time used up and
 * the lock ordered, a right code opening it, the state kept, and a new
 * engine loaded from the kept bytes giving the same orders.
 */
class EngineDayTest {
  @get:Rule val folder = TemporaryFolder()

  /** The orders without what only the engine that kept the state can know. */
  private fun comparable(orders: PhoneOrders) = orders.copy(saveState = false)

  @Test
  fun aShortDayFromAMadeUpWelcome() {
    FakePhone(folder.newFolder(), WEDNESDAY_4_PM).use { phone ->
      // Counting in a game, out of reach of the master.
      var orders = phone.run(60)
      assertEquals(CoverKind.NOTHING, orders.cover.kind)
      assertTrue(orders.timeLeft.counting)
      assertTrue(orders.timeLeft.away)
      assertEquals(30L * 60 - 60, orders.timeLeft.secondsLeft)
      assertEquals("29 min left", orders.timeLeft.words)
      assertEquals(60L, phone.counted())
      assertFalse(orders.sendReport)

      // A blocked app is covered, named by its label, and does not count.
      phone.front = app(CASINO, "Casino")
      orders = phone.run(10)
      assertEquals(CoverKind.BLOCK, orders.cover.kind)
      val block = orders.cover.block!!
      assertEquals(CASINO, block.packageName)
      assertEquals("Casino is not allowed.", block.title)
      assertEquals("It is blocked by your family's rules.", block.message)
      assertTrue(block.goHome)
      assertNull(orders.cover.lock)
      assertFalse(orders.timeLeft.counting)
      assertEquals(60L, phone.counted())
      // The guardian is told, by the app's package, once; the notes cross
      // as the engine's JSON, for the host's log.
      val noted = phone.notes.filter { it.contains("blocked_app_opened") }
      assertEquals(1, noted.size)
      assertTrue(noted[0].contains(CASINO))

      // Back to the game until the time is used up: three warnings, then the lock.
      phone.front = app(GAME, "Game")
      val warnings = mutableListOf<Int>()
      orders = phone.tick()
      while (orders.cover.kind == CoverKind.NOTHING) {
        phone.wall += 1
        phone.elapsedMs += 1_000
        orders = phone.tick()
        orders.warning?.let { warnings += it.minutes }
        if (warnings.size > 10) fail("too many warnings")
      }
      assertEquals(listOf(15, 5, 1), warnings)
      assertEquals(30L * 60, phone.counted())
      assertEquals(CoverKind.LOCK, orders.cover.kind)
      val lock = orders.cover.lock!!
      assertEquals(LockWhy.TIME_USED_UP, lock.why)
      assertEquals("Today's time is used up.", lock.title)
      assertEquals("Locked", lock.stamp)
      assertEquals(listOf(CLOCK), lock.allowedPackages)
      assertTrue(lock.canUseCode)
      assertTrue(lock.ways.any { it.way == Way.CODE })
      assertTrue(orders.timeLeft.locked)
      assertFalse(orders.timeLeft.counting)

      // The clock stays usable under the lock; the diallers and LiMo are never covered; home is.
      phone.front = app(CLOCK, "Clock")
      assertEquals(CoverKind.NOTHING, phone.run(5).cover.kind)
      assertEquals(30L * 60, phone.counted())
      for (kind in listOf(FrontKind.DIALLER, FrontKind.EMERGENCY_DIALLER, FrontKind.LIMO)) {
        phone.front = screen(kind)
        assertEquals(kind.name, CoverKind.NOTHING, phone.tick().cover.kind)
      }
      phone.front = screen(FrontKind.HOME)
      assertEquals(CoverKind.LOCK, phone.tick().cover.kind)

      // A right code, read out by a guardian who is away, opens it. The
      // code crosses as bytes, which the host then fills with zeros.
      phone.front = screen(FrontKind.LIMO)
      val code = phone.family.makeCode(phone.deviceId, 15, phone.wall).toByteArray()
      val answer = phone.engine.tryCode(code, phone.wall)
      code.fill(0)
      assertEquals(TryKind.ADDED, answer.kind)
      assertEquals(15L * 60, answer.addedSecs)
      assertEquals("15 minutes added.", answer.message)
      assertEquals("The phone opens now.", answer.opensNow)
      assertTrue(phone.engine.shouldSave())
      phone.kept = phone.engine.save()

      phone.front = app(GAME, "Game")
      orders = phone.run(30)
      assertEquals(CoverKind.NOTHING, orders.cover.kind)
      assertTrue(orders.timeLeft.counting)
      assertEquals(30L * 60 + 30, phone.counted())

      // The state kept; a new engine loaded from those bytes gives the same orders.
      phone.kept = phone.engine.save()
      val reloaded = PhoneEngine.load(phone.kept.copyOf())
      assertNull(reloaded.loadProblem())
      phone.wall += 1
      phone.elapsedMs += 1_000
      val before = phone.engine.tick(phone.facts())
      val after = reloaded.tick(phone.facts())
      assertEquals(comparable(before), comparable(after))
      // It cannot know when its bytes were last written, so it asks at once.
      assertTrue(after.saveState)
      assertEquals(phone.counted(), reloaded.view()!!.countedSecs)

      // And the day goes on from it.
      phone.engine.close()
      phone.engine = reloaded
      orders = phone.run(60)
      assertTrue(orders.timeLeft.counting)
      assertEquals(30L * 60 + 31 + 60, phone.counted())
    }
  }

  @Test
  fun inReachTheReportGoesOutAndTheGrantComesBackAsBytes() {
    FakePhone(folder.newFolder(), WEDNESDAY_7_AM).use { phone ->
      phone.inReach = true
      val orders = phone.run(3)
      assertFalse(orders.timeLeft.away)
      assertTrue(orders.timeLeft.counting)
      val report = phone.engine.outgoingReport(phone.wall)
      assertNotNull(report)
      val grant = phone.family.answer(report!!, phone.wall)
      assertEquals(GrantOutcome.TAKEN, phone.engine.takeGrant(grant, phone.wall).outcome)
      assertEquals(GrantOutcome.STALE, phone.engine.takeGrant(grant, phone.wall).outcome)
      try {
        phone.engine.takeGrant("not a grant".toByteArray(), phone.wall)
        fail("bytes that are not a grant were taken")
      } catch (e: EngineException.NotAGrant) {
        assertEquals("What was handed in as a grant is not one.", e.message)
      }
    }
  }

  @Test
  fun theCoresRefusalsArriveAsTheirOwnExceptionsWithTheirWords() {
    FakePhone(folder.newFolder(), WEDNESDAY_4_PM).use { phone ->
      phone.run(1)
      val request = phone.engine.askForTime(20, "For the school project", phone.wall)
      assertEquals(20, request.minutes)
      assertEquals(phone.deviceId, request.deviceId)
      try {
        phone.engine.askForTime(10, "", phone.wall)
        fail("a second request was made")
      } catch (e: EngineException.RequestWaiting) {
        assertEquals("A request from this device is still waiting for an answer.", e.message)
      }
      try {
        phone.engine.tick(phone.facts().copy(front = Front(FrontKind.APP)))
        fail("an app with no package was taken")
      } catch (e: EngineException.OutOfRange) {
        assertTrue(e.message!!.contains("no package name"))
      }
      // The PIN crosses as bytes, which the host fills with zeros after.
      val typed = "111111".toByteArray()
      val wrong = phone.engine.tryPin(typed, PinAction(PinActionKind.ADD_MINUTES, minutes = 20), phone.wall)
      typed.fill(0)
      assertEquals(TryKind.WRONG, wrong.kind)
      assertEquals(1, wrong.wrongGuesses)
      val right = PIN.toByteArray()
      val added = phone.engine.tryPin(right, PinAction(PinActionKind.ADD_MINUTES, minutes = 20), phone.wall)
      right.fill(0)
      assertEquals(TryKind.ADDED, added.kind)
      assertEquals(20L * 60, added.addedSecs)
      try {
        phone.engine.tryPin(PIN.toByteArray(), PinAction(PinActionKind.ADD_MINUTES), phone.wall)
        fail("a PIN to add minutes with none was taken")
      } catch (e: EngineException.OutOfRange) {
        assertTrue(e.message!!.contains("minutes to add are missing"))
      }
    }
  }

  @Test
  fun aKeptStateThatCannotBeReadLocksThePhoneAndIsNeverWrittenOver() {
    val engine = PhoneEngine.load(ByteArray(0))
    engine.use {
      assertEquals(LoadProblemKind.EMPTY, it.loadProblem()!!.kind)
      val orders = it.tick(
        PhoneFacts(
          wallSecs = WEDNESDAY_4_PM,
          elapsedMs = 1_000,
          bootCount = 1,
          utcOffsetSecs = IST,
          hours24 = true,
          autoTime = true,
          interactive = true,
          keyguardLocked = false,
          call = CallState.IDLE,
          front = app(GAME, "Game"),
          inReach = false,
          prodded = false,
          permissions = ALL_PERMISSIONS,
        ),
      )
      assertEquals(LockWhy.CANNOT_CHECK, orders.cover.lock!!.why)
      assertEquals("Ask a guardian.", orders.cover.lock!!.nothingToDo!!.bold)
      assertFalse(orders.saveState)
      assertFalse(it.shouldSave())
      assertEquals(0, it.save().size)
      assertNull(it.view())
    }
  }

  @Test
  fun theViewCarriesTodayTheRulesAndWhatIsSwitchedOffAsTheEnginesJson() {
    FakePhone(folder.newFolder(), WEDNESDAY_4_PM).use { phone ->
      phone.run(60)
      // The child opens LiMo; the Phone permission is not granted.
      phone.front = screen(FrontKind.LIMO)
      phone.permissions = ALL_PERMISSIONS.copy(phone = false)
      phone.run(1)
      val json = phone.engine.view()!!.json
      for (part in listOf(
        """"today":{"left":{"kind":"left","words":"29 min"},"used":"1 min used of 30 min","used_part":0.033,""",
        """"where_used":"On this phone 1 min","bedtime":null,""",
        """"apps":[{"package":"$GAME","secs":60,"minutes":"1 min"}],""",
        """"guardian_needed":{"off":[{"permission":"phone","words":"Phone access is switched off, so a lock could hide a call."}]}""",
        """"rules_words":{"time_a_day":"30 min","screens_off":null,"always_open":["$CLOCK"],"not_allowed":["$CASINO"]}""",
      )) {
        assertTrue("$part is not in $json", json.contains(part))
      }
      // The guardian is told it was switched off, by its name only.
      assertEquals(1, phone.notes.count { it.contains("\"permission_off\"") && it.contains("\"phone\"") })
      // Granted again: no guardian needed.
      phone.permissions = ALL_PERMISSIONS
      phone.run(1)
      assertTrue(phone.engine.view()!!.json.contains(""""guardian_needed":null"""))
    }
  }

  @Test
  fun setupCountsAndCoversNothingUntilAGuardianFinishesIt() {
    FakePhone(folder.newFolder(), WEDNESDAY_4_PM, finished = false).use { phone ->
      // A game and the blocked casino while the guardian sets the phone up.
      var orders = phone.run(60)
      assertEquals(CoverKind.NOTHING, orders.cover.kind)
      assertFalse(orders.timeLeft.counting)
      assertEquals("Setup is not finished on this phone.", orders.timeLeft.words)
      phone.front = app(CASINO, "Casino")
      assertEquals(CoverKind.NOTHING, phone.run(5).cover.kind)
      assertEquals(0L, phone.counted())
      val view = phone.engine.view()!!
      assertFalse(view.setupFinished)
      assertTrue(view.json, view.json.contains(""""setup":{"finished":false,"finished_at":null,"words":"Setup is not finished on this phone."}"""))

      // Finished, from Kotlin: kept at once, covering and counting from the next call.
      assertTrue(phone.engine.finishSetup(phone.wall))
      assertTrue(phone.engine.shouldSave())
      phone.kept = phone.engine.save()
      assertTrue(phone.engine.view()!!.setupFinished)
      assertEquals(CoverKind.BLOCK, phone.run(1).cover.kind)
      phone.front = app(GAME, "Game")
      phone.tick()
      orders = phone.run(30)
      assertTrue(orders.timeLeft.counting)
      assertEquals(30L, phone.counted())

      // Again: nothing changes, nothing to keep; and it stays finished in the kept bytes.
      phone.kept = phone.engine.save()
      assertFalse(phone.engine.finishSetup(phone.wall))
      assertFalse(phone.engine.shouldSave())
      PhoneEngine.load(phone.kept.copyOf()).use { assertTrue(it.view()!!.setupFinished) }
    }
  }

  @Test
  fun theLibraryNamesItsVersions() {
    val v = versions()
    assertEquals("0.1.0", v.ffi)
    assertEquals(1, v.linkProtocol)
  }
}
