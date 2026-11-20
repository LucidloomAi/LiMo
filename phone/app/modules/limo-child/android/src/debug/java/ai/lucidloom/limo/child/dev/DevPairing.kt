package ai.lucidloom.limo.child.dev

import java.io.File

/**
 * The test family's pairing file, pushed over the USB cable by the
 * development tool on the PC (`phone/devtool`) through `run-as`, into the
 * app's private `files/limo-dev/pairing.json`:
 *
 * ```
 * { "welcome": { ...the core's Grant... }, "code_secret_hex": "<64 hex digits>", "made_at": 1795000000 }
 * ```
 *
 * `welcome` is the master's welcome grant as a JSON object, passed to the
 * engine as it is; `code_secret_hex` the phone's 32 byte code secret;
 * `made_at` the PC's clock when it wrote the file, in unix seconds.
 *
 * Debug builds only (src/debug): a release build has none of this.
 */
internal object DevPairing {
  /** A file whose `made_at` is further than this from the phone's clock is refused: left over, not just pushed. */
  const val MAX_AGE_SECS = 60L * 60

  /** What the file gave. */
  sealed class Taken {
    /** The welcome's JSON and the code secret, as bytes: fill both with zeros once the engine has them. */
    class Pairing(val welcome: ByteArray, val codeSecret: ByteArray) : Taken()

    /** There was no file. */
    data object NoFile : Taken()

    /** The file was not a pairing file LiMo takes; [why] is a short word for the log, never what it held. */
    data class Refused(val why: String) : Taken()
  }

  /**
   * Takes the pairing file once: reads it, deletes it whatever it held (a
   * pairing is pushed again for another try, never read twice), and reads
   * what it gives. [nowSecs] is the phone's clock.
   */
  fun take(file: File, nowSecs: Long): Taken {
    if (!file.exists()) return Taken.NoFile
    val bytes = try {
      file.readBytes()
    } catch (_: Exception) {
      file.delete()
      return Taken.Refused("unreadable")
    }
    if (!file.delete()) {
      bytes.fill(0)
      return Taken.Refused("not_deleted")
    }
    return try {
      read(String(bytes, Charsets.UTF_8), nowSecs)
    } finally {
      bytes.fill(0)
    }
  }

  /** Reads a pairing file's [text]. */
  fun read(text: String, nowSecs: Long): Taken {
    val members = try {
      DevJson.members(text)
    } catch (_: DevJson.Bad) {
      return Taken.Refused("not_json")
    }
    val welcome = members["welcome"]?.takeIf { it.startsWith("{") } ?: return Taken.Refused("no_welcome")
    val hex = members["code_secret_hex"]?.let { raw -> runCatching { DevJson.string(raw) }.getOrNull() }
      ?: return Taken.Refused("no_code_secret")
    val madeAt = members["made_at"]?.let { raw -> runCatching { DevJson.long(raw) }.getOrNull() }
      ?: return Taken.Refused("no_made_at")
    if (madeAt < nowSecs - MAX_AGE_SECS || madeAt > nowSecs + MAX_AGE_SECS) return Taken.Refused("old_file")
    val secret = fromHex(hex) ?: return Taken.Refused("code_secret_not_hex")
    if (secret.size != 32) {
      secret.fill(0)
      return Taken.Refused("code_secret_not_32_bytes")
    }
    return Taken.Pairing(welcome.toByteArray(Charsets.UTF_8), secret)
  }

  private fun fromHex(hex: String): ByteArray? {
    if (hex.length % 2 != 0) return null
    val out = ByteArray(hex.length / 2)
    for (i in out.indices) {
      val hi = Character.digit(hex[2 * i], 16)
      val lo = Character.digit(hex[2 * i + 1], 16)
      if (hi < 0 || lo < 0) {
        out.fill(0)
        return null
      }
      out[i] = ((hi shl 4) or lo).toByte()
    }
    return out
  }
}

/**
 * A small, strict JSON reader for the pairing file: the members of the top
 * level object, each value as the very text it is in the file, so the
 * welcome reaches the engine as it was written.
 */
internal object DevJson {
  class Bad(why: String) : Exception(why)

  /** The top level object's members: each name, and its value's text. */
  fun members(text: String): Map<String, String> {
    val r = Reader(text)
    r.space()
    r.expect('{')
    val out = LinkedHashMap<String, String>()
    r.space()
    if (r.peek() == '}') {
      r.next()
    } else {
      while (true) {
        r.space()
        val name = r.string()
        r.space()
        r.expect(':')
        r.space()
        val start = r.at
        r.value()
        out[name] = text.substring(start, r.at)
        r.space()
        when (r.next()) {
          ',' -> continue
          '}' -> break
          else -> throw Bad("expected , or }")
        }
      }
    }
    r.space()
    if (!r.done) throw Bad("text after the object")
    return out
  }

  /** A JSON string's value, from its text. */
  fun string(raw: String): String {
    val r = Reader(raw.trim())
    val s = r.string()
    if (!r.done) throw Bad("not a string")
    return s
  }

  /** A JSON whole number's value, from its text. */
  fun long(raw: String): Long {
    val t = raw.trim()
    if (!Regex("-?(0|[1-9][0-9]*)").matches(t)) throw Bad("not a whole number")
    return t.toLongOrNull() ?: throw Bad("too big")
  }

  private class Reader(val text: String) {
    var at = 0
    val done: Boolean get() = at >= text.length

    fun peek(): Char = if (done) throw Bad("cut short") else text[at]

    fun next(): Char = peek().also { at += 1 }

    fun expect(c: Char) {
      if (next() != c) throw Bad("expected $c")
    }

    fun space() {
      while (!done && text[at] in " \t\r\n") at += 1
    }

    fun string(): String {
      expect('"')
      val out = StringBuilder()
      while (true) {
        val c = next()
        when {
          c == '"' -> return out.toString()
          c == '\\' -> when (val e = next()) {
            '"', '\\', '/' -> out.append(e)
            'b' -> out.append('\b')
            'f' -> out.append('\u000C')
            'n' -> out.append('\n')
            'r' -> out.append('\r')
            't' -> out.append('\t')
            'u' -> {
              if (at + 4 > text.length) throw Bad("cut short")
              val code = text.substring(at, at + 4).toIntOrNull(16) ?: throw Bad("bad escape")
              out.append(code.toChar())
              at += 4
            }
            else -> throw Bad("bad escape")
          }
          c < ' ' -> throw Bad("control character in a string")
          else -> out.append(c)
        }
      }
    }

    fun value() {
      when (peek()) {
        '{' -> container('{', '}', members = true)
        '[' -> container('[', ']', members = false)
        '"' -> string()
        't' -> word("true")
        'f' -> word("false")
        'n' -> word("null")
        else -> number()
      }
    }

    private fun container(open: Char, close: Char, members: Boolean) {
      expect(open)
      space()
      if (peek() == close) {
        next()
        return
      }
      while (true) {
        space()
        if (members) {
          string()
          space()
          expect(':')
          space()
        }
        value()
        space()
        when (next()) {
          ',' -> continue
          close -> return
          else -> throw Bad("expected , or $close")
        }
      }
    }

    private fun word(w: String) {
      if (!text.startsWith(w, at)) throw Bad("expected $w")
      at += w.length
    }

    private fun number() {
      val start = at
      while (!done && (text[at].isDigit() || text[at] in "+-.eE")) at += 1
      if (start == at || text.substring(start, at).toDoubleOrNull() == null) throw Bad("not a value")
    }
  }
}
