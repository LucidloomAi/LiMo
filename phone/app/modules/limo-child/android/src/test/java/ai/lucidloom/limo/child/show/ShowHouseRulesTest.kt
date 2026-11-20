package ai.lucidloom.limo.child.show

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The promises of show/, checked in its files: no log line carries a name,
 * a label, the engine's words, a reason, a code or a PIN; nothing reads
 * another app's screen; the covers are accessibility overlays and nothing
 * else; a typed code or PIN never becomes a string; the ways out's window
 * cannot be captured; and nothing a person reads has dash punctuation.
 */
class ShowHouseRulesTest {
  private val moduleDir: File by lazy {
    var dir: File? = File(System.getProperty("user.dir") ?: ".").absoluteFile
    while (dir != null) {
      if (File(dir, "src/main/java/ai/lucidloom/limo/child/show").isDirectory) return@lazy dir
      dir = dir.parentFile
    }
    error("the module was not found from the tests' working folder")
  }

  private val main by lazy { File(moduleDir, "src/main/java/ai/lucidloom/limo/child/show") }
  private val tests by lazy { File(moduleDir, "src/test/java/ai/lucidloom/limo/child/show") }

  private fun kt(dir: File) = dir.listFiles { f -> f.name.endsWith(".kt") }.orEmpty().sortedBy { it.name }

  // Line endings are made one kind, so a checkout with Windows line endings reads the same.
  private fun code(f: File) = f.readText().replace("\r\n", "\n")

  @Test
  fun noLogLineCarriesANameALabelWordsAReasonACodeOrAPin() {
    val logCall = Regex("""Log\.[a-z]\(""")
    val forbidden = Regex("""\$\{?[A-Za-z_.?]*(name|label|reason|message|words|title|typed|digit|pin\b|code\b|text|eyebrow|until)""", RegexOption.IGNORE_CASE)
    val sources = kt(main)
    assertTrue(sources.size >= 10)
    var lines = 0
    for (file in sources) {
      file.readLines().forEachIndexed { i, line ->
        if (!logCall.containsMatchIn(line)) return@forEachIndexed
        lines += 1
        // A package name may be logged, never anything a person reads.
        val cleaned = line.replace("\${p.packageName}", "").replace("\${e.javaClass.simpleName}", "")
        assertFalse("${file.name}:${i + 1} logs what it must not: $line", forbidden.containsMatchIn(cleaned))
      }
    }
    assertTrue("show/ logs what it does", lines >= 5)
  }

  @Test
  fun nothingReadsAnotherAppsScreenAndTheCoversAreAccessibilityOverlays() {
    val all = kt(main).joinToString("\n") { code(it) }
    for (bad in listOf("rootInActiveWindow", "getRootInActiveWindow", ".windows", "findAccessibilityNodeInfos", "getSource(", "TYPE_APPLICATION_OVERLAY", "TYPE_SYSTEM_ALERT", "SYSTEM_ALERT_WINDOW", "takeScreenshot")) {
      assertFalse("show/ uses $bad", all.contains(bad))
    }
    assertTrue(all.contains("TYPE_ACCESSIBILITY_OVERLAY"))
    assertTrue("covers never take the keys or the keyboard", code(File(main, "CoverShower.kt")).contains("FLAG_NOT_FOCUSABLE"))
    assertTrue("the warning lets touches outside it through", code(File(main, "CoverShower.kt")).contains("FLAG_NOT_TOUCH_MODAL"))
  }

  @Test
  fun aTypedCodeOrPinNeverBecomesAStringAndTheWaysOutCannotBeCaptured() {
    val model = code(File(main, "WaysOutModel.kt"))
    assertTrue(model.contains("private val digits = CharArray("))
    for (bad in listOf("String(digits", "digits.concatToString", "digits.joinToString", "String(typed", "typed.decodeToString", "typed.toString(")) {
      assertFalse("WaysOutModel makes a string of what was typed: $bad", model.contains(bad))
    }
    for (file in kt(main)) assertFalse("${file.name} logs", file.name == "WaysOutModel.kt" && code(file).contains("Log."))
    val activity = code(File(main, "WaysOutActivity.kt"))
    assertTrue(activity.contains("FLAG_SECURE"))
    assertTrue(activity.contains("setRecentsScreenshotEnabled(false)"))
    // What survives a rebuilt screen: the choices, the message and the call's number; never the digits.
    val saved = activity.substringAfter("override fun onSaveInstanceState").substringBefore("\n  }\n")
    assertFalse(saved.contains("digit") || saved.contains("typed"))
  }

  @Test
  fun noDashPunctuationInAnythingAPersonReads() {
    val em = 0x2014.toChar()
    val en = 0x2013.toChar()
    val readme = File(main, "README.md")
    assertTrue("show/README.md is missing", readme.isFile)
    val all = kt(main) + kt(tests) + readme
    for (file in all) {
      val markdown = file.name.endsWith(".md")
      var fenced = false
      file.readLines().forEachIndexed { i, line ->
        val at = "${file.name}:${i + 1}"
        assertFalse("$at has a long dash", line.contains(em) || line.contains(en))
        val trimmed = line.trim()
        if (trimmed.startsWith("```")) {
          fenced = !fenced
          return@forEachIndexed
        }
        if (fenced) return@forEachIndexed
        val prose = when {
          markdown -> if (line.startsWith("    ") || trimmed.startsWith("|")) null else trimmed
          trimmed.startsWith("//") -> trimmed.removePrefix("//")
          trimmed.startsWith("/**") -> trimmed.removePrefix("/**")
          trimmed.startsWith("*") -> trimmed.removePrefix("*")
          else -> line.substringAfter("// ", "").ifEmpty { null }
        } ?: return@forEachIndexed
        val body = prose.trim().removePrefix("- ")
        if (body.startsWith("|")) return@forEachIndexed
        assertFalse("$at has a hyphen used as a dash", body.contains(" - "))
        assertFalse("$at has a double hyphen as a dash", body.contains(" --") || body.contains("-- "))
      }
    }
    // The words the screens show, too.
    for (f in Drawn::class.java.declaredFields.filter { it.type == String::class.java }) {
      f.isAccessible = true
      val w = f.get(null) as String
      assertFalse("$w has a dash", w.contains(em) || w.contains(en) || w.contains(" - "))
    }
  }
}
