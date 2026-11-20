package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.CASINO
import ai.lucidloom.limo.child.ffi.CLOCK
import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.EngineException
import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.ffi.GAME
import ai.lucidloom.limo.child.ffi.LockWhy
import ai.lucidloom.limo.child.ffi.PIN
import ai.lucidloom.limo.child.ffi.PinAction
import ai.lucidloom.limo.child.ffi.PinActionKind
import ai.lucidloom.limo.child.ffi.TryKind
import ai.lucidloom.limo.child.ffi.app
import ai.lucidloom.limo.child.ffi.screen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * A day through the host, on the real engine: not paired and waiting;
 * paired; ticking once a second; the time left reaching the keeper; a
 * blocked app's cover handed to the shower; the warnings; the lock; the ways
 * out with their secrets wiped; the state kept exactly when the engine said
 * so and before anything else; and the process killed and started again
 * with the same orders.
 */
class HostDayTest {
  @get:Rule val folder = TemporaryFolder()

  @Test
  fun aDayThroughTheHost() {
    HostRig(folder.newFolder()).use { rig ->
      // Not paired: the host waits, and does nothing else.
      rig.core.start()
      assertFalse(rig.core.paired)
      assertNull(rig.tickDueAt)
      assertNull(rig.alarmAt)
      assertEquals(0, rig.listening)
      assertEquals(0, rig.store.writes)
      assertTrue(rig.log.contains("host not_paired waiting"))
      rig.pass(30)
      assertEquals(0, rig.ticksRun)
      assertEquals(WayOut.NotPaired, rig.core.tryCode("1234 5678".toByteArray()))
      assertEquals(HostView(paired = false, json = null, problem = null), rig.core.view())

      // Paired and set up: kept at once and again when setup was finished (and at the ticks the engine asks), told,
      // watching the facts, ticking.
      rig.pair()
      assertTrue(rig.core.paired)
      assertEquals(rig.pairWrites + rig.engines.current.orders.count { it.saveState }, rig.store.writes)
      assertTrue(rig.log.contains("host setup finished"))
      assertEquals(listOf(HostChange.PAIRING), rig.events.take(1))
      assertEquals(1, rig.listening)
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      assertEquals("30 min left", rig.keeper.last.words)

      // A minute in a game: a tick every second, the time left reaching the keeper.
      val ticks = rig.ticksRun
      rig.pass(60)
      assertEquals(60, rig.ticksRun - ticks)
      assertEquals("29 min left", rig.keeper.last.words)
      assertTrue(rig.keeper.last.counting)
      assertEquals(29L * 60, rig.keeper.last.secondsLeft)
      assertTrue("the alarm is always set, at most fifteen minutes away", rig.alarmAt!! - rig.elapsedMs <= HostTimes.WATCHDOG_MS)

      // A blocked app: the watch tells, the host ticks at once, the cover goes to the shower.
      rig.front = app(CASINO, "Casino")
      val before = rig.ticksRun
      rig.changed()
      assertEquals(before + 1, rig.ticksRun)
      assertEquals(CoverKind.BLOCK, rig.shower.last.kind)
      assertEquals("Casino is not allowed.", rig.shower.last.block!!.title)
      // The engine's note, logged by the app's package, never its label.
      assertEquals(1, rig.log.count { it.startsWith("note ") && it.contains("blocked_app_opened") && it.contains(CASINO) })
      assertFalse(rig.log.any { it.contains("Casino") })

      // Back to the game until the time is used up: three warnings to the keeper and the shower, then the lock.
      rig.front = app(GAME, "Game")
      rig.changed()
      var guard = 0
      while (rig.shower.last.kind == CoverKind.NOTHING) {
        rig.pass(1)
        if (++guard > 40 * 60) fail("the lock never came")
      }
      assertEquals(listOf(15, 5, 1), rig.keeper.warnings.map { it.minutes })
      assertEquals(listOf(15, 5, 1), rig.shower.warnings.map { it.minutes })
      assertEquals("5 minutes of screen time left today.", rig.keeper.warnings[1].message)
      val lock = rig.shower.last.lock!!
      assertEquals(LockWhy.TIME_USED_UP, lock.why)
      assertTrue(rig.keeper.last.locked)
      assertEquals("Today's time is used up.", rig.keeper.last.words)
      assertTrue(rig.events.contains(HostChange.VIEW))

      // The ways out, on the host: a wrong PIN, then a code that adds time.
      // Each secret is wiped, the state kept before the answer, and the lock lifts at once.
      rig.front = screen(FrontKind.LIMO)
      rig.changed()
      val wrongPin = "111111".toByteArray()
      val writes = rig.store.writes
      val wrong = rig.core.tryPin(wrongPin, PinAction(PinActionKind.ADD_MINUTES, minutes = 20))
      assertTrue(wrongPin.all { it.toInt() == 0 })
      assertEquals(TryKind.WRONG, (wrong as WayOut.Done).value.kind)
      assertTrue(rig.store.writes > writes)
      val code = rig.family.makeCode(rig.engines.current.real.view()!!.deviceId, 15, rig.wall).toByteArray()
      val added = rig.core.tryCode(code)
      assertTrue(code.all { it.toInt() == 0 })
      assertEquals(TryKind.ADDED, (added as WayOut.Done).value.kind)
      assertEquals("15 minutes added.", added.value.message)
      assertTrue(rig.log.contains("way_out code answer=added"))
      rig.front = app(GAME, "Game")
      rig.changed()
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      assertTrue(rig.keeper.last.counting)

      // Asking for time: the core's request, then its refusal in its own words.
      val asked = rig.core.askForTime(20, "For the school project")
      assertEquals(20, (asked as WayOut.Done).value.minutes)
      val again = rig.core.askForTime(10, "")
      assertTrue(again is WayOut.Refused)
      assertTrue((again as WayOut.Refused).refusal is EngineException.RequestWaiting)
      assertEquals("A request from this device is still waiting for an answer.", again.refusal.message)
      // The reason the child typed is never in the log.
      assertFalse(rig.log.any { it.contains("school") })

      // The view for the screens: the engine's own JSON.
      val view = rig.core.view()
      assertTrue(view.paired)
      assertNull(view.problem)
      assertTrue(view.json!!.contains(""""today":{"left":"""))
      assertTrue(view.json!!.contains(""""rules_words":"""))

      // The right PIN: a guardian adds 20 minutes; the PIN wiped.
      val pin = PIN.toByteArray()
      val right = rig.core.tryPin(pin, PinAction(PinActionKind.ADD_MINUTES, minutes = 20))
      assertTrue(pin.all { it.toInt() == 0 })
      assertEquals(TryKind.ADDED, (right as WayOut.Done).value.kind)
    }
  }

