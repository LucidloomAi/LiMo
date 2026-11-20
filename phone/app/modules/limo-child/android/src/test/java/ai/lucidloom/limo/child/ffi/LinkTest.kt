package ai.lucidloom.limo.child.ffi

import ai.lucidloom.limo.child.ffi.testkit.TestFamily
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * The phone's side of the link from Kotlin, against the testkit's master:
 * an offer read, a pairing that makes the engine with the code secret never
 * reaching Kotlin, and a session carrying the engine's report and the
 * master's grant as bytes Kotlin never reads.
 */
class LinkTest {
  @get:Rule val folder = TemporaryFolder()

  /** Carries the phone's bytes to the master and back until the phone has nothing more to say. */
  private fun shuttle(family: TestFamily, take: () -> ByteArray, give: (ByteArray) -> Unit, ms: Long, wall: Long) {
    repeat(10) {
      val out = take()
      if (out.isEmpty()) return
      val back = family.exchange(out, ms, wall)
      if (back.isNotEmpty()) give(back)
    }
    fail("the link kept talking")
  }

  @Test
  fun aPairingMakesTheEngineAndASessionCarriesTheReportAndTheGrant() {
    val start = WEDNESDAY_7_AM
    TestFamily(folder.newFolder().absolutePath, start).use { family ->
      val child = family.addChild("Asha", miraRules(), start)

      // The offer, as it would come over the cable.
      val text = family.makeOffer(child, start)
      val offer = Offer.fromText(text)
      text.fill(0)
      assertEquals(47810, offer.port())
      assertEquals(listOf("192.168.1.20"), offer.addresses())
      assertEquals(16, offer.masterId().length)
      assertFalse(offer.looksOld(start))

      // Pairing.
      val pairing = PhonePairing.start(offer, PairRequest("Asha's phone", "android", 33), 1_000)
      try {
        pairing.pairedMaster()
        fail("a pairing gave what to keep before its welcome")
      } catch (e: LinkException.NotWelcomed) {
        // As it should.
      }
      family.connect(1_000, start)
      var welcomed = false
      shuttle(family, { pairing.takeOutgoing() }, { welcomed = welcomed or pairing.receive(it, 1_050) }, 1_050, start)
      assertTrue(welcomed)
      assertTrue(pairing.isDone())
      assertEquals(start, pairing.masterNow())

      // What the host keeps: the pairing (with the phone's private key)
      // and the engine's state. The code secret never came this way.
      val keptPairing = pairing.pairedMaster()
      val engine = pairing.makeEngine(start)
      try {
        pairing.makeEngine(start)
        fail("an engine was made twice from one pairing")
      } catch (e: EngineException.NotWelcomed) {
        // Once only.
      }
      assertTrue(engine.shouldSave())
      val keptState = engine.save()
      pairing.close()
      offer.close()

      val paired = PairedMaster.fromBytes(keptPairing)
      keptPairing.fill(0)
      assertEquals(child, paired.childId())
      assertEquals(engine.view()!!.deviceId, paired.deviceId())
      assertEquals(47810, paired.port())

      // A session, and an exchange the engine orders.
      val session = PhoneSession.start(paired, 2_000)
      family.connect(2_000, start + 60)
      val kinds = mutableListOf<LinkEventKind>()
      shuttle(family, { session.takeOutgoing() }, { back -> kinds += session.receive(back, 2_010).map { it.kind } }, 2_010, start + 60)
      assertEquals(listOf(LinkEventKind.OPENED, LinkEventKind.PONG), kinds)
      assertTrue(session.isOpen())

      val facts = PhoneFacts(
        wallSecs = start + 60,
        elapsedMs = 60_000,
        bootCount = 3,
        utcOffsetSecs = IST,
        hours24 = false,
        autoTime = true,
        interactive = true,
        keyguardLocked = false,
        call = CallState.IDLE,
        front = app(GAME, "Game"),
        inReach = true,
        prodded = false,
        permissions = ALL_PERMISSIONS,
      )
      assertTrue(engine.tick(facts).sendReport)
      val report = engine.outgoingReport(start + 60)!!
      session.sendReport(report, 2_020)
      assertTrue(session.reportWaiting())
      val events = session.receive(family.exchange(session.takeOutgoing(), 2_030, start + 60), 2_030)
      assertEquals(1, events.size)
      assertEquals(LinkEventKind.GRANT, events[0].kind)
      assertEquals(GrantOutcome.TAKEN, engine.takeGrant(events[0].grant!!, start + 60).outcome)
      assertFalse(session.reportWaiting())
      val orders = engine.tick(facts)
      assertFalse(orders.sendReport)
      assertFalse(orders.timeLeft.away)

      // The kept state, loaded again, is the same phone.
      PhoneEngine.load(keptState).use { again -> assertEquals(paired.deviceId(), again.view()!!.deviceId) }

      session.sendBye(2_040)
      assertTrue(session.takeOutgoing().isNotEmpty())
      try {
        session.sendBye(2_050)
        fail("a closed session said goodbye twice")
      } catch (e: LinkException.Closed) {
        assertEquals("the link has ended; start a new one", e.message)
      }
      session.close()
      paired.close()
      engine.close()
    }
  }

  @Test
  fun aBadOfferIsRefusedWithoutQuotingIt() {
    try {
      Offer.fromText("limo1:AAAA".toByteArray())
      fail("a broken offer was read")
    } catch (e: LinkException.BadOffer) {
      assertFalse(e.message!!.contains("AAAA"))
    }
  }
}
