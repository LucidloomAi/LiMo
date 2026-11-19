package ai.lucidloom.limo.child.keep

import android.app.admin.DeviceAdminReceiver

/**
 * The device admin. A stub: it adds nothing to what DeviceAdminReceiver
 * does by itself.
 *
 * Later, while it is active, LiMo cannot be uninstalled until a guardian
 * switches it off (the base protection), and in strict mode the same
 * component is the phone's device owner (folder strict/). Android remembers
 * both by this class name, so it must not be renamed or moved.
 */
class AdminReceiver : DeviceAdminReceiver()
