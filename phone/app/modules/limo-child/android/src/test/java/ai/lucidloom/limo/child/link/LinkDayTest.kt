package ai.lucidloom.limo.child.link

import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.ffi.LockWhy
import ai.lucidloom.limo.child.ffi.PinAction
import ai.lucidloom.limo.child.ffi.PinActionKind
import ai.lucidloom.limo.child.ffi.TryKind
import ai.lucidloom.limo.child.host.WayOut
import ai.lucidloom.limo.child.ffi.screen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * A whole day of one budget, through the link: the phone paired over the
 * cable, its use reported and counted at the master, the cable pulled and
 * the phone living on its away time, the PC used meanwhile, the phone home
 * on the Wi-Fi with the PC's minutes taken off and said on Today, and the
 * day used up across both. Then the PC removes the phone, which releases
 * itself.
 */
class LinkDayTest {
  @get:Rule val folder = TemporaryFolder()

  /** The minutes the master counted for the child today, on all devices. */
  private fun LinkRig.countedMinutes(): Long = family.usedToday(rig.child, rig.wall) / 60

  @Test
  fun aWholeDayOfOneBudgetWithTheMaster() {
    LinkRig(folder.newFolder()).use { r ->
      // 07:00. The PC hands the phone an offer over the cable; it pairs.
      val device = r.pairOverTheCable()
      assertEquals("Mira", r.rig.names.read())
      assertTrue(r.pairing.bytes != null)
      assertTrue("the pairing kept before the state", r.rig.trail.indexOf("pairing") in 0 until r.rig.trail.indexOf("save"))

      // A session through the cable at once, a report, and the app list.
      r.passUntil("link open via=cable")
      assertTrue(r.core.inReach)
      r.pass(3)
      assertEquals(listOf("org.example.game", "org.example.maps"), r.family.appsOf(device))
      assertFalse(r.rig.keeper.last.away)

      // Ten minutes in the game, in reach: reported every minute and counted.
      r.pass(10 * 60)
      r.rig.interactive = false
      r.rig.changed()
      r.pass(70)
      assertEquals(10L, r.countedMinutes())

      // 08:00. The cable is pulled: the link closes, the phone is away.
      r.wires.pullCable()
      r.pass(2)
      assertTrue(r.log.contains("link closed"))
      assertFalse(r.core.inReach)
      assertTrue(r.rig.keeper.last.away)

      // 16:00. The PC is used for ten minutes, the phone lying at home off
      // the Wi-Fi.
      r.rig.wall += 8 * 3_600 - 72
      r.rig.elapsedMs += (8 * 3_600 - 72) * 1_000L
      r.family.usePc(r.rig.child, 10, r.rig.wall)
      assertEquals(20L, r.countedMinutes())

      // 18:00. The phone is home on the Wi-Fi: found at the offer's address,
      // it reports and hears what is left and how much went on the PC.
      r.rig.wall += 2 * 3_600
      r.rig.elapsedMs += 2 * 3_600 * 1_000L
      r.wires.onWifi = true
      r.core.poke()
      r.passUntil("link open via=wifi", most = 120)
      r.pass(3)
      val today = r.rig.core.view().json!!
      assertTrue(today, today.contains("on the PC 10 min"))
      assertTrue(today, today.contains("\"used\":\"20 min used of 30 min\""))

      // The rest of the day in the game: the phone locks at 30 minutes in
      // all, the PC's ten among them.
      r.rig.interactive = true
      r.rig.changed()
      var locked = false
      repeat(15 * 60) {
        if (!locked) {
          r.pass(1)
          locked = r.rig.shower.last.kind == CoverKind.LOCK
        }
      }
      assertTrue("locked when the day is used up", locked)
      assertEquals(LockWhy.TIME_USED_UP, r.rig.shower.last.lock?.why)
      r.rig.front = screen(FrontKind.LIMO)
      r.rig.changed()
      r.pass(70)
      assertEquals(30L, r.countedMinutes())
    }
  }

