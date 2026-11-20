package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.EngineException
import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.ffi.GAME
import ai.lucidloom.limo.child.ffi.InternalException
import ai.lucidloom.limo.child.ffi.LoadProblemKind
import ai.lucidloom.limo.child.ffi.LockWhy
import ai.lucidloom.limo.child.ffi.app
import ai.lucidloom.limo.child.ffi.screen
import java.io.IOException
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * When things go wrong, and with the screen off: a kept state that cannot
 * be read locks the phone and is never written over; an engine that
 * reports itself broken is made again from the kept state; a state that
 * cannot be written is asked for again; with the screen off nothing ticks
 * every second and the alarm wakes the host at the engine's next check.
 */
class HostTroubleTest {
  @get:Rule val folder = TemporaryFolder()

  @Test
  fun aKeptStateThatCannotBeReadLocksThePhoneAndIsNeverWrittenOver() {
    HostRig(folder.newFolder()).use { rig ->
      val damaged = "limo-phone-state\nformat 1\ncheck 0000000000000000\n{not what was kept}".toByteArray()
      rig.store.bytes = damaged.copyOf()
      rig.core.start()
      assertTrue(rig.core.paired)
      // Locked, with the engine's words, and nothing to do on the phone.
      val lock = rig.shower.last.lock!!
      assertEquals(LockWhy.CANNOT_CHECK, lock.why)
      assertEquals("Ask a guardian.", lock.nothingToDo!!.bold)
      assertTrue(rig.log.any { it.startsWith("host state_unreadable kind=") })
      // The dialler and LiMo stay open, as always.
      rig.front = screen(FrontKind.DIALLER)
      rig.changed()
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      rig.front = app(GAME, "Game")
      rig.changed()
      rig.pass(60)
      assertEquals(CoverKind.LOCK, rig.shower.last.kind)
      // Never written over: the bytes are as they were.
      assertEquals(0, rig.store.writes)
      assertArrayEquals(damaged, rig.store.bytes)
      // No way out on the phone; the code is wiped all the same.
      val code = "1234 5678".toByteArray()
      val answer = rig.core.tryCode(code)
      assertTrue(code.all { it.toInt() == 0 })
      assertTrue((answer as WayOut.Refused).refusal is EngineException.CannotCheck)
      assertEquals(0, rig.store.writes)
      // The view says why, and has nothing else.
      val view = rig.core.view()
      assertTrue(view.paired)
      assertNull(view.json)
      assertEquals(LoadProblemKind.DAMAGED, view.problem)

      // A guardian pairs the phone again, which replaces it.
      rig.pair()
      assertEquals(rig.pairWrites + rig.engines.current.orders.count { it.saveState }, rig.store.writes)
      assertFalse(rig.store.bytes!!.contentEquals(damaged))
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
    }
  }

