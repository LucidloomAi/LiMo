package ai.lucidloom.limo.child.link

import ai.lucidloom.limo.child.ffi.testkit.TestFamily
import ai.lucidloom.limo.child.ffi.testkit.TestkitException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.IOException
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket

/**
 * The link through real sockets on this PC's loopback, as the cable's
 * tunnel brings the phone's connections to the PC's: a pairing and a
 * session through [SocketConn], the played PC behind a server socket of
 * the test's own; the PC closing the connection read as the PC out of
 * reach; a close that ends both ways at once; and an address that is not
 * the family's PC never connected to.
 */
class LinkSocketTest {
  @get:Rule val folder = TemporaryFolder()

  /** The played PC behind a socket: each connection's bytes to the testkit's master and back, until either side closes. */
  private class PlayedPc(private val family: TestFamily, private val clock: LinkClock) : AutoCloseable {
    val server = ServerSocket(0, 4, InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1)))
    @Volatile var connections = 0
    @Volatile var lastClient: Socket? = null
    @Volatile var sawTheEnd = false
    private val thread = Thread({ serve() }, "played PC").apply {
      isDaemon = true
      start()
    }

    private fun serve() {
      while (!server.isClosed) {
        val client = try {
          server.accept()
        } catch (_: IOException) {
          return
        }
        connections += 1
        lastClient = client
        family.connect(clock.elapsedMs(), clock.wallSecs())
        val buffer = ByteArray(64 * 1024)
        try {
          val input = client.getInputStream()
          val output = client.getOutputStream()
          while (true) {
            val n = input.read(buffer)
            if (n < 0) {
              sawTheEnd = true
              break
            }
            val back = try {
              family.exchange(buffer.copyOf(n), clock.elapsedMs(), clock.wallSecs())
            } catch (_: TestkitException) {
              break
            }
            if (back.isNotEmpty()) {
              output.write(back)
              output.flush()
            }
          }
        } catch (_: IOException) {
          sawTheEnd = true
        } finally {
          client.close()
        }
      }
    }

    override fun close() {
      server.close()
      thread.join(2_000)
    }
  }

  @Test
  fun aPairingAndASessionGoThroughRealSockets() {
    LinkRig(folder.newFolder()).use { r ->
      PlayedPc(r.family, r.clock).use { pc ->
        // The cable's route, to the port of the test's own server.
        val dialer = LinkDialer { route ->
          assertEquals(Way.CABLE, route.way)
          SocketConn.connect(Socket(), Route(Way.CABLE, Addresses.LOOPBACK, pc.server.localPort))
        }
        val core = LinkCore(
          RigSide(r.rig), r.pairing, dialer, r.clock, NoFinder, { r.apps }, { PhoneSelf("Galaxy S20 Ultra", 33) }, { r.about }, { r.log += it },
        )
        r.rig.link = core
        r.rig.core = r.rig.newCore()
        fun step(times: Int) = repeat(times) {
          r.rig.pass(1)
          core.step()
          core.step()
        }
        assertEquals(OfferAnswer.TAKEN, core.takeOffer(r.family.makeOffer(r.rig.child, r.rig.wall)))
        step(3)
        assertTrue(r.log.toString(), r.log.contains("link paired"))
        r.rig.core.finishSetup()
        step(5)
        assertTrue(r.log.toString(), r.log.contains("link open via=cable"))
        assertTrue(core.inReach)
        // A minute in the game: reports and grants through the socket.
        step(65)
        assertTrue(r.family.usedToday(r.rig.child, r.rig.wall) >= 60)
        assertFalse(r.rig.keeper.last.away)
        // The PC closes the connection: the phone takes it that the PC is out of reach.
        pc.lastClient!!.close()
        step(2)
        assertTrue(r.log.contains("link closed"))
        assertFalse(core.inReach)
        assertEquals(2, pc.connections)
      }
    }
  }

  @Test
  fun aCloseEndsBothWaysAtOnceAndOnlyTheFamilysPcIsEverConnectedTo() {
    ServerSocket(0, 1, InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1))).use { server ->
      val conn = SocketConn.connect(Socket(), Route(Way.CABLE, Addresses.LOOPBACK, server.localPort))
      server.accept().use { pc ->
        conn.write(byteArrayOf(1, 2, 3))
        assertEquals(3, pc.getInputStream().read(ByteArray(8)))
        assertEquals(0, conn.read(50)!!.size)
        pc.getOutputStream().write(byteArrayOf(9))
        assertEquals(listOf<Byte>(9), conn.read(1_000)!!.toList())
        conn.close()
        // The PC's side sees the end of the stream at once: nothing was left
        // half open.
        pc.soTimeout = 2_000
        assertEquals(-1, pc.getInputStream().read())
      }
    }
    // Only the phone's own loopback for the cable, only a home network's
    // address for the Wi-Fi, only a real port; no name is looked up.
    for (route in listOf(
      Route(Way.CABLE, "192.168.1.20", 47810),
      Route(Way.WIFI, Addresses.LOOPBACK, 47810),
      Route(Way.WIFI, "8.8.8.8", 47810),
      Route(Way.WIFI, "lucidloom.ai", 47810),
      Route(Way.CABLE, Addresses.LOOPBACK, 0),
    )) {
      try {
        SocketConn.connect(Socket(), route).close()
        fail("connected to $route")
      } catch (e: IOException) {
        // Never connected.
      }
    }
  }
}
