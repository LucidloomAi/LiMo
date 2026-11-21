package ai.lucidloom.limo.child.link

import ai.lucidloom.limo.child.ffi.AppEntry
import ai.lucidloom.limo.child.watch.Watch
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import android.util.AtomicFile
import android.util.Log
import java.io.File
import java.io.IOException
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

/**
 * The link on the phone: [LinkCore] on a thread of its own ("LiMo link"),
 * with Android's sockets, the Wi-Fi network, the announcement on the home
 * network, the kept pairing and the app list around it. The engine's host
 * (host/) makes one and starts it; nothing else does.
 */
class LinkHost internal constructor(context: Context, side: LinkHostSide) {
  private val app: Context = context.applicationContext ?: context
  private val wifi = WifiNetwork(app)
  private val finder = NsdFinder(app, wifi)
  private var thread: Thread? = null

  /** The core, which the host asks whether the master is in reach and hands its reports. */
  val core: LinkCore = LinkCore(
    host = side,
    store = AtomicPairingStore(File(app.filesDir, "limo/paired")),
    dialer = WifiDialer(wifi),
    clock = AndroidLinkClock,
    finder = finder,
    apps = WatchApps(app),
    self = { phoneSelf(app) },
    log = AndroidLinkLog,
  )

  /** Starts the link's thread, once. Any thread. */
  fun start() {
    synchronized(this) {
      if (thread != null) return
      thread = Thread({ run() }, "LiMo link").apply {
        isDaemon = true
        start()
      }
    }
    wifi.start { core.poke() }
  }

  private fun run() {
    while (true) {
      val wait = try {
        core.step()
      } catch (e: Exception) {
        AndroidLinkLog.line("link failed what=step error=${e.javaClass.simpleName}")
        LinkCore.FIRST_WAIT_MS
      } catch (e: LinkageError) {
        AndroidLinkLog.line("link failed what=step error=${e.javaClass.simpleName}")
        LinkCore.MAX_WAIT_MS
      }
      core.waitForWork(wait)
    }
  }
}

/** The kept pairing in one file, written all or nothing with Android's AtomicFile, out of backups as every file of the app. */
internal class AtomicPairingStore(private val file: File) : PairingStore {
  private val atomic = AtomicFile(file)

  override fun read(): ByteArray? {
    if (!file.exists() && !File(file.path + ".bak").exists()) return null
    return atomic.readFully()
  }

  override fun write(bytes: ByteArray) {
    file.parentFile?.mkdirs()
    val out = atomic.startWrite()
    try {
      out.write(bytes)
      atomic.finishWrite(out)
    } catch (e: IOException) {
      atomic.failWrite(out)
      throw e
    }
  }

  override fun delete() = atomic.delete()
}

/**
 * One connection over a socket. Reads wait at most as long as asked; a close
 * closes the socket both ways at once, never its sending side alone.
 */
class SocketConn(private val socket: Socket) : LinkConn {
  private val input = socket.getInputStream()
  private val output = socket.getOutputStream()
  private val buffer = ByteArray(64 * 1024)

  override fun write(bytes: ByteArray) {
    output.write(bytes)
    output.flush()
  }

  override fun read(waitMs: Long): ByteArray? {
    socket.soTimeout = waitMs.coerceIn(1L, 60_000L).toInt()
    return try {
      val n = input.read(buffer)
      if (n < 0) null else buffer.copyOf(n)
    } catch (e: SocketTimeoutException) {
      ByteArray(0)
    }
  }

  override fun close() {
    try {
      socket.close()
    } catch (_: IOException) {
      // Closed already.
    }
  }

  companion object {
    /** How long a connection may take to be made: the cable answers at once, the home network may not. */
    const val CABLE_CONNECT_MS = 1_000
    const val WIFI_CONNECT_MS = 3_000

    /**
     * A connection to [route], made with [socket] (one of the Wi-Fi
     * network's own, or a plain one for the cable). The address is the
     * route's own four bytes: no name is ever looked up.
     */
    fun connect(socket: Socket, route: Route): SocketConn {
      val bytes = Addresses.parse(route.address) ?: throw IOException("not an address")
      val allowed = when (route.way) {
        Way.CABLE -> route.address == Addresses.LOOPBACK
        Way.WIFI -> Addresses.isHome(route.address)
      }
      if (!allowed || route.port !in 1..65535) throw IOException("not an address of the family's PC")
      try {
        socket.tcpNoDelay = true
        val wait = if (route.way == Way.CABLE) CABLE_CONNECT_MS else WIFI_CONNECT_MS
        socket.connect(InetSocketAddress(InetAddress.getByAddress(bytes), route.port), wait)
      } catch (e: IOException) {
        socket.close()
        throw e
      }
      return SocketConn(socket)
    }
  }
}

