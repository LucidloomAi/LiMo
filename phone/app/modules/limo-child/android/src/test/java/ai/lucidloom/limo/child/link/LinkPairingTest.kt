package ai.lucidloom.limo.child.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.IOException

/**
 * Pairing over the cable: the door's rules, an offer taken before the PC's
 * tunnel is up and paired as soon as it is, the child's name from the
 * master's welcome, offers refused (none, too long, not an offer, a second,
 * while paired, used already, never answered), a pairing that cannot be
 * kept, and the way to the master when something else listens on the
 * phone's loopback.
 */
class LinkPairingTest {
  @get:Rule val folder = TemporaryFolder()

  @Test
  fun theDoorIsWrittenOnlyByTheShellAtItsOnePathAndTakesAtMostAnOffer() {
    assertNull(CableDoorRules.refusal("/offer", "w", hasDump = true))
    assertEquals("caller", CableDoorRules.refusal("/offer", "w", hasDump = false))
    assertEquals("path", CableDoorRules.refusal("/status", "w", hasDump = true))
    assertEquals("path", CableDoorRules.refusal(null, "w", hasDump = true))
    assertEquals("mode", CableDoorRules.refusal("/offer", "r", hasDump = true))
    assertEquals("mode", CableDoorRules.refusal("/offer", "rw", hasDump = true))
    val offer = ByteArray(LinkCore.MAX_OFFER_BYTES) { 'A'.code.toByte() }
    assertEquals(LinkCore.MAX_OFFER_BYTES, CableDoorRules.readAtMost(ByteArrayInputStream(offer))!!.size)
    assertNull(CableDoorRules.readAtMost(ByteArrayInputStream(offer + 'A'.code.toByte())))
    assertEquals(0, CableDoorRules.readAtMost(ByteArrayInputStream(ByteArray(0)))!!.size)
  }

  @Test
  fun anOfferTakenBeforeTheTunnelIsUpPairsAsSoonAsItIsWithTheChildsName() {
    LinkRig(folder.newFolder()).use { r ->
      r.wires.cableIn = false
      val text = r.family.makeOffer(r.rig.child, r.rig.wall)
      assertEquals(OfferAnswer.TAKEN, r.core.takeOffer(text))
      assertTrue("the offer wiped", text.all { it.toInt() == 0 })
      assertTrue(r.log.contains("cable offer taken"))
      // The PC sets its tunnel up a moment later.
      r.pass(3)
      assertFalse(r.rig.core.paired)
      assertTrue(r.wires.knocked.all { it.way == Way.CABLE && it.address == Addresses.LOOPBACK && it.port == 47810 })
      r.wires.cableIn = true
      r.passUntil("link paired", most = 5)
      assertTrue(r.rig.core.paired)
      assertEquals("Mira", r.rig.names.read())
      assertTrue(r.rig.log.contains("host paired"))
      // Paired: the door refuses any offer now.
      val again = r.family.makeOffer(r.rig.child, r.rig.wall)
      assertEquals(OfferAnswer.PAIRED, r.core.takeOffer(again))
      assertTrue(r.log.contains("cable offer refused why=paired"))
      // And the log never holds the offer, nor the child's name.
      assertTrue(r.log.none { it.contains("limo1") || it.contains("Mira") })
    }
  }

  @Test
  fun offersTheDoorRefuses() {
    LinkRig(folder.newFolder()).use { r ->
      assertEquals(OfferAnswer.EMPTY, r.core.takeOffer(ByteArray(0)))
      assertEquals(OfferAnswer.TOO_LONG, r.core.takeOffer(null))
      assertEquals(OfferAnswer.TOO_LONG, r.core.takeOffer(ByteArray(LinkCore.MAX_OFFER_BYTES + 1) { 'A'.code.toByte() }))
      val bad = "limo1:AAAA".toByteArray()
      assertEquals(OfferAnswer.NOT_AN_OFFER, r.core.takeOffer(bad))
      assertTrue(bad.all { it.toInt() == 0 })
      // One offer at a time: a second while the first is being paired with.
      r.wires.cableIn = false
      assertEquals(OfferAnswer.TAKEN, r.core.takeOffer(r.family.makeOffer(r.rig.child, r.rig.wall)))
      assertTrue(r.core.offerHeld)
      assertEquals(OfferAnswer.BUSY, r.core.takeOffer(r.family.makeOffer(r.rig.child, r.rig.wall)))
      // Never answered (no cable, no Wi-Fi), it is dropped after ten minutes,
      // and the door takes a new one.
      r.pass(601)
      assertTrue(r.log.contains("link offer dropped why=too_old"))
      assertFalse(r.core.offerHeld)
      assertFalse(r.rig.core.paired)
      r.wires.cableIn = true
      assertEquals(OfferAnswer.TAKEN, r.core.takeOffer(r.family.makeOffer(r.rig.child, r.rig.wall)))
      r.passUntil("link paired", most = 5)
      for (word in listOf("empty", "too_long", "not_an_offer", "busy")) {
        assertTrue(word, r.log.contains("cable offer refused why=$word"))
      }
    }
  }