  @Test
  fun theStateIsKeptExactlyWhenTheEngineSaysAndBeforeAnythingElse() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair()
      rig.pass(90)
      rig.front = app(CASINO, "Casino")
      rig.changed()
      rig.pass(5)
      rig.front = screen(FrontKind.HOME)
      rig.changed()
      rig.pass(20)
      rig.interactive = false
      rig.changed()
      rig.pass(120)

      // Every tick of every engine, with whether its orders said to keep the state.
      val orders = rig.engines.made.flatMap { it.orders }
      val toKeep = orders.count { it.saveState }
      assertTrue("the engine asked to keep the state now and then, not at every tick", toKeep in 1 until orders.size)
      // One write for the pairing and one for setup finished, then exactly one for each tick that said so: no more,
      // no fewer.
      assertEquals(rig.pairWrites + toKeep, rig.store.writes)

      // And in each tick, the state was kept before the cover was handed out.
      var i = rig.trail.indexOf("tick")
      var t = 0
      while (i >= 0) {
        val next = rig.trail.subList(i + 1, rig.trail.size).indexOf("tick").let { if (it < 0) rig.trail.size else i + 1 + it }
        val segment = rig.trail.subList(i + 1, next)
        if (orders[t].saveState) {
          assertEquals("tick $t", listOf("save", "show"), segment.take(2))
        } else {
          assertEquals("tick $t", listOf("show"), segment.take(1))
        }
        t += 1
        i = if (next < rig.trail.size) next else -1
      }
      assertEquals(orders.size, t)
    }
  }

  @Test
  fun aKilledProcessStartsAgainWithTheSameOrders() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair()
      // Counting in a game, then killed in the middle of it.
      rig.pass(47)
      val countedBefore = rig.engines.current.real.view()!!.countedSecs
      rig.kill()
      rig.pass(1)
      rig.core.start()
      // The new host loaded what was kept and ticked at once: counting, at
      // most five seconds lost (the engine keeps the state every five
      // seconds while counting, and once more at the next tick).
      assertEquals(1, rig.engines.loads)
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      assertTrue(rig.keeper.last.counting)
      val countedAfter = rig.engines.current.real.view()!!.countedSecs
      assertTrue("counted $countedBefore before, $countedAfter after", countedAfter in (countedBefore - 5)..(countedBefore + 1))

      // Run the day to its lock, then kill the process: the lock is on again
      // at the first tick, the same orders as the engine that was killed gave.
      var guard = 0
      while (rig.shower.last.kind != CoverKind.LOCK) {
        rig.pass(1)
        if (++guard > 40 * 60) fail("the lock never came")
      }
      rig.pass(3)
      val killed = rig.engines.current
      rig.kill()
      val lastBefore = killed.orders.last()
      rig.core.start()
      val firstAfter = rig.engines.current.orders.first()
      assertEquals(2, rig.engines.loads)
      assertEquals(lastBefore.cover, firstAfter.cover)
      assertEquals(lastBefore.timeLeft, firstAfter.timeLeft)
      assertEquals(CoverKind.LOCK, rig.shower.last.kind)
      assertEquals(LockWhy.TIME_USED_UP, rig.shower.last.lock!!.why)
      // And an allowed app is still usable under it.
      rig.front = app(CLOCK, "Clock")
      rig.changed()
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
    }
  }
}
