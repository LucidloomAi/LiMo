package ai.lucidloom.limo.child.seam

/**
 * A small, strict JSON reader for the engine's view: text in, plain Kotlin
 * values out. An object is a `Map<String, Any?>` in the order written, an
 * array a `List<Any?>`, a string a `String`, a whole number a `Long`, any
 * other number a `Double`, and `true`, `false` and `null` themselves.
 *
 * Pure Kotlin, so the JVM tests read the engine's real view exactly as the
 * app does (Android's own org.json is not there in a JVM test). Anything that
 * is not JSON is refused with [Bad], whose words never quote the text.
 */
internal object SeamJson {
  class Bad(why: String) : Exception(why)

  fun read(text: String): Any? {
    val r = Reader(text)
    r.space()
    val value = r.value(depth = 0)
    r.space()
    if (!r.done) throw Bad("text after the value")
    return value
  }

  /** Deeper than this is refused: the view is a few levels deep. */
  private const val MAX_DEPTH = 32

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

    fun value(depth: Int): Any? {
      if (depth > MAX_DEPTH) throw Bad("too deep")
      return when (peek()) {
        '{' -> obj(depth)
        '[' -> array(depth)
        '"' -> string()
        't' -> word("true", true)
        'f' -> word("false", false)
        'n' -> word("null", null)
        else -> number()
      }
    }

    private fun obj(depth: Int): Map<String, Any?> {
      expect('{')
      val out = LinkedHashMap<String, Any?>()
      space()
      if (peek() == '}') {
        next()
        return out
      }
      while (true) {
        space()
        val name = string()
        space()
        expect(':')
        space()
        if (out.containsKey(name)) throw Bad("a name twice")
        out[name] = value(depth + 1)
        space()
        when (next()) {
          ',' -> continue
          '}' -> return out
          else -> throw Bad("expected , or }")
        }
      }
    }

    private fun array(depth: Int): List<Any?> {
      expect('[')
      val out = ArrayList<Any?>()
      space()
      if (peek() == ']') {
        next()
        return out
      }
      while (true) {
        space()
        out += value(depth + 1)
        space()
        when (next()) {
          ',' -> continue
          ']' -> return out
          else -> throw Bad("expected , or ]")
        }
      }
    }

    private fun word(word: String, value: Any?): Any? {
      if (!text.startsWith(word, at)) throw Bad("not JSON")
      at += word.length
      return value
    }

    private fun number(): Any {
      val m = NUMBER.matcher(text).region(at, text.length)
      if (!m.lookingAt()) throw Bad("not JSON")
      val t = m.group()
      at += t.length
      if (t.none { it == '.' || it == 'e' || it == 'E' }) t.toLongOrNull()?.let { return it }
      val d = t.toDouble()
      if (!d.isFinite()) throw Bad("a number too big")
      return d
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
  }

  private val NUMBER = java.util.regex.Pattern.compile("""-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?""")
}