/**
 * Opens connections: the cable's through a plain socket to the phone's own
 * loopback, the home network's through the Wi-Fi network's own socket
 * factory, so they go over Wi-Fi even when Android has made mobile data the
 * default network (proof four). With no Wi-Fi, the home network is not
 * tried.
 */
internal class WifiDialer(private val wifi: WifiNetwork) : LinkDialer {
  override fun dial(route: Route): LinkConn = when (route.way) {
    Way.CABLE -> SocketConn.connect(Socket(), route)
    Way.WIFI -> {
      val network = wifi.current ?: throw IOException("no Wi-Fi")
      SocketConn.connect(network.socketFactory.createSocket(), route)
    }
  }
}

/**
 * Follows the Wi-Fi network: its Network object, for the home network's
 * sockets, and word when it comes, so the link knocks at once. Needs only
 * ACCESS_NETWORK_STATE. The network's name is never read.
 */
internal class WifiNetwork(context: Context) {
  private val connectivity = context.getSystemService(ConnectivityManager::class.java)

  @Volatile var current: Network? = null
    private set

  private var started = false

  fun start(onCame: () -> Unit) {
    synchronized(this) {
      if (started) return
      started = true
    }
    val manager = connectivity ?: return
    val request = NetworkRequest.Builder()
      .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
      .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
      .build()
    try {
      manager.registerNetworkCallback(
        request,
        object : ConnectivityManager.NetworkCallback() {
          override fun onAvailable(network: Network) {
            current = network
            onCame()
          }

          override fun onLost(network: Network) {
            if (current == network) current = null
          }
        },
      )
    } catch (e: RuntimeException) {
      AndroidLinkLog.line("link failed what=wifi_watch error=${e.javaClass.simpleName}")
    }
  }
}

/**
 * Finds the family's PC by its announcement on the home network, with
 * Android's NsdManager: the service `_limo._tcp`, whose TXT records say
 * `v=1` and the master's id. Only an announcement with the id this phone
 * paired with, and an IPv4 address of a home network, is believed. Each
 * look runs for half a minute at most, and none while one runs; only on a
 * Wi-Fi network, and on the same one at most every five minutes, so a phone
 * away from home does not look again and again.
 */
internal class NsdFinder(context: Context, private val wifi: WifiNetwork) : MasterFinder {
  private val nsd = context.getSystemService(NsdManager::class.java)
  private val main = Handler(Looper.getMainLooper())
  private val serial = Executors.newSingleThreadExecutor()
  private val found = ConcurrentHashMap<String, List<String>>()

  @Volatile private var looking = false
  private var lastLook: Pair<Network, Long>? = null

  override fun found(masterId: String): List<String> = found[masterId.lowercase()].orEmpty()

  override fun look(masterId: String) {
    val nsd = nsd ?: return
    val network = wifi.current ?: return
    val now = SystemClock.elapsedRealtime()
    synchronized(this) {
      if (looking) return
      val last = lastLook
      if (last != null && last.first == network && now - last.second < AGAIN_MS) return
      looking = true
      lastLook = network to now
    }
    val wanted = masterId.lowercase()
    val listener = object : NsdManager.DiscoveryListener {
      override fun onDiscoveryStarted(serviceType: String) = Unit

      override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
        looking = false
      }

      override fun onDiscoveryStopped(serviceType: String) {
        looking = false
      }

      override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {
        looking = false
      }

      override fun onServiceFound(info: NsdServiceInfo) {
        resolve(nsd, info, wanted)
      }

      override fun onServiceLost(info: NsdServiceInfo) = Unit
    }
    try {
      nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, listener)
    } catch (e: RuntimeException) {
      looking = false
      AndroidLinkLog.line("link failed what=look error=${e.javaClass.simpleName}")
      return
    }
    main.postDelayed({
      try {
        nsd.stopServiceDiscovery(listener)
      } catch (_: RuntimeException) {
        // Stopped already.
      }
      looking = false
    }, LOOK_MS)
  }

  private fun resolve(nsd: NsdManager, info: NsdServiceInfo, wanted: String) {
    if (Build.VERSION.SDK_INT >= 34) {
      val callback = object : NsdManager.ServiceInfoCallback {
        override fun onServiceInfoCallbackRegistrationFailed(errorCode: Int) = Unit

        override fun onServiceUpdated(resolved: NsdServiceInfo) {
          take(resolved, wanted)
          try {
            nsd.unregisterServiceInfoCallback(this)
          } catch (_: RuntimeException) {
            // Gone already.
          }
        }

        override fun onServiceLost() = Unit

        override fun onServiceInfoCallbackUnregistered() = Unit
      }
      try {
        nsd.registerServiceInfoCallback(info, serial, callback)
      } catch (_: RuntimeException) {
        // A second resolve of the same service: the first one answers.
      }
    } else {
      // Before Android 14 one resolve runs at a time; the others fail, and
      // the next look tries again.
      try {
        @Suppress("DEPRECATION")
        nsd.resolveService(
          info,
          object : NsdManager.ResolveListener {
            override fun onResolveFailed(failed: NsdServiceInfo, errorCode: Int) = Unit

            override fun onServiceResolved(resolved: NsdServiceInfo) = take(resolved, wanted)
          },
        )
      } catch (_: RuntimeException) {
        // Tried again at the next look.
      }
    }
  }

  private fun take(info: NsdServiceInfo, wanted: String) {
    val txt = info.attributes.mapKeys { it.key.lowercase() }
    val id = txt["id"]?.let { String(it, Charsets.US_ASCII).lowercase() }
    val version = txt["v"]?.let { String(it, Charsets.US_ASCII) }
    if (id != wanted || version != "1") return
    val addresses: List<InetAddress> = if (Build.VERSION.SDK_INT >= 34) {
      info.hostAddresses
    } else {
      @Suppress("DEPRECATION")
      listOfNotNull(info.host)
    }
    val home = addresses.filterIsInstance<Inet4Address>().mapNotNull { it.hostAddress }.filter(Addresses::isHome)
    if (home.isEmpty()) return
    found[wanted] = (home + found[wanted].orEmpty()).distinct().take(8)
  }

  private companion object {
    const val SERVICE_TYPE = "_limo._tcp"
    const val LOOK_MS = 30_000L
    const val AGAIN_MS = 5L * 60 * 1_000
  }
}

