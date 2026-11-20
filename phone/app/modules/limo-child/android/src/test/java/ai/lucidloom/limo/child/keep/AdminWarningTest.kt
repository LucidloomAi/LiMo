package ai.lucidloom.limo.child.keep

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The device admin's answer when Android asks whether it may be switched
 * off: never empty, since Settings switches the admin off at once, with no
 * dialog for the guard to cover, when the answer is empty or late.
 */
class AdminWarningTest {
  @Test
  fun theAnswerIsLiMosWordsAndNeverEmpty() {
    assertEquals("Switching this off lets LiMo be removed from this phone.", AdminReceiver.warning { "Switching this off lets LiMo be removed from this phone." })
    for (read in listOf<() -> CharSequence?>({ null }, { "" }, { "   " }, { throw IllegalStateException("no resources") })) {
      val words = AdminReceiver.warning(read)
      assertEquals(AdminReceiver.WARNING_FALLBACK, words)
      assertTrue(words.isNotBlank())
    }
  }

  @Test
  fun theWordsAreInTheModulesStringsAndPlain() {
    var dir: File? = File(System.getProperty("user.dir") ?: ".").absoluteFile
    var strings: File? = null
    while (dir != null && strings == null) {
      strings = File(dir, "src/main/res/values/limo_strings.xml").takeIf { it.isFile }
      dir = dir.parentFile
    }
    val text = (strings ?: error("limo_strings.xml was not found")).readText()
    val words = Regex("""<string name="limo_admin_disable_warning">([^<]+)</string>""").find(text)?.groupValues?.get(1)
    assertEquals("Switching this off lets LiMo be removed from this phone.", words)
    assertFalse(words!!.contains(" - ") || words.contains(0x2014.toChar()) || words.contains(0x2013.toChar()))
  }
}
