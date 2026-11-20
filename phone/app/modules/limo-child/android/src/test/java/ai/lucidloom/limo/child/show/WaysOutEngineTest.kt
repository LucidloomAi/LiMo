package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.ffi.GAME
import ai.lucidloom.limo.child.ffi.LockWhy
import ai.lucidloom.limo.child.ffi.PIN
import ai.lucidloom.limo.child.ffi.Way
import ai.lucidloom.limo.child.ffi.app
import ai.lucidloom.limo.child.ffi.screen
import ai.lucidloom.limo.child.host.HostRig
import ai.lucidloom.limo.child.host.WayOut
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * The covers and the ways out on the REAL engine, through the host (its
 * test rig, host/HostRig.kt): the lock the engine orders becomes the plan
 * drawn; each way out goes through the host's three calls with what was
 * typed, wiped once read, and its answer is shown in the engine's own words.
 */
class WaysOutEngineTest {
  @get:Rule val folder = TemporaryFolder()

  private val ist = ZoneId.of("Asia/Kolkata")

  @Suppress("UNCHECKED_CAST")
  private fun send(rig: HostRig, call: WayCall?): WayOut<Any> = when (call) {
    is WayCall.Ask -> rig.core.askForTime(call.minutes, call.reason)
    is WayCall.Code -> rig.core.tryCode(call.typed).also { assertTrue("the host wiped the code", call.typed.all { it.toInt() == 0 }) }
    is WayCall.Pin -> rig.core.tryPin(call.typed, call.action).also { assertTrue("the host wiped the PIN", call.typed.all { it.toInt() == 0 }) }
    null -> error("no call")
  } as WayOut<Any>

  private fun lockPlan(rig: HostRig): CoverPlan.Lock {
    assertEquals(CoverKind.LOCK, rig.shower.last.kind)
    return Plans.of(rig.shower.last, "Mira", FakeApps(dialler = null)) as CoverPlan.Lock
  }

  private fun useTheDayUp(rig: HostRig) {
    rig.core.start()
    rig.pair()
    var guard = 0
    while (rig.shower.last.kind == CoverKind.NOTHING) {
      rig.pass(1)
      check(++guard < 40 * 60) { "the lock never came" }
    }
  }

  @Test
  fun theLocksWaysOutWithTheEnginesWordsRightAndWrong() {
    HostRig(folder.newFolder()).use { rig ->
      useTheDayUp(rig)
      val lock = lockPlan(rig)
      assertEquals(LockWhy.TIME_USED_UP, lock.why)
      assertEquals("Today’s time is used up.", lock.title)
      assertEquals(listOf(Way.ASK, Way.CODE, Way.PIN), lock.rows.map { it.way })
      assertEquals(listOf(Pinned.EMERGENCY), lock.pinned)

      // The child opens a way out: LiMo is in front, nothing is covered.
      rig.front = screen(FrontKind.LIMO)
      rig.changed()
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)

      // A wrong PIN: stays, in the engine's words.
      val pin = WaysOutModel(Panel.PIN)
      "111111".forEach { pin.type(it) }
      assertEquals(After.Stay, pin.answered(send(rig, pin.submit(rig.wall, ist))))
      assertEquals("That PIN is not right. 4 more tries before a wait.", pin.message!!.text)
      assertFalse(pin.message!!.right)
      assertTrue(pin.wiped())

      // A right code: the engine's words, and the phone opens.
      val code = WaysOutModel(Panel.CODE)
      rig.family.makeCode(rig.engines.current.real.view()!!.deviceId, 15, rig.wall).filter { it.isDigit() }.forEach { code.type(it) }
      assertEquals(After.CloseSoon, code.answered(send(rig, code.submit(rig.wall, ist))))
      assertEquals("15 minutes added. The phone opens now.", code.message!!.text)
      assertTrue(code.message!!.right)
      rig.front = app(GAME, "Game")
      rig.changed()
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
    }
  }

  @Test
  fun aRequestWaitsOnTheLockAndFiveWrongTriesGreyTheCodeAndThePin() {
    HostRig(folder.newFolder()).use { rig ->
      useTheDayUp(rig)
      rig.front = screen(FrontKind.LIMO)
      rig.changed()

      val ask = WaysOutModel(Panel.ASK)
      ask.pick(AskChoice.MIN30)
      ask.reason = "To finish my science project"
      assertEquals(After.Close, ask.answered(send(rig, ask.submit(rig.wall, ist))))
      // A second request is the core's to refuse, in its words.
      val again = WaysOutModel(Panel.ASK)
      assertEquals(After.Stay, again.answered(send(rig, again.submit(rig.wall, ist))))
      assertEquals("A request from this device is still waiting for an answer.", again.message!!.text)

      // Back to the lock: the request waits there, and asking is not offered again.
      rig.front = app(GAME, "Game")
      rig.changed()
      var lock = lockPlan(rig)
      assertEquals(Tone.INK, lock.request!!.tone)
      assertEquals("Waiting", lock.request!!.stamp)
      assertTrue(lock.request!!.text.startsWith("You asked for 30 min at "))
      assertEquals(listOf(Way.CODE, Way.PIN), lock.rows.map { it.way })

      // Five wrong codes: the fifth goes back to the lock, which shows the wait with the rows greyed.
      rig.front = screen(FrontKind.LIMO)
      rig.changed()
      var after: After = After.Stay
      repeat(5) {
        val m = WaysOutModel(Panel.CODE)
        "11112222".forEach { d -> m.type(d) }
        after = m.answered(send(rig, m.submit(rig.wall, ist)))
      }
      assertEquals(After.Close, after)
      rig.front = app(GAME, "Game")
      rig.changed()
      lock = lockPlan(rig)
      assertTrue(lock.waitWords!!.startsWith("Too many wrong tries. The next try is "))
      assertEquals(listOf(true, true), lock.rows.map { it.off })
      assertTrue(lock.rows.all { it.sub.startsWith("Wait until ") })

      // While it waits, even the right PIN must wait: back to the lock.
      rig.front = screen(FrontKind.LIMO)
      rig.changed()
      val right = WaysOutModel(Panel.PIN)
      PIN.forEach { right.type(it) }
      assertEquals(After.Close, right.answered(send(rig, right.submit(rig.wall, ist))))
    }
  }

  @Test
  fun theShowerFollowsTheRealOrdersAsALevel() {
    HostRig(folder.newFolder()).use { rig ->
      val cover = FakeCover()
      val core = ShowerCore(cover, FakeCard(), FakeTimer())
      val apps = FakeApps(dialler = null)
      useTheDayUp(rig)
      val from = rig.shower.covers.size
      rig.pass(30)
      for (c in rig.shower.covers.drop(from - 1)) core.show(Plans.of(c, "Mira", apps))
      assertEquals("thirty ticks of the same lock draw it once", 1, cover.drawn.size)
      assertTrue(cover.refreshed >= 30)
      rig.front = screen(FrontKind.DIALLER)
      rig.changed()
      core.show(Plans.of(rig.shower.last, "Mira", apps))
      assertFalse("the dialler is never covered", cover.up)
    }
  }
}
