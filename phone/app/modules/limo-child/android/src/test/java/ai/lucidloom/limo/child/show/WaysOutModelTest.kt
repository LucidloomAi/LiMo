package ai.lucidloom.limo.child.show

import ai.lucidloom.limo.child.ffi.EngineException
import ai.lucidloom.limo.child.ffi.PinActionKind
import ai.lucidloom.limo.child.ffi.TimeRequest
import ai.lucidloom.limo.child.ffi.TryAnswer
import ai.lucidloom.limo.child.ffi.TryKind
import ai.lucidloom.limo.child.host.WayOut
import java.time.ZoneId
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The ways out's state machines, with no Android and no engine: typing,
 * each kind of answer, the wait, a request, back, the screen rebuilt in the
 * middle of a call, and the digits wiped at every turn.
 */
class WaysOutModelTest {
  private val ist = ZoneId.of("Asia/Kolkata")
  private val now = 1_795_018_320L

  private fun answer(kind: TryKind, message: String, opensNow: String? = null, waitUntil: Long? = null) =
    TryAnswer(kind, message, opensNow, null, null, null, waitUntil)

  private fun typed(m: WaysOutModel, digits: String) = digits.forEach { assertTrue(m.type(it)) }

  @Suppress("UNCHECKED_CAST")
  private fun done(v: Any): WayOut<Any> = WayOut.Done(v) as WayOut<Any>

  @Test
  fun aRightCodeIsSentAsBytesTheDigitsWipedAndTheEnginesWordsShownBeforeTheScreenCloses() {
    val m = WaysOutModel(Panel.CODE)
    typed(m, "13420715")
    assertFalse("a ninth digit is not taken", m.type('9'))
    assertEquals('1', m.digit(0))
    assertEquals('5', m.digit(7))
    val call = m.submit(now, ist) as WayCall.Code
    assertArrayEquals("13420715".toByteArray(), call.typed)
    assertTrue("the digits are wiped once the call is made", m.wiped())
    assertTrue(m.busy)
    assertFalse("no tap while the engine answers", m.type('1'))
    assertNull("no second call while one waits", m.submit(now, ist))
    val after = m.answered(done(answer(TryKind.ADDED, "15 minutes added.", opensNow = "The phone opens now.")))
    assertEquals(After.CloseSoon, after)
    assertEquals(Message("15 minutes added. The phone opens now.", right = true), m.message)
  }

  @Test
  fun aWrongCodeStaysWithTheEnginesWordsAndTypingAgainClearsThem() {
    val m = WaysOutModel(Panel.CODE)
    typed(m, "1111")
    m.submit(now, ist)
    val after = m.answered(done(answer(TryKind.WRONG, "That code is not right. 4 more tries before a wait.")))
    assertEquals(After.Stay, after)
    assertEquals(Message("That code is not right. 4 more tries before a wait.", right = false), m.message)
    assertFalse(m.busy)
    assertEquals(0, m.typed)
    m.type('2')
    assertNull(m.message)
    assertEquals(1, m.typed)
    assertTrue(m.delete())
    assertFalse(m.delete())
  }

  @Test
  fun aShortCodeIsTheEnginesToRefuseInItsOwnWords() {
    val m = WaysOutModel(Panel.CODE)
    typed(m, "134")
    m.submit(now, ist)
    assertEquals(After.Stay, m.answered(done(answer(TryKind.MALFORMED, "A code has 8 digits."))))
    assertEquals("A code has 8 digits.", m.message!!.text)
    m.submit(now, ist)
    assertEquals(After.Stay, m.answered(done(answer(TryKind.ALREADY_USED, "That code was already used today."))))
  }

  @Test
  fun theFifthWrongTryOrAWaitGoesBackToTheLockWhichShowsTheWait() {
    val m = WaysOutModel(Panel.PIN)
    typed(m, "111111")
    m.submit(now, ist)
    assertEquals(After.Close, m.answered(done(answer(TryKind.WRONG, "Too many wrong tries. The next try is at 9:47 pm.", waitUntil = now + 300))))
    val n = WaysOutModel(Panel.CODE)
    n.submit(now, ist)
    assertEquals(After.Close, n.answered(done(answer(TryKind.MUST_WAIT, "Too many wrong tries. The next try is at 9:47 pm."))))
  }

  @Test
  fun aRequestSentGoesBackToTheLockAndARefusalIsInTheCoresWords() {
    val m = WaysOutModel(Panel.ASK)
    assertFalse("asking has no digits", m.type('1'))
    assertEquals(AskChoice.MIN30, m.ask)
    m.pick(AskChoice.HOUR)
    m.reason = "To finish my science project"
    val call = m.submit(now, ist) as WayCall.Ask
    assertEquals(WayCall.Ask(60, "To finish my science project"), call)
    val req = TimeRequest("r1", "c1", "d1", 20_775L, 60, "To finish my science project", now)
    assertEquals(After.Close, m.answered(done(req)))

    val again = WaysOutModel(Panel.ASK)
    again.submit(now, ist)
    val refused = again.answered(WayOut.Refused(EngineException.RequestWaiting("A request from this device is still waiting for an answer.")))
    assertEquals(After.Stay, refused)
    assertEquals(Message("A request from this device is still waiting for an answer.", right = false), again.message)
  }

