//! Interface languages. French is the default; English is available.
//!
//! Every user-facing text is written in both languages at its call site:
//! - [`tr`] picks between two static strings;
//! - [`tf!`] picks between two `format!` strings (both may capture variables).
//!
//! The current language is per thread: the UI runs on one thread, and tests can set
//! the language they assert on without affecting each other.

use serde::{Deserialize, Serialize};
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Fr,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Fr, Lang::En];

    /// The language's own name, as shown in the language menu.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
        }
    }
}

thread_local! {
    static LANG: Cell<Lang> = const { Cell::new(Lang::Fr) };
}

pub fn lang() -> Lang {
    LANG.with(Cell::get)
}

pub fn set_lang(lang: Lang) {
    LANG.with(|l| l.set(lang));
}

/// The French or English version of a fixed text.
pub fn tr(fr: &'static str, en: &'static str) -> &'static str {
    match lang() {
        Lang::Fr => fr,
        Lang::En => en,
    }
}

/// `tf!("Bonjour {name}", "Hello {name}")`: a `String` in the current language.
#[macro_export]
macro_rules! tf {
    ($fr:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::lang() {
            $crate::i18n::Lang::Fr => format!($fr $(, $arg)*),
            $crate::i18n::Lang::En => format!($en $(, $arg)*),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_per_thread() {
        let n = 3;
        set_lang(Lang::Fr);
        assert_eq!(tr("Fichier", "File"), "Fichier");
        assert_eq!(crate::tf!("{n} liens", "{n} links"), "3 liens");
        set_lang(Lang::En);
        assert_eq!(crate::tf!("{n} liens", "{n} links"), "3 links");
        std::thread::spawn(|| assert_eq!(lang(), Lang::Fr))
            .join()
            .unwrap();
    }
}