/** The apps a child can open, from the watch, with their labels; the home screens and the diallers left out. */
internal class WatchApps(private val context: Context) : AppsSource {
  override fun apps(): List<AppEntry> = Watch.of(context).launchableApps()
    .filter { !it.neverCounted }
    .map { AppEntry(it.packageName, cut(it.label ?: it.packageName)) }
    .sortedBy { it.packageName }

  private fun cut(label: String): String =
    if (label.codePointCount(0, label.length) <= MAX_LABEL_CHARS) label else label.substring(0, label.offsetByCodePoints(0, MAX_LABEL_CHARS))

  private companion object {
    /** The longest label the link carries. */
    const val MAX_LABEL_CHARS = 120
  }
}

/**
 * What the phone says of itself when it pairs: its own name, as Android's
 * Settings show it ("Galaxy S20 Ultra"), else its model, cleaned and cut to
 * 40 characters, the most the family's file keeps; and Android's API level.
 */
internal fun phoneSelf(context: Context): PhoneSelf {
  val named = try {
    Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)
  } catch (_: RuntimeException) {
    null
  }
  return PhoneSelf(PhoneName.clean(named) ?: PhoneName.clean(Build.MODEL) ?: PhoneName.FALLBACK, Build.VERSION.SDK_INT)
}

/** The phone's name for the pairing, cleaned. Pure, so the JVM tests check it. */
object PhoneName {
  /** When the phone gives no name. */
  const val FALLBACK = "Android phone"

  /** The longest name sent: what the family's file keeps. */
  const val MAX_CHARS = 40

  /** Control characters and direction marks dropped, trimmed, cut at [MAX_CHARS] characters; null when nothing is left. */
  fun clean(raw: String?): String? {
    if (raw == null) return null
    val kept = StringBuilder()
    var i = 0
    while (i < raw.length) {
      val cp = raw.codePointAt(i)
      i += Character.charCount(cp)
      if (Character.getType(cp) == Character.CONTROL.toInt() || cp in 0x200e..0x200f || cp in 0x202a..0x202e || cp in 0x2066..0x2069) continue
      kept.appendCodePoint(cp)
    }
    val trimmed = kept.toString().trim()
    val cut = if (trimmed.codePointCount(0, trimmed.length) > MAX_CHARS) {
      trimmed.substring(0, trimmed.offsetByCodePoints(0, MAX_CHARS)).trim()
    } else {
      trimmed
    }
    return cut.ifEmpty { null }
  }
}

internal object AndroidLinkClock : LinkClock {
  override fun elapsedMs(): Long = SystemClock.elapsedRealtime()

  override fun wallSecs(): Long = System.currentTimeMillis().floorDiv(1_000L)
}

/** The link's log: Android's log, tag `LiMo`. */
internal object AndroidLinkLog : LinkLog {
  override fun line(text: String) {
    Log.i("LiMo", text)
  }
}
