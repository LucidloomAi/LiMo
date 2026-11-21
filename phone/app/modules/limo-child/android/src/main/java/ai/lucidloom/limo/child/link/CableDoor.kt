package ai.lucidloom.limo.child.link

import ai.lucidloom.limo.child.host.Host
import android.content.ContentProvider
import android.content.ContentValues
import android.content.pm.PackageManager
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.util.Log
import java.io.FileNotFoundException
import java.io.IOException
import java.io.InputStream

/**
 * The cable door: how the family's PC hands this phone a pairing offer over
 * the USB cable, in release builds too. A content provider at
 * `content://ai.lucidloom.limo.child.cable/offer`, exported and guarded by
 * `android.permission.DUMP`, which only the adb shell and the system hold:
 * no app on the phone can open it. The PC writes the offer's text with
 * `content write --uri content://ai.lucidloom.limo.child.cable/offer`, the
 * text on its standard input, so the offer is never in a command line.
 *
 * It takes one offer at most [LinkCore.MAX_OFFER_BYTES] long, refuses a
 * second while the first is still being paired with, and refuses any while
 * the phone is paired: a PC with the cable must not be able to pair a
 * child's phone again with another family. A refusal before reading makes
 * `content write` fail, so the PC knows. The offer goes straight to the
 * link, which wipes it; the log says only `cable offer taken` or `cable
 * offer refused why=<word>`.
 *
 * Nothing else: it answers no query, insert, update or delete, and reads no
 * file.
 */
class CableDoor : ContentProvider() {
  override fun onCreate(): Boolean = true

  override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
    val app = context?.applicationContext ?: throw FileNotFoundException("refused")
    val host = Host.get(app)
    val dump = app.checkCallingPermission(CableDoorRules.DUMP) == PackageManager.PERMISSION_GRANTED
    val refusal = CableDoorRules.refusal(uri.path, mode, dump)
      ?: if (host.paired) OfferAnswer.PAIRED.word else if (host.cableBusy) OfferAnswer.BUSY.word else null
    if (refusal != null) {
      Log.i(TAG, "cable offer refused why=$refusal")
      throw FileNotFoundException("refused")
    }
    // A reliable pipe, as the cable proof used on the test phone.
    val (reading, writing) = ParcelFileDescriptor.createReliablePipe()
    Thread({
      try {
        val text = ParcelFileDescriptor.AutoCloseInputStream(reading).use { CableDoorRules.readAtMost(it) }
        host.cableOffer(text)
      } catch (e: IOException) {
        Log.i(TAG, "cable offer refused why=unreadable")
      }
    }, "LiMo cable door").start()
    return writing
  }

  override fun query(uri: Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor? = null

  override fun getType(uri: Uri): String? = null

  override fun insert(uri: Uri, values: ContentValues?): Uri? = null

  override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

  override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int = 0

  private companion object {
    const val TAG = "LiMo"
  }
}

/** What the cable door checks, with no Android in it. */
object CableDoorRules {
  /** The one path the door has. */
  const val PATH = "/offer"

  /** The permission that guards the door: the adb shell and the system hold it, no app can. */
  const val DUMP = "android.permission.DUMP"

  /**
   * Why the door is shut to this call before anything is read, or null when
   * it may go on: only the path [PATH], only to write, and only for a caller
   * holding [DUMP] ([hasDump]). Android itself lets no one in who lacks it;
   * this checks again.
   */
  fun refusal(path: String?, mode: String, hasDump: Boolean): String? = when {
    !hasDump -> "caller"
    path != PATH -> "path"
    !mode.contains('w') || mode.contains('r') -> "mode"
    else -> null
  }

  /**
   * What the PC wrote, to its end: null when it is longer than an offer can
   * be ([LinkCore.MAX_OFFER_BYTES]). Read into one array, which the link
   * wipes.
   */
  fun readAtMost(input: InputStream, most: Int = LinkCore.MAX_OFFER_BYTES): ByteArray? {
    val buffer = ByteArray(most + 1)
    var length = 0
    while (length < buffer.size) {
      val n = input.read(buffer, length, buffer.size - length)
      if (n < 0) break
      length += n
    }
    if (length > most) {
      buffer.fill(0)
      // Drained, so the writer is not left blocked, and dropped.
      val drain = ByteArray(256)
      while (input.read(drain) >= 0) drain.fill(0)
      return null
    }
    val text = buffer.copyOf(length)
    buffer.fill(0)
    return text
  }
}
