//! Names and labels typed by a guardian or sent by the service.

use serde::{Deserialize, Serialize};

use crate::error::HubError;

/// Which piece of text a problem is about, so that a screen can point at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextField {
    /// A child's name: 1 to 40 characters.
    ChildName,
    /// A device's name: 1 to 40 characters.
    DeviceName,
    /// A device's platform label, such as "windows" or "android": 1 to 40
    /// characters.
    Platform,
    /// A Windows account's identifier: 1 to 200 characters.
    AccountId,
    /// A Windows account's display name: 1 to 100 characters.
    AccountName,
    /// The id a screen made for one tap: 1 to 100 characters.
    TapId,
    /// A program's name as LiMo records it, such as "minecraft.exe": 1 to
    /// 260 characters.
    ProgramName,
}

impl TextField {
    /// The most characters allowed.
    pub fn max_chars(self) -> u32 {
        match self {
            TextField::ChildName | TextField::DeviceName | TextField::Platform => 40,
            TextField::AccountId => 200,
            TextField::AccountName | TextField::TapId => 100,
            TextField::ProgramName => 260,
        }
    }

    /// What it is, in words, starting with a capital: "A child's name".
    pub fn label_capital(self) -> &'static str {
        match self {
            TextField::ChildName => "A child's name",
            TextField::DeviceName => "A device's name",
            TextField::Platform => "A device's platform",
            TextField::AccountId => "A Windows account's identifier",
            TextField::AccountName => "A Windows account's name",
            TextField::TapId => "The id of a tap",
            TextField::ProgramName => "A program's name",
        }
    }
}

/// True for characters that cannot be shown or that change how the text
/// around them is shown: control characters, and the marks that reverse or
/// isolate the direction of writing.
fn is_hidden(c: char) -> bool {
    c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}')
}

/// The text with spaces trimmed from both ends, or why it cannot be taken.
/// Letters of any script, digits, marks and emoji are all fine.
pub(crate) fn clean(text: &str, field: TextField) -> Result<String, HubError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(HubError::TextEmpty { field });
    }
    let max = field.max_chars();
    if trimmed.chars().count() as u64 > max as u64 {
        return Err(HubError::TextTooLong { field, max_chars: max });
    }
    if trimmed.chars().any(is_hidden) {
        return Err(HubError::TextHasHiddenCharacters { field });
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_of_any_script_are_taken_and_trimmed() {
        for name in ["Asha", "  Ravi  ", "आशा", "Zoë", "李明", "Robert'); DROP TABLE child;--", "🙂 Kid"] {
            let cleaned = clean(name, TextField::ChildName).unwrap();
            assert_eq!(cleaned, name.trim());
        }
    }

    #[test]
    fn empty_long_and_hidden_text_is_refused() {
        assert_eq!(clean("   ", TextField::ChildName), Err(HubError::TextEmpty { field: TextField::ChildName }));
        let long = "a".repeat(41);
        assert_eq!(
            clean(&long, TextField::ChildName),
            Err(HubError::TextTooLong { field: TextField::ChildName, max_chars: 40 })
        );
        // Forty characters of four bytes each are still forty characters.
        assert!(clean(&"🙂".repeat(40), TextField::ChildName).is_ok());
        for bad in ["As\u{0}ha", "As\nha", "Asha\u{202E}gnp.exe", "A\u{2066}b"] {
            assert_eq!(
                clean(bad, TextField::ChildName),
                Err(HubError::TextHasHiddenCharacters { field: TextField::ChildName }),
                "{bad:?}"
            );
        }
    }
}