  @Test
  fun aKeptStateAndroidCannotReadLocksThePhoneToo() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair()
      rig.pass(10)
      val kept = rig.store.bytes!!.copyOf()
      // A new process finds the file there and cannot read it.
      rig.kill()
      rig.store.refuseRead = IOException("the disk said no")
      rig.core.start()
      assertEquals(LockWhy.CANNOT_CHECK, rig.shower.last.lock!!.why)
      assertTrue(rig.log.any { it == "host failed what=state_read error=IOException" })
      // Nothing about what the error said.
      assertFalse(rig.log.any { it.contains("disk") })
      rig.pass(30)
      assertArrayEquals(kept, rig.store.bytes)
    }
  }

  @Test
  fun anEngineThatReportsItselfBrokenIsMadeAgainFromTheKeptState() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair()
      rig.pass(30)
      val first = rig.engines.current
      val counted = first.real.view()!!.countedSecs

      // The engine says it is broken (a panic inside it): closed, loaded again.
      first.failNextTick = EngineException.Broken("An earlier call failed inside the engine.")
      rig.pass(1)
      assertTrue(first.closed)
      assertEquals(1, rig.engines.loads)
      assertTrue(rig.log.contains("host engine_broken error=Broken loading_again"))
      val second = rig.engines.current
      assertTrue(second !== first)
      // It goes on from the kept state, a second later, losing at most what was not yet kept.
      rig.pass(5)
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      assertTrue(rig.keeper.last.counting)
      val now = second.real.view()!!.countedSecs
      assertTrue("counted $counted, then $now", now in (counted - 5)..(counted + 7))

      // A panic reaches Kotlin as UniFFI's InternalException: the same.
      second.failNextTick = InternalException("panic")
      rig.pass(1)
      assertTrue(second.closed)
      assertEquals(2, rig.engines.loads)
      rig.pass(5)
      assertTrue(rig.keeper.last.counting)

      // Broken during a way out: the answer is a failure to try again, the
      // code is wiped all the same, and the engine is made again.
      val third = rig.engines.current
      third.failNextWayOut = EngineException.Broken("An earlier call failed inside the engine.")
      val code = "1234 5678".toByteArray()
      assertEquals(WayOut.Failed, rig.core.tryCode(code))
      assertTrue(code.all { it.toInt() == 0 })
      assertTrue(third.closed)
      assertEquals(3, rig.engines.loads)

      // A failure the host does not know: logged by its kind, and the next tick goes on.
      val fourth = rig.engines.current
      fourth.failNextTick = IllegalStateException("something odd")
      rig.pass(1)
      assertFalse(fourth.closed)
      assertTrue(rig.log.contains("host failed what=tick error=IllegalStateException"))
      assertFalse(rig.log.any { it.contains("odd") })
      rig.pass(2)
      assertTrue(rig.keeper.last.counting)
      assertEquals(3, rig.engines.loads)
    }
  }

  @Test
  fun aStateThatCannotBeWrittenIsAskedForAgain() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair()
      rig.store.refuseWrite = IOException("full")
      rig.pass(20)
      val writes = rig.store.writes
      assertEquals(1, rig.log.count { it == "host failed what=state_write error=IOException" })
      // Room again: the engine asks at the next tick, and it is written.
      rig.store.refuseWrite = null
      rig.pass(1)
      assertTrue(rig.store.writes > writes)
    }
  }

  @Test
  fun aPairingWhoseStateCannotBeKeptChangesNothing() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.store.refuseWrite = IOException("full")
      val welcome = rig.family.welcome(rig.child, "Phone", rig.wall)
      assertFalse(rig.core.pair(welcome.welcome, welcome.codeSecret, "Mira"))
      // The name is kept only with the state.
      assertNull(rig.names.text)
      // Wiped all the same; the engine made for it closed and never used; still not paired.
      assertTrue(welcome.codeSecret.all { it.toInt() == 0 })
      assertTrue(rig.engines.current.closed)
      assertFalse(rig.core.paired)
      assertNull(rig.tickDueAt)
      assertTrue(rig.events.isEmpty())
    }
  }

  @Test
  fun withTheScreenOffNothingTicksEverySecondAndTheAlarmWakesTheHost() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair()
      rig.pass(10)

      // The screen goes off: one tick at once, then no tick every second.
      rig.interactive = false
      rig.changed()
      assertFalse(rig.keeper.last.counting)
      assertTrue(rig.keeper.last.words, rig.keeper.last.words.endsWith(" paused"))
      val ticks = rig.ticksRun
      rig.pass(10 * 60)
      assertTrue("ticked ${rig.ticksRun - ticks} times in ten minutes with the screen off", rig.ticksRun - ticks <= 1)
      // The alarm is set, an ordinary one, at most fifteen minutes away; it wakes the host.
      val alarm = rig.alarmAt
      assertNotNull(alarm)
      assertTrue(alarm!! - rig.elapsedMs <= HostTimes.WATCHDOG_MS)
      val fired = rig.alarmsFired
      rig.pass(15 * 60)
      assertTrue(rig.alarmsFired > fired)
      assertNotNull("set again after it fired", rig.alarmAt)

      // The screen on: a tick at once, and every second again.
      rig.interactive = true
      val on = rig.ticksRun
      rig.changed()
      assertEquals(on + 1, rig.ticksRun)
      rig.pass(30)
      assertEquals(on + 31, rig.ticksRun)
      assertTrue(rig.keeper.last.counting)
    }
  }

  @Test
  fun forgettingThePairingStopsEverything() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair()
      rig.pass(5)
      rig.core.forget()
      assertFalse(rig.core.paired)
      assertNull(rig.store.bytes)
      assertNull(rig.tickDueAt)
      assertNull(rig.alarmAt)
      assertEquals(0, rig.listening)
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      assertEquals(1, rig.keeper.cleared)
      assertEquals(HostChange.PAIRING, rig.events.last())
      val ticks = rig.ticksRun
      rig.pass(60)
      assertEquals(ticks, rig.ticksRun)
      assertEquals(WayOut.NotPaired, rig.core.askForTime(10, ""))
    }
  }
}