  @Test
  fun whatAGuardianChangesOnThePcReachesThePhoneAtOnceWhenTheMasterAsks() {
    LinkRig(folder.newFolder()).use { r ->
      r.rig.interactive = false
      r.pairOverTheCable()
      r.passUntil("link open via=cable")
      r.pass(5)
      val spy = r.rig.engines.current
      fun newPinWorks(): Boolean {
        val answer = r.rig.core.tryPin("135790".toByteArray(), PinAction(PinActionKind.CONFIRM_RELEASE))
        return (answer as WayOut.Done).value.kind == TryKind.RELEASE_CONFIRMED
      }
      // Idle, in reach, the phone reports by itself only every five minutes.
      val quiet = spy.orders.size
      r.pass(3)
      assertTrue(spy.orders.drop(quiet).none { it.sendReport })
      // A guardian sets a new PIN on the PC. The master asks the phone to
      // report, and its grant brings the new PIN, within seconds.
      val ordersBefore = spy.orders.size
      r.family.setPin("135790", r.rig.wall)
      r.prod()
      r.pass(2)
      assertTrue(spy.orders.drop(ordersBefore).any { it.sendReport })
      assertTrue(newPinWorks())
    }
  }

  @Test
  fun contactLostWithAReportWaitingIsTheEnginesAndComesBack() {
    LinkRig(folder.newFolder()).use { r ->
      r.pairOverTheCable()
      r.passUntil("link open via=cable")
      r.pass(65)
      assertFalse(r.rig.keeper.last.away)
      // The PC stops answering: the report the engine sends gets no grant.
      r.wires.pcDeaf = true
      r.pass(75)
      // Ten seconds with a report waiting: the session ends, the engine is
      // told, and the phone is away.
      assertTrue(r.log.contains("link closed"))
      assertTrue(r.rig.keeper.last.away)
      // The PC answers again: the phone comes back on its own.
      val back = r.mark()
      r.wires.pcDeaf = false
      r.passUntil("link open via=cable", most = 120, from = back)
      r.pass(3)
      assertFalse(r.rig.keeper.last.away)
    }
  }

  @Test
  fun aPhoneRemovedOnThePcReleasesItselfAtItsNextContact() {
    LinkRig(folder.newFolder()).use { r ->
      val device = r.pairOverTheCable()
      r.passUntil("link open via=cable")
      r.pass(5)
      r.wires.pullCable()
      r.pass(2)
      r.family.removePhone(device, r.rig.wall)
      // Back in reach: the PC says it removed the phone.
      r.wires.cableIn = true
      r.core.poke()
      r.passUntil("link removed releasing")
      assertTrue(r.rig.log.contains("host released"))
      assertFalse(r.rig.core.paired)
      assertNull(r.pairing.bytes)
      assertNull(r.rig.store.bytes)
      assertNull(r.rig.names.text)
      assertEquals(1, r.rig.keeper.cleared)
      assertFalse(r.core.inReach)
      // Nothing more is tried, and a new offer can pair it again.
      val knocks = r.wires.knocked.size
      r.pass(120)
      assertEquals(knocks, r.wires.knocked.size)
      r.pairOverTheCable()
      assertTrue(r.rig.core.paired)
    }
  }

  @Test
  fun neverMoreThanFortyCallsOnThePcInAMinute() {
    LinkRig(folder.newFolder()).use { r ->
      r.rig.interactive = false
      r.pairOverTheCable()
      r.passUntil("link open via=cable")
      r.pass(65)
      // A PC that asks for a report every second, for a minute (each new PIN
      // is news the phone has not had): the phone reports until it has
      // called forty times this minute, then waits, and the session stays.
      repeat(60) { i ->
        r.family.setPin("48291${i % 10}", r.rig.wall)
        r.prod()
        r.pass(1)
      }
      assertTrue(r.rig.log.contains("host report_ordered no_link"))
      assertFalse(r.log.contains("link closed"))
      assertTrue(r.core.inReach)
    }
  }
}
