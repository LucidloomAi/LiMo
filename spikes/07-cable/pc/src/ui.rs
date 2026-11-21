//! The terminal: lines said to the person, and the one question asked
//! (before USB debugging is switched off). The tests keep what is said in
//! memory instead.

#![forbid(unsafe_code)]

use std::io::{BufRead, Write};

/// Where lines go, and where an answer comes from.
pub trait Ui {
    /// Says one line.
    fn say(&mut self, line: &str);
    /// Asks a question and returns the line typed, trimmed; None when
    /// nothing can be typed (no terminal, or the input ended).
    fn ask(&mut self, question: &str) -> Option<String>;
}

/// The real terminal.
#[derive(Debug, Default)]
pub struct Terminal;

impl Ui for Terminal {
    fn say(&mut self, line: &str) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }

    fn ask(&mut self, question: &str) -> Option<String> {
        {
            let mut out = std::io::stdout().lock();
            let _ = write!(out, "{question} ");
            let _ = out.flush();
        }
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim().to_string()),
        }
    }
}

/// Lines kept in memory, and answers given in advance: for the tests.
#[derive(Debug, Default)]
pub struct Recorder {
    /// Everything said, one entry a line.
    pub lines: Vec<String>,
    /// The answers to give, in order.
    pub answers: Vec<String>,
}

impl Recorder {
    /// Everything said, as one text.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

impl Ui for Recorder {
    fn say(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    fn ask(&mut self, question: &str) -> Option<String> {
        self.lines.push(question.to_string());
        if self.answers.is_empty() { None } else { Some(self.answers.remove(0)) }
    }
}
