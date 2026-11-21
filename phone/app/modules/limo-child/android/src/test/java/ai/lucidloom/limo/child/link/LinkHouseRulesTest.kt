package ai.lucidloom.limo.child.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * The house rules that keep the phone talking to the family's PC and to
 * nothing else: only link/ opens a socket, never a server and never one
 * of datagrams; no web address, web view, web request or analytics
 * anywhere in the child module or the app's screens; the link's only
 * address written down is the phone's own loopback, and no port is
 * written down at all (the PC gives it); and a connection is never closed
 * one way alone.
 */
class LinkHouseRulesTest {
  private val repo = File(System.getProperty("limo.repo") ?: "../../../../..")
  private val app = File(repo, "phone/app")
  private val module = File(app, "modules/limo-child/android/src/main/java/ai/lucidloom/limo/child")
  private val linkFolder = File(module, "link")

  private fun kotlin(folder: File): List<File> = folder.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()

  /** The code of a Kotlin file without its comments, so that the rules read what runs. */
  private fun code(file: File): String {
    val text = file.readText()
    val noBlocks = Regex("/\\*.*?\\*/", RegexOption.DOT_MATCHES_ALL).replace(text, " ")
    return noBlocks.lines().joinToString("\n") { line -> line.substringBefore("//") }
  }

  @Test
  fun onlyTheLinkOpensASocketAndNeverAServerADatagramOrAWebRequest() {
    val files = kotlin(module)
    assertTrue("the module's Kotlin is found", files.size > 20)
    val network = Regex("""\b(java\.net\.|Socket\b|SocketFactory|InetAddress|InetSocketAddress|NsdManager|ConnectivityManager)""")
    val never = Regex("""\b(ServerSocket|DatagramSocket|MulticastSocket|URLConnection|HttpURLConnection|HttpsURLConnection|okhttp|OkHttp|WebView|WebSocket|shutdownOutput|shutdownInput)\b|\bURL\(|https?://""")
    for (file in files) {
      val text = code(file)
      never.find(text)?.let { throw AssertionError("${file.name}: ${it.value}") }
      if (!file.path.startsWith(linkFolder.path)) {
        network.find(text)?.let { throw AssertionError("${file.name} reaches the network (${it.value}); only link/ may") }
      }
    }
  }

  @Test
  fun theLinkWritesDownNoAddressButTheLoopbackAndNoPort() {
    val files = kotlin(linkFolder)
    assertTrue(files.isNotEmpty())
    val dotted = Regex(""""\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}"""")
    val found = files.flatMap { f -> dotted.findAll(code(f)).map { it.value }.toList() }.toSet()
    assertEquals(setOf("\"127.0.0.1\""), found)
    // The port is the PC's, in the offer and the kept pairing: never here.
    for (f in files) assertTrue(f.name, !code(f).contains("47810"))
  }

  @Test
  fun theAppsScreensMakeNoWebRequestAndCarryNoAnalytics() {
    val screens = File(app, "src").walkTopDown().filter { it.isFile && (it.extension == "ts" || it.extension == "tsx") }.toList()
    assertTrue(screens.size > 10)
    val requests = Regex("""\b(fetch\s*\(|XMLHttpRequest|new\s+WebSocket|EventSource|openURL|sendBeacon)|from\s+['"](react-native-webview|@expo/dom-webview|expo-web-browser|expo-linking)['"]""")
    for (file in screens) {
      val text = Regex("/\\*.*?\\*/", RegexOption.DOT_MATCHES_ALL).replace(file.readText(), " ")
        .lines().joinToString("\n") { it.substringBefore("//") }
      requests.find(text)?.let { throw AssertionError("${file.name}: ${it.value}") }
    }
    val manifest = File(app, "package.json").readText()
    val dependencies = Regex(""""(dependencies|devDependencies)"\s*:\s*\{([^}]*)\}""").findAll(manifest).joinToString(" ") { it.groupValues[2] }
    for (name in listOf(
      "firebase", "sentry", "bugsnag", "crashlytics", "amplitude", "mixpanel", "segment", "appcenter", "datadog",
      "newrelic", "posthog", "analytics", "expo-updates", "expo-insights", "react-native-webview", "expo-web-browser",
    )) {
      assertTrue("package.json names $name", !dependencies.contains(name, ignoreCase = true))
    }
  }
}
