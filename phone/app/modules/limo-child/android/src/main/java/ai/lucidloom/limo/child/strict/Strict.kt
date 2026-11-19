package ai.lucidloom.limo.child.strict

/**
 * Folder strict/ will hold strict mode (Android 9 and later): LiMo as the
 * phone's device owner, the user restrictions it sets, and its release. It
 * needs no component of its own: the device owner is keep/AdminReceiver,
 * and its timers use keep/'s receivers.
 *
 * Nothing yet.
 */
internal object Strict
