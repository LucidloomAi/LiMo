package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.GuardedPage
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The folder's own promises, checked in its files: the watch reads nothing
 * on the screen; a class name only for Android's Settings, the installer and
 * the package Android named for the device admin's page, and only to match
 * it against the guarded pages; no package name but the framework's and the
 * guarded list's; and nothing a person reads has dash punctuation.
 */
class HouseRulesTest {
  private val watchDir: File by lazy {
    var dir: File? = File(System.getProperty("user.dir") ?: ".").absoluteFile
    while (dir != null) {
      val found = File(dir, "src/main/java/ai/lucidloom/limo/child/watch")
      if (found.isDirectory) return@lazy found
      dir = dir.parentFile
    }
    error("the watch folder was not found from the tests' working folder")
  }

  private val testDir: File by lazy { File(watchDir.path.replace("src${File.separator}main", "src${File.separator}test")) }

  private fun sources(dir: File) = dir.listFiles { f -> f.name.endsWith(".kt") }.orEmpty().sortedBy { it.name }

  /** The code of a line: what is before a line comment, and nothing of a doc comment. */
  private fun code(line: String): String? {
    val code = line.substringBefore("//").trim()
    return if (code.startsWith("*") || code.startsWith("/*")) null else code
  }

  /** The module's other source sets: src/release/java, src/debug/watch, src/debug/java. */
  private fun moduleFile(vararg parts: String): File {
    val src = watchDir.path.substringBefore("${File.separator}main${File.separator}")
    return File(src, parts.joinToString(File.separator))
  }

  private val readsAClassName = Regex("""\.className\b|getClassName""")

  /** The names of a class's own fields, what an instance keeps. */
  private fun fieldsOf(type: Class<*>): Set<String> =
    type.declaredFields.filter { !it.isSynthetic && !java.lang.reflect.Modifier.isStatic(it.modifiers) }.map { it.name }.toSet()

