//! What LiMo will never close, whatever a family's rules say: the parts of
//! Windows a signed in session cannot run without.
//!
//! A program on this list is spared only when its file is inside the Windows
//! folder (such as `C:\Windows`), where a standard account cannot write. A
//! game renamed to `explorer.exe` in a child's Downloads folder is not
//! spared: its name is on the list, but its file is not Windows'.
//!
//! Beside this list the service never closes a process in session 0 or in
//! another session than the order's, a process of LiMo's own (by its id, and
//! by its file being LiMo's own), or a process whose file it could not read.

#![forbid(unsafe_code)]

use crate::programs::file_name_of;

/// One part of Windows LiMo never closes, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeverClose {
    /// Its file name, in lower case.
    pub file: &'static str,
    /// Why it must not be closed, in plain words.
    pub why: &'static str,
}

/// The list. Each entry runs, or can run, inside a signed in session, and
/// closing it would break the session for everyone in it, or stop Windows.
pub const NEVER_CLOSE: &[NeverClose] = &[
    NeverClose { file: "csrss.exe", why: "It runs the session itself; closing it stops Windows with a blue screen." },
    NeverClose { file: "winlogon.exe", why: "It signs the session in and out and shows the secure sign in screen; closing it ends the session or stops Windows." },
    NeverClose { file: "wininit.exe", why: "It starts and watches the core of Windows; closing it stops Windows." },
    NeverClose { file: "smss.exe", why: "It starts every session; closing it stops Windows." },
    NeverClose { file: "lsass.exe", why: "It checks passwords and keeps everyone signed in; closing it restarts Windows." },
    NeverClose { file: "services.exe", why: "It runs every Windows service, LiMo’s own among them." },
    NeverClose { file: "svchost.exe", why: "It runs Windows services, some of them inside a signed in session." },
    NeverClose { file: "dwm.exe", why: "It draws every window; closing it blanks the screen." },
    NeverClose { file: "fontdrvhost.exe", why: "It draws the fonts every program uses." },
    NeverClose { file: "explorer.exe", why: "It is the desktop, the taskbar and the Start menu; without it the session cannot be used." },
    NeverClose { file: "sihost.exe", why: "It runs the shell’s own parts: notifications, keyboard layouts, the action centre." },
    NeverClose { file: "startmenuexperiencehost.exe", why: "It is the Start menu." },
    NeverClose { file: "shellexperiencehost.exe", why: "It is the clock, the notification area and other parts of the taskbar." },
    NeverClose { file: "ctfmon.exe", why: "It handles typing: keyboard layouts and input in other languages." },
    NeverClose { file: "textinputhost.exe", why: "It is the touch keyboard and the emoji and input language panels." },
    NeverClose { file: "conhost.exe", why: "It draws every console window; LiMo closes the program in the console instead." },
    NeverClose { file: "taskhostw.exe", why: "It runs Windows' own tasks for the signed in user." },
    NeverClose { file: "runtimebroker.exe", why: "It checks what Store programs may do; closing it breaks them all." },
    NeverClose { file: "dllhost.exe", why: "It runs parts of Windows on behalf of other programs, such as pictures in folders." },
    NeverClose { file: "userinit.exe", why: "It sets up the session when someone signs in." },
    NeverClose { file: "logonui.exe", why: "It is the Windows sign in and lock screen." },
    NeverClose { file: "consent.exe", why: "It asks for an administrator’s permission; closing it would stop a guardian from giving it." },
];

/// Paths compared as Windows compares them: case ignored, either slash,
/// no `\\?\` in front, no slash at the end.
pub fn normalise_path(path: &str) -> String {
    let mut s = path.replace('/', "\\").to_lowercase();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        s = rest.to_string();
    }
    while s.ends_with('\\') {
        s.pop();
    }
    s
}

/// True when two paths name the same file.
pub fn same_path(a: &str, b: &str) -> bool {
    !a.is_empty() && normalise_path(a) == normalise_path(b)
}

/// True when `child` is inside the folder `parent`, at any depth.
pub fn path_is_inside(child: &str, parent: &str) -> bool {
    let parent = normalise_path(parent);
    if parent.is_empty() {
        return false;
    }
    normalise_path(child).starts_with(&format!("{parent}\\"))
}

/// The entry that spares a process, when its file is a part of Windows on
/// the list: on the list by name, and inside the Windows folder.
pub fn never_close(path: &str, windows_dir: &str) -> Option<&'static NeverClose> {
    if !path_is_inside(path, windows_dir) {
        return None;
    }
    let name = file_name_of(path).to_lowercase();
    NEVER_CLOSE.iter().find(|entry| entry.file == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: &str = r"C:\Windows";

    #[test]
    fn the_parts_of_windows_are_spared_only_where_windows_keeps_them() {
        for entry in NEVER_CLOSE {
            assert_eq!(entry.file, entry.file.to_lowercase());
            assert!(entry.why.ends_with('.'), "{}", entry.why);
        }
        let spared = never_close(r"C:\Windows\explorer.exe", WINDOWS).unwrap();
        assert_eq!(spared.file, "explorer.exe");
        assert!(never_close(r"c:\windows\system32\DWM.EXE", WINDOWS).is_some());
        assert!(never_close(r"\\?\C:\WINDOWS\System32\svchost.exe", "c:/windows/").is_some());
        assert!(never_close(r"C:\Windows\SystemApps\ShellExperienceHost_cw5n1h2txyewy\ShellExperienceHost.exe", WINDOWS).is_some());
        // Renamed into a name on the list, outside the Windows folder: not
        // spared.
        assert_eq!(never_close(r"C:\Users\Asha\Downloads\explorer.exe", WINDOWS), None);
        assert_eq!(never_close(r"C:\WindowsGames\explorer.exe", WINDOWS), None);
        // A program of Windows that is not on the list can be closed, if a
        // family blocks it.
        assert_eq!(never_close(r"C:\Windows\System32\notepad.exe", WINDOWS), None);
        assert_eq!(never_close(r"C:\Windows\System32\cmd.exe", WINDOWS), None);
        // An unknown path or Windows folder spares nothing.
        assert_eq!(never_close("", WINDOWS), None);
        assert_eq!(never_close(r"C:\Windows\explorer.exe", ""), None);
    }

    #[test]
    fn paths_compare_as_windows_compares_them() {
        assert!(same_path(r"C:\Program Files\LiMo\limo.exe", r"c:/program files/limo/LIMO.EXE"));
        assert!(same_path(r"\\?\C:\Program Files\LiMo\limo.exe", r"C:\Program Files\LiMo\limo.exe"));
        assert!(!same_path("", ""));
        assert!(!same_path(r"C:\a\limo.exe", r"C:\b\limo.exe"));
        assert!(path_is_inside(r"C:\Windows\System32\x.exe", r"C:\Windows\"));
        assert!(!path_is_inside(r"C:\Windows", r"C:\Windows"));
        assert!(!path_is_inside(r"C:\WindowsOld\x.exe", r"C:\Windows"));
    }
}
