//! The console's shortcut: a `.lnk` file made through the shell's own
//! interface, and the Start menu folder of every account.

use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IPersistFile};
#[cfg(test)]
use windows::Win32::System::Com::STGM_READ;
use windows::Win32::UI::Shell::{FOLDERID_CommonPrograms, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellLink};
use windows::core::{Interface, PCWSTR};

use super::com::Com;
use super::{error_text, wide};

/// The Start menu's Programs folder of every account
/// (`%ProgramData%\Microsoft\Windows\Start Menu\Programs`).
pub(crate) fn every_accounts_programs() -> Option<PathBuf> {
    // SAFETY: the path is made by Windows and freed with CoTaskMemFree once
    // read.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_CommonPrograms, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const core::ffi::c_void));
        text.map(PathBuf::from)
    }
}

/// Makes a shortcut at `at` that opens `target`, in its own folder.
pub(crate) fn make(at: &Path, target: &Path, description: &str) -> Result<(), String> {
    let _com = Com::start();
    let target_text = wide(&target.to_string_lossy());
    let folder = wide(&target.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
    let description = wide(description);
    let at_text = wide(&at.to_string_lossy());
    let text = |e: windows::core::Error| error_text(&e);
    // SAFETY: a shell link object made in this process, filled from strings
    // that live until the end of the call, and saved to `at`.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(text)?;
        link.SetPath(PCWSTR(target_text.as_ptr())).map_err(text)?;
        link.SetWorkingDirectory(PCWSTR(folder.as_ptr())).map_err(text)?;
        link.SetDescription(PCWSTR(description.as_ptr())).map_err(text)?;
        let file: IPersistFile = link.cast().map_err(text)?;
        file.Save(PCWSTR(at_text.as_ptr()), true).map_err(text)
    }
}

/// The program a shortcut opens: for the test of [`make`].
#[cfg(test)]
pub(crate) fn target_of(at: &Path) -> Option<PathBuf> {
    let _com = Com::start();
    let at_text = wide(&at.to_string_lossy());
    let mut buffer = vec![0u16; 1_024];
    // SAFETY: loads the shortcut into a link object of this process and
    // reads its path into a buffer of the size given.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        file.Load(PCWSTR(at_text.as_ptr()), STGM_READ).ok()?;
        link.GetPath(&mut buffer, std::ptr::null_mut(), 0).ok()?;
    }
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..end])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shortcut_made_in_the_tests_own_folder_opens_its_program() {
        // Only a temporary folder of the test's own: never the Start menu.
        let folder = tempfile::tempdir().unwrap();
        let program = folder.path().join("limo-console.exe");
        std::fs::write(&program, b"not a program").unwrap();
        let at = folder.path().join("LiMo settings.lnk");
        make(&at, &program, "LiMo's settings, for a guardian.").unwrap();
        assert!(at.is_file());
        let opened = target_of(&at).unwrap();
        assert_eq!(
            std::fs::canonicalize(opened).unwrap(),
            std::fs::canonicalize(&program).unwrap(),
            "the shortcut opens the program it was made for"
        );
    }

    #[test]
    fn the_start_menu_of_every_account_is_under_programdata() {
        // A read of where it is; nothing is made there.
        let programs = every_accounts_programs().unwrap();
        let text = programs.to_string_lossy().to_lowercase();
        assert!(text.ends_with(r"microsoft\windows\start menu\programs"), "{text}");
    }
}