  /**
   * The rule a class name is read by, as the watch keeps it: for windows,
   * Settings, the installer and the package Android named for the device
   * admin's page; for resumed activities, Settings and that package; never
   * any other app, whatever it claims, whatever its activities are named.
   */
  @Test
  fun aClassNameIsReadOnlyForSettingsTheInstallerAndThePackageAndroidNamedNeverAnyOther() {
    val maker = "org.example.maker.security"
    val makerPage = "org.example.maker.security.AdminPage"
    val access = "org.example.maker.accessibility"
    val adminPage = "com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd"
    val phone = TestPhone().apply {
      // A child's own game claims the device admin's intent, chosen to open it
      // always, and names an activity like the device admin's page.
      answers[PageIntent.DEVICE_ADMIN] = IntentAnswer(
        installed(GAME, adminPage),
        listOf(installed(GAME, adminPage), preinstalled(maker, makerPage)),
      )
      // It claims the accessibility settings too, so Android would show its chooser.
      answers[PageIntent.ACCESSIBILITY] = IntentAnswer(
        preinstalled("android", "com.android.internal.app.ResolverActivity"),
        listOf(installed(GAME, "org.example.game.Access"), preinstalled(access, "org.example.maker.accessibility.Main")),
      )
      answers[PageIntent.SETTINGS] = answering(preinstalled("com.android.settings", "com.android.settings.Settings"))
    }
    val rig = Rig(phone)
    val reach = rig.core.reachNow()
    // Only preinstalled answers are believed, and never Android's chooser.
    assertEquals(setOf(maker to makerPage), reach.adminPages)
    assertEquals(setOf(access), reach.accessibilityPackages)
    for (pkg in listOf(GAME, LAUNCHER, LIMO, SYSTEM_UI, "android", access, "")) {
      assertFalse(pkg, reach.readsClassName(pkg))
      assertFalse(pkg, reach.readsResumedClassName(pkg))
    }
    assertFalse(reach.readsClassName(null))
    for (pkg in Guarded.INSTALLERS) {
      assertTrue(pkg, reach.readsClassName(pkg))
      assertFalse(pkg, reach.readsResumedClassName(pkg))
    }
    for (pkg in listOf("com.android.settings", maker)) {
      assertTrue(pkg, reach.readsClassName(pkg))
      assertTrue(pkg, reach.readsResumedClassName(pkg))
    }
    // The game opens activities named like the device admin's pages, by its
    // window and in Android's record: never read, never guarded.
    rig.open(LAUNCHER)
    rig.open(GAME, activity = adminPage)
    rig.open(GAME, activity = makerPage)
    rig.phone.pass(WatchTimes.CONFIRM_GRACE_MS)
    assertNull(rig.facts().guarded)
    assertNull(reach.page(GAME, adminPage))
    assertNull(reach.page(GAME, makerPage))
    // The maker's page itself is.
    rig.open(maker, activity = makerPage, window = false)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.facts().guarded)
    // Every class read from the record was of a package the reach names.
    assertTrue(phone.classReads.toString(), phone.classReads.isNotEmpty() && phone.classReads.all { it == maker || it == "com.android.settings" })
    // And the log names none of them.
    for (name in listOf(adminPage, makerPage, "AdminPage", "ResolverActivity", "Access")) {
      assertFalse(name, rig.log.lines.any { it.contains(name) })
    }
  }

  @Test
  fun theWatchReadsNothingOnTheScreenAndAClassNameOnlyForSettingsAndTheInstaller() {
    // What would read a window's content or its text.
    val forbidden = listOf(
      Regex("""AccessibilityNodeInfo"""),
      Regex("""rootInActiveWindow|getRootInActiveWindow"""),
      Regex("""\.windows\b|getWindows"""),
      Regex("""\.source\b|getSource"""),
      Regex("""contentDescription|getContentDescription"""),
      Regex("""\.text\b(?!\.)|getText\("""),
      Regex("""setServiceInfo|serviceInfo\.flags"""),
    )
    val release = moduleFile("release", "java", "ai", "lucidloom", "limo", "child", "watch", "WindowPage.kt")
    val debug = moduleFile("debug", "watch", "ai", "lucidloom", "limo", "child", "watch", "WindowPage.kt")
    val files = sources(watchDir) + release + debug
    assertTrue(files.size >= 12)
    val classNameReads = ArrayList<String>()
    for (file in files) {
      file.readLines().forEachIndexed { i, line ->
        val code = code(line) ?: return@forEachIndexed
        for (rule in forbidden) assertFalse("${file.name}:${i + 1} reads the screen: $line", rule.containsMatchIn(code))
        if (readsAClassName.containsMatchIn(code)) classNameReads += "${file.name}: $code"
      }
    }
    // A class name read from Android is read in two places, each only after
    // asking the guard's reach that the package is one it reads, and only to
    // hand it to WindowPage, which matches it and drops it: the service, for
    // a window; AndroidPhone, for a resumed activity in the usage record.
    assertEquals(classNameReads.toString(), 2, classNameReads.size)
    val window = classNameReads.single { it.startsWith("WatchService.kt: ") }
    assertTrue(window, window.contains("val page = if (reach.readsClassName(packageName)) WindowPage.of(reach, packageName, event.className?.toString()) else null"))
    val resumed = classNameReads.single { it.startsWith("AndroidPhone.kt: ") }
    assertTrue(
      resumed,
      resumed.contains("val page = if (kind == UsageKind.RESUMED && reach.readsResumedClassName(pkg)) WindowPage.resumed(reach, pkg, event.className) else null"),
    )
    // The package manager's own names for the guard's intents are taken in
    // one place, AndroidPhone's answer, and nowhere else.
    val managerNames = files.flatMap { file ->
      file.readLines().mapNotNull(::code).filter { Regex("""targetActivity|\binfo\.name\b""").containsMatchIn(it) }.map { "${file.name}: $it" }
    }
    assertEquals(managerNames.toString(), 1, managerNames.size)
    assertTrue(managerNames.single(), managerNames.single().startsWith("AndroidPhone.kt: NamedActivity(info.packageName, info.name, info.targetActivity,"))
    // The release copy of WindowPage only matches: no log, no probe.
    val releaseCode = release.readLines().mapNotNull(::code).joinToString("\n")
    assertEquals(2, Regex("""= reach\.page\(packageName, className\)""").findAll(releaseCode).count())
    for (word in listOf("Log", "log", "probe", "Probe", "println", "print(")) assertFalse(word, releaseCode.contains(word))
    // The debug copy hands each to the probe, then matches it as release does.
    val debugCode = debug.readLines().mapNotNull(::code).joinToString("\n")
    assertTrue(debugCode.contains("PageProbe.window(reach, packageName, className)"))
    assertTrue(debugCode.contains("PageProbe.resumed(reach, packageName, className)"))
    assertEquals(2, Regex("""return reach\.page\(packageName, className\)""").findAll(debugCode).count())
    // The fixed list's own rule: Settings and the installer, and pages only of them.
    assertEquals(setOf("com.android.settings", "com.android.packageinstaller", "com.google.android.packageinstaller"), Guarded.CLASS_NAME_PACKAGES)
    assertTrue(Guarded.PAGES.keys.all { it.first in Guarded.CLASS_NAME_PACKAGES })
    // A usage event never keeps a class: its package, kind, time and the page matched.
    assertEquals(setOf("packageName", "kind", "atWallMs", "page"), fieldsOf(UsageEvent::class.java))
    // The reach keeps only what the package manager answered for the device
    // admin's page (packages and classes) and packages for the accessibility settings.
    assertEquals(setOf("adminPages", "accessibilityPackages", "accessibility", "accessibilityIn", "adminPackages"), fieldsOf(GuardReach::class.java))
    // No log line of the watch holds a class name: none names one.
    for (file in sources(watchDir)) {
      for (line in file.readLines()) {
        if (!line.contains("log.line(") && !line.contains("AndroidWatchLog.line(")) continue
        assertFalse("${file.name}: $line", line.contains("className") || line.contains("cls"))
      }
    }
    // A caller's number is never read: it is a parameter Android passes, named once and unused.
    val signals = File(watchDir, "WatchSignals.kt").readText()
    assertTrue(Regex("""phoneNumber""").findAll(signals).count() == 1)
  }

  @Test
  fun packageNamesAreHeldOnlyInTheFrameworksListAndTheGuardedList() {
    // Which app plays which part is asked of Android on the phone; the only
    // names written in the watch are the framework's own overlays (Roles.kt)
    // and the guarded list (Guarded.kt), each entry there with its reason.
    val packageLike = Regex(""""[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+"""")
    for (file in sources(watchDir)) {
      val names = file.readLines().mapNotNull(::code).flatMap { c -> packageLike.findAll(c).map { it.value }.toList() }
      when (file.name) {
        "Roles.kt" -> assertTrue(names.toString(), names.all { it.startsWith("\"com.android.") || it.startsWith("\"com.google.android.") })
        "Guarded.kt" -> assertTrue(names.isNotEmpty())
        else -> assertTrue("${file.name} names a package: $names", names.isEmpty())
      }
    }
    // A maker's name, anything but Android's and Google's own, is in the guarded list only.
    assertTrue(Guarded.PACKAGES.keys.any { !it.startsWith("com.android.") && !it.startsWith("com.google.android.") })
  }

  @Test
  fun noDashPunctuationInAnythingAPersonReads() {
    val em = 0x2014.toChar()
    val en = 0x2013.toChar()
    val files = sources(watchDir) + sources(testDir) + File(watchDir, "README.md")
    for (file in files) {
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
        // Table rows, in a README or a comment, have their own rules.
        if (body.startsWith("|")) return@forEachIndexed
        assertFalse("$at has a hyphen used as a dash", body.contains(" - "))
        assertFalse("$at has a double hyphen as a dash", body.contains(" --") || body.contains("-- "))
      }
    }
  }
}
