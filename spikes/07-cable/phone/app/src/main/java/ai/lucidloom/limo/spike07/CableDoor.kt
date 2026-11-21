package ai.lucidloom.limo.spike07

import android.content.ContentProvider
import android.content.ContentValues
import android.content.pm.PackageManager
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Binder
import android.os.ParcelFileDescriptor
import android.util.Log
import java.io.FileNotFoundException
import java.io.IOException

/**
 * The guarded door: a content provider that Android lets only holders of
 * `android.permission.DUMP` reach (the adb shell and the system), as the
 * manifest says, checked here a second time.
 *
 * - `content://ai.lucidloom.limo.spike07.cable/offer`, opened to write
 *   (`content write --uri ...` with the bytes on its standard input): the
 *   bytes go through a pipe into memory, never to a file, are judged
 *   ([Door.judge]), their length and check kept, and are wiped.
 * - `content://ai.lucidloom.limo.spike07.cable/status`, queried: what the
 *   door and the loopback have seen ([Door.COLUMNS]), never a secret.
 *
 * Its log says only `cable secret taken bytes=N check=C uid=U` or
 * `cable secret refused why=W uid=U`.
 */
class CableDoor : ContentProvider() {
    override fun onCreate(): Boolean = true

    private fun allowed(): Boolean =
        context?.checkCallingPermission(DUMP) == PackageManager.PERMISSION_GRANTED

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        val uid = Binder.getCallingUid()
        if (!allowed()) {
            SpikeState.recordWrite(Door.Taken.Refused("not_allowed", 0), uid)
            Log.i(TAG, "cable secret refused why=not_allowed uid=$uid")
            throw SecurityException("the door is for the adb shell only")
        }
        if (uri.path != "/offer") throw FileNotFoundException("no such door")
        if (!mode.contains('w') || mode.contains('r')) throw FileNotFoundException("the door is written, never read")
        val pipe = ParcelFileDescriptor.createReliablePipe()
        val reading = pipe[0]
        Thread({ take(reading, uid) }, "limo-spike07-door").start()
        return pipe[1]
    }

    /** Reads what the writer sends, judges it, keeps its length and check, wipes it. */
    private fun take(reading: ParcelFileDescriptor, uid: Int) {
        val buffer = ByteArray(Door.MAX_BYTES + 1)
        var length = 0
        var overflow = false
        var cut = false
        try {
            ParcelFileDescriptor.AutoCloseInputStream(reading).use { input ->
                val drain = ByteArray(4096)
                while (true) {
                    if (length < buffer.size) {
                        val n = input.read(buffer, length, buffer.size - length)
                        if (n < 0) break
                        length += n
                    } else {
                        overflow = true
                        if (input.read(drain) < 0) break
                    }
                }
            }
        } catch (e: IOException) {
            cut = true
        }
        val taken = when {
            cut -> Door.Taken.Refused("cut_short", length)
            overflow || length > Door.MAX_BYTES -> Door.Taken.Refused("too_long", length)
            else -> Door.judge(buffer, length, false)
        }
        buffer.fill(0)
        SpikeState.recordWrite(taken, uid)
        when (taken) {
            is Door.Taken.Ok -> Log.i(TAG, "cable secret taken bytes=${taken.bytes} check=${taken.check} uid=$uid")
            is Door.Taken.Refused -> Log.i(TAG, "cable secret refused why=${taken.why} uid=$uid")
        }
    }

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? {
        if (!allowed()) throw SecurityException("the door is for the adb shell only")
        if (uri.path != "/status") return null
        return MatrixCursor(Door.COLUMNS).apply { addRow(Door.row(SpikeState.snapshot())) }
    }

    override fun getType(uri: Uri): String? = null

    override fun insert(uri: Uri, values: ContentValues?): Uri? = throw UnsupportedOperationException("the door takes writes through openFile only")

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int = 0

    companion object {
        const val TAG = "LiMoSpike07"
        const val DUMP = "android.permission.DUMP"
    }
}