  @Test
  fun aPinGivesWhatTheGuardianChoseAndTheRightOneMayLeaveThePhoneLocked() {
    val m = WaysOutModel(Panel.PIN)
    assertEquals(PinChoice.ADD30, m.pin)
    typed(m, "482915")
    var call = m.submit(now, ist) as WayCall.Pin
    assertEquals(PinActionKind.ADD_MINUTES, call.action.kind)
    assertEquals(30, call.action.minutes)
    // At bedtime time added does not open the phone: back to the lock.
    assertEquals(After.CloseSoon, m.answered(done(answer(TryKind.ADDED, "30 minutes added."))))

    val n = WaysOutModel(Panel.PIN)
    for ((choice, until) in listOf(PinChoice.OPEN60 to now + 3_600, PinChoice.OPEN120 to now + 7_200, PinChoice.TODAY to 1_795_026_600L)) {
      n.pick(choice)
      typed(n, "482915")
      call = n.submit(now, ist) as WayCall.Pin
      assertEquals(PinActionKind.UNLOCK_UNTIL, call.action.kind)
      assertEquals(until, call.action.until)
      n.answered(WayOut.Failed)
    }
    // A minute before midnight, Today still keeps it open a minute at least.
    assertEquals(1_795_026_540L + 60, PinChoice.TODAY.action(1_795_026_540L, ist).until)
    assertEquals(listOf(15, 30, 60), PinChoice.entries.mapNotNull { it.addMinutes })
  }

  @Test
  fun backWipesWhatWasTypedAndGoesToTheLock() {
    val m = WaysOutModel(Panel.PIN)
    typed(m, "4829")
    assertEquals(null, m.digit(0))
    assertEquals(4, m.typed)
    assertEquals(After.Close, m.back())
    assertTrue(m.wiped())
  }

  @Test
  fun anAnswerThatCouldNotBeHadGoesBackToTheLock() {
    val m = WaysOutModel(Panel.CODE)
    m.submit(now, ist)
    assertEquals(After.Close, m.answered(WayOut.Failed))
    m.submit(now, ist)
    assertEquals(After.Close, m.answered(WayOut.NotPaired))
  }

  @Test
  fun theScreenRebuiltInTheMiddleOfACallGetsTheAnswerAndNeverTheDigits() {
    val relay = CallRelay()
    // The first screen types a code and sends it.
    val first = WaysOutModel(Panel.CODE)
    typed(first, "13420715")
    first.pick(PinChoice.OPEN60)
    first.submit(now, ist)
    val id = relay.begin()
    relay.attach(id) { _, _ -> error("the first screen is gone before the answer") }
    // The phone is turned: the screen is rebuilt; only what is not secret survives.
    relay.detach()
    val saved = first.saved()
    assertEquals(WaysOutModel.Saved(Panel.CODE, AskChoice.MIN30, PinChoice.OPEN60, null), saved)
    assertFalse("nothing typed is in what survives", saved.toString().contains("1342"))
    first.wipe()
    // The answer arrives while no screen is up, and is held.
    relay.answered(id, done(answer(TryKind.WRONG, "That code is not right. 4 more tries before a wait.")))
    val second = WaysOutModel(Panel.CODE)
    second.restore(saved, waiting = true)
    assertTrue(second.busy)
    assertEquals(0, second.typed)
    var got: After? = null
    assertTrue(relay.attach(id) { _, a -> got = second.answered(a) })
    assertEquals(After.Stay, got)
    assertEquals("That code is not right. 4 more tries before a wait.", second.message!!.text)
    assertFalse(second.busy)
    // Handed over once only.
    assertFalse(relay.attach(id) { _, _ -> error("twice") })
  }

  @Test
  fun theGuardsPinGoesToTheGuardWipedAndARightOneIsShownThenCloses() {
    val m = WaysOutModel(Panel.PIN, guard = true)
    typed(m, "482915")
    val call = m.submit(now, ist) as WayCall.GuardPin
    assertArrayEquals("482915".toByteArray(), call.typed)
    assertTrue(m.wiped())
    assertTrue(m.busy)
    assertNull("one call at a time", m.submit(now, ist))
    // Wrong: stays, in the engine's words; typing clears them.
    assertEquals(After.Stay, m.answered(done(answer(TryKind.WRONG, "That PIN is not right. 4 more tries before a wait."))))
    assertEquals(Message("That PIN is not right. 4 more tries before a wait.", right = false), m.message)
    typed(m, "4")
    assertNull(m.message)
    m.back()
    // Right: the engine's words in teal, then the screen closes.
    typed(m, "482915")
    m.submit(now, ist)
    assertEquals(After.CloseSoon, m.answered(done(answer(TryKind.RELEASE_CONFIRMED, "These settings are open until 9:47 pm."))))
    assertEquals(Message("These settings are open until 9:47 pm.", right = true), m.message)
    // The lock's PIN is still the lock's: with no guard, a PIN with what to give.
    val lockPin = WaysOutModel(Panel.PIN)
    typed(lockPin, "482915")
    assertTrue(lockPin.submit(now, ist) is WayCall.Pin)
  }

  @Test
  fun anAnswerForAScreenThatIsUpIsHandedOverAtOnceAndAnUnknownCallIsSaidToBeLost() {
    val relay = CallRelay()
    val id = relay.begin()
    var got = 0
    relay.attach(id) { i, _ -> got = i }
    relay.answered(id, WayOut.Failed)
    assertEquals(id, got)
    // A number from before the process started again: nothing will come.
    assertFalse(CallRelay().attach(41) { _, _ -> error("never") })
    assertTrue(CallRelay().attach(null) { _, _ -> error("never") })
  }
}