  @Test
  fun anOfferUsedAlreadyIsRefusedByTheMasterAndDropped() {
    LinkRig(folder.newFolder()).use { r ->
      val text = r.family.makeOffer(r.rig.child, r.rig.wall)
      val copy = text.copyOf()
      assertEquals(OfferAnswer.TAKEN, r.core.takeOffer(text))
      r.passUntil("link paired", most = 5)
      // The phone is forgotten on the phone itself (as the development tool
      // can), and the same offer comes again.
      r.rig.core.forget()
      r.pass(2)
      assertNull("the pairing kept for a forgotten phone goes", r.pairing.bytes)
      assertEquals(OfferAnswer.TAKEN, r.core.takeOffer(copy))
      r.passUntil("link pair_refused why=offer_used", most = 5)
      assertFalse(r.core.offerHeld)
      assertFalse(r.rig.core.paired)
      assertNull(r.pairing.bytes)
    }
  }

  @Test
  fun aPairingThatCannotBeKeptLeavesThePhoneAsItWas() {
    LinkRig(folder.newFolder()).use { r ->
      r.pairing.refuseWrite = IOException("full")
      assertEquals(OfferAnswer.TAKEN, r.core.takeOffer(r.family.makeOffer(r.rig.child, r.rig.wall)))
      r.passUntil("link pair_refused why=not_kept", most = 5)
      assertFalse(r.rig.core.paired)
      assertNull(r.rig.store.bytes)
      // The state cannot be kept either: the kept pairing is taken back.
      r.pairing.refuseWrite = null
      r.rig.store.refuseWrite = IOException("full")
      val second = r.mark()
      assertEquals(OfferAnswer.TAKEN, r.core.takeOffer(r.family.makeOffer(r.rig.child, r.rig.wall)))
      r.passUntil("link pair_refused why=not_kept", most = 5, from = second)
      assertFalse(r.rig.core.paired)
      assertNull(r.pairing.bytes)
    }
  }

  @Test
  fun somethingElseListeningOnThePhonesLoopbackCannotStandInTheWay() {
    LinkRig(folder.newFolder()).use { r ->
      r.pairOverTheCable()
      r.passUntil("link open via=cable")
      // The cable goes; a stranger listens on the phone's loopback at the
      // PC's port, which answers nothing; the PC is on the Wi-Fi.
      r.wires.pullCable()
      r.pass(2)
      r.wires.strangerOnLoopback = true
      r.wires.onWifi = true
      r.core.poke()
      r.pass(1)
      // The stranger's handshake runs out, and the next try goes to the
      // home network first.
      r.passUntil("link open via=wifi", most = 30)
      assertTrue(r.core.inReach)
    }
  }

  @Test
  fun addressesAreTheLoopbackOrAHomeNetworksOnly() {
    for (home in listOf("10.0.0.2", "172.16.4.1", "172.31.255.254", "192.168.1.20")) assertTrue(home, Addresses.isHome(home))
    for (not in listOf("127.0.0.1", "8.8.8.8", "172.32.0.1", "172.15.0.1", "169.254.1.1", "100.64.0.1", "192.169.0.1", "0.0.0.0", "255.255.255.255")) {
      assertFalse(not, Addresses.isHome(not))
    }
    for (bad in listOf("", "1.2.3", "1.2.3.4.5", "256.1.1.1", "a.b.c.d", "1..2.3", " 10.0.0.1", "10.0.0.01x", "lucidloom.ai")) {
      assertNull(bad, Addresses.parse(bad))
    }
    assertEquals(listOf(192, 168, 1, 20), Addresses.parse("192.168.1.20")!!.map { it.toInt() and 0xff })
    assertEquals("Galaxy S20 Ultra", PhoneName.clean("  Galaxy‎ S20 Ultra\n"))
    assertEquals(40, PhoneName.clean("x".repeat(60))!!.length)
    assertNull(PhoneName.clean(" \u0007 "))
  }
}
