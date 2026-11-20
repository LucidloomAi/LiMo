package ai.lucidloom.limo.child.show

import java.time.Instant
import java.time.ZoneId
import java.time.ZonedDateTime
import java.util.Locale

/**
 * The words of the covers and the ways out that are not the engine's: the
 * labels and buttons the approved drawings (design/screens/phone.js) show
 * on every lock, word for word. Every word about time, rules, a reason, a
 * way out's answer or an app comes from the engine or from Android; these
 * never change with the state. The screens are in English only.
 */
internal object Drawn {
  const val LIMO = "LiMo"
  const val THIS_PHONE = "this phone"
  const val OPENS_AGAIN = "Opens again"
  const val WHAT_YOU_CAN_DO = "What you can do"
  const val STILL_OPEN = "Still open"
  const val NOTHING_CLOSED = "Nothing was closed. Your apps are still open behind this screen."
  const val NOTHING_ELSE_CLOSED = "Nothing else was closed."
  const val MADE_BY = "Made by"
  const val MAKER = "LucidLoom AI"
  const val EMERGENCY_CALL = "Emergency call"
  const val BLOCKED = "Blocked"
  const val GO_HOME = "Go to the home screen"
  const val OK = "OK"

  // The ways out.
  const val BACK = "Back"
  const val ASK_TITLE = "Ask for more time"
  const val CODE_TITLE = "Type a one time code"
  const val PIN_TITLE = "Guardian PIN"
  const val HOW_LONG = "How long"
  const val WHY = "Why"
  const val WHY_HINT = "you can leave this empty"
  const val ASK_SMALL = "A guardian can say yes, give a different amount, or say no."
  const val SEND_REQUEST = "Send request"
  const val CODE_NOTE = "A guardian who is away from home can read you a code. It has 8 digits."
  const val CODE_SMALL = "A code works once, only today, and only on this phone."
  const val USE_THIS_CODE = "Use this code"
  const val PIN_NOTE = "For a guardian. Type your PIN and choose what to give."
  const val ADD_TIME = "Add time"
  const val KEEP_OPEN = "Or keep it open for"
  const val PIN_PLACEHOLDER = "6 to 8 digits"
  const val UNLOCK = "Unlock"
  const val REMOVE_DIGIT = "Remove the last digit"

  /** "Opens tomorrow, 7:00 am", from the engine's "Tomorrow, 7:00 am": the strip of a way out. */
  fun opens(untilWords: String): String =
    if (untilWords.isEmpty()) untilWords else "Opens " + untilWords.substring(0, 1).lowercase(Locale.ROOT) + untilWords.substring(1)

  /** "Mira · this phone", or "this phone" with no name; shown in capitals. Never logged. */
  fun eyebrow(childName: String?): String {
    val name = childName?.trim().orEmpty()
    return if (name.isEmpty()) THIS_PHONE else "$name · $THIS_PHONE"
  }

  /** "0 / 200": the characters of a reason, as the rules core counts them (code points). */
  fun count(reason: CharSequence, most: Int): String =
    "${Character.codePointCount(reason, 0, reason.length)} / $most"
}

/** Typography for words the engine writes with plain ASCII apostrophes. */
internal object Typo {
  /**
   * An ASCII apostrophe between two letters or digits as the typographic
   * one, as the drawings and the PC's screens show the engine's words:
   * "Today's" reads "Today’s". Nothing else changes.
   */
  fun quotes(text: String): String {
    if (text.indexOf('\'') < 0) return text
    val out = StringBuilder(text.length)
    for (i in text.indices) {
      val c = text[i]
      val before = i > 0 && text[i - 1].isLetterOrDigit()
      val after = i + 1 < text.length && text[i + 1].isLetterOrDigit()
      out.append(if (c == '\'' && before && after) '’' else c)
    }
    return out.toString()
  }

  private val TIME = Regex("(\\d) (am|pm)\\b")

  /** A time never breaks between its digits and am or pm, as the drawings set it: "7:00 am". */
  fun timesWhole(text: String): String = TIME.replace(text, "$1 $2")
}

/**
 * The clock at the top right of a lock and the block screen: "9:42 pm"
 * over "Wed 18 Nov", on the phone's own clock and zone, in 12 or 24 hours
 * as the phone shows them ("21:42"). Plain English names, as the engine
 * says its times.
 */
internal object ClockWords {
  private val DAYS = arrayOf("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")
  private val MONTHS = arrayOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")

  fun time(atMs: Long, zone: ZoneId, hours24: Boolean): String {
    val t = at(atMs, zone)
    return if (hours24) {
      "%02d:%02d".format(Locale.ROOT, t.hour, t.minute)
    } else {
      val h = (t.hour % 12).let { if (it == 0) 12 else it }
      "%d:%02d %s".format(Locale.ROOT, h, t.minute, if (t.hour < 12) "am" else "pm")
    }
  }

  fun date(atMs: Long, zone: ZoneId): String {
    val t = at(atMs, zone)
    return "${DAYS[t.dayOfWeek.value - 1]} ${t.dayOfMonth} ${MONTHS[t.monthValue - 1]}"
  }

  /** The end of today on the phone's clock, unix seconds: for a guardian's "Today". */
  fun endOfDay(nowSecs: Long, zone: ZoneId): Long {
    val t = Instant.ofEpochSecond(nowSecs).atZone(zone).toLocalDate().plusDays(1).atStartOfDay(zone)
    return t.toEpochSecond()
  }

  private fun at(ms: Long, zone: ZoneId): ZonedDateTime = Instant.ofEpochMilli(ms).atZone(zone)
}
