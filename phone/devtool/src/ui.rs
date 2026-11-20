//! The terminal: lines printed, and the PIN typed at a prompt that does
//! not show it. Behind the [`Ui`] trait, so that the tests can read every
//! line the tool prints and type a PIN for it.

use std::io::IsTerminal;

use zeroize::Zeroizing;

/// What a hidden prompt got.
pub enum Hidden {
    /// What was typed, wiped from memory when dropped.
    Typed(Zeroizing<String>),
    /// The tool does not run in a terminal (its input comes from a file or
    /// a pipe), so nothing can be typed without it showing.
    NotATerminal,
    /// Nothing could be read.
    Nothing,
}

/// The terminal.
pub trait Ui {
    /// Prints one line.
    fn say(&mut self, line: &str);
    /// Asks for something secret, at a prompt where what is typed does not
    /// show.
    fn hidden(&mut self, prompt: &str) -> Hidden;
}

/// The real terminal.
#[derive(Debug, Default)]
pub struct Terminal;

impl Ui for Terminal {
    fn say(&mut self, line: &str) {
        println!("{line}");
    }

    fn hidden(&mut self, prompt: &str) -> Hidden {
        // The console crate reads the keys itself, so what is typed is never
        // echoed. It needs a real terminal for that.
        let term = console::Term::stdout();
        if !term.is_term() || !std::io::stdin().is_terminal() {
            return Hidden::NotATerminal;
        }
        if term.write_str(prompt).is_err() {
            return Hidden::Nothing;
        }
        let _ = term.flush();
        match term.read_secure_line() {
            Ok(typed) => Hidden::Typed(Zeroizing::new(typed)),
            Err(_) => Hidden::Nothing,
        }
    }
}
