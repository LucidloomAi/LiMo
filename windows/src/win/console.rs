//! Typing a password or a PIN at the prompt without it showing.

use std::io::{BufRead, Write};

use windows::Win32::System::Console::{CONSOLE_MODE, ENABLE_ECHO_INPUT, GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE, SetConsoleMode};
use zeroize::Zeroizing;

/// Asks for something secret: what is typed does not show. The answer is
/// wiped from memory when dropped. None when nothing could be read.
pub(crate) fn read_hidden(prompt: &str) -> Option<Zeroizing<String>> {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut mode = CONSOLE_MODE::default();
    // SAFETY: reads and changes the console mode of this process's own
    // input, and puts it back below.
    let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) }.ok();
    let hidden = input.is_some_and(|input| unsafe {
        GetConsoleMode(input, &mut mode).is_ok() && SetConsoleMode(input, mode & !ENABLE_ECHO_INPUT).is_ok()
    });
    // Room made at once, so that the text never grows and leaves a copy.
    let mut typed = Zeroizing::new(String::with_capacity(4_096));
    let read = std::io::stdin().lock().read_line(&mut typed);
    if hidden && let Some(input) = input {
        // SAFETY: puts back the mode read above.
        unsafe {
            let _ = SetConsoleMode(input, mode);
        }
    }
    println!();
    match read {
        Ok(0) | Err(_) => None,
        Ok(_) => {
            let trimmed = Zeroizing::new(typed.trim_end_matches(['\r', '\n']).to_string());
            Some(trimmed)
        }
    }
}
