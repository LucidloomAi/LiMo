package ai.lucidloom.limo.child.link

/**
 * Folder link/ will hold the phone's half of the link to the master PC:
 * finding it on the home network (NsdManager), the socket through the Wi-Fi
 * network, and the frames and messages of the limo-link crate. It needs no
 * component of its own: it runs inside keep/KeepService.
 *
 * Nothing yet. When it is built it adds INTERNET and ACCESS_NETWORK_STATE
 * to the manifest, the only permissions that let any part of LiMo open a
 * socket.
 */
internal object Link
