//! The shape of the manual.
//!
//! A structured document rather than rendered HTML: the interface draws it with
//! its own widgets, so it inherits the palette, stays navigable, and can switch
//! language without reloading anything.

/// Which language the reader has chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    French,
}

impl Language {
    /// The other one, for the toggle button.
    pub fn other(self) -> Self {
        match self {
            Language::English => Language::French,
            Language::French => Language::English,
        }
    }

    /// The name of the language, written in that language.
    pub fn label(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::French => "Français",
        }
    }

    /// A short code for the toggle.
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "EN",
            Language::French => "FR",
        }
    }
}

/// A string in both languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text {
    pub en: &'static str,
    pub fr: &'static str,
}

impl Text {
    pub const fn new(en: &'static str, fr: &'static str) -> Self {
        Self { en, fr }
    }

    pub fn get(&self, language: Language) -> &'static str {
        match language {
            Language::English => self.en,
            Language::French => self.fr,
        }
    }
}

/// What a callout is telling the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutKind {
    /// Something that makes the work easier.
    Tip,
    /// Something that will bite if ignored.
    Warning,
    /// Background worth knowing.
    Note,
}

/// One row of a parameter table.
///
/// The name and the type are not translated: they are what the interface shows
/// and what the configuration file contains, so translating them would send the
/// reader looking for a control that does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterRow {
    pub name: &'static str,
    pub kind: &'static str,
    pub description: Text,
}

impl ParameterRow {
    pub const fn new(name: &'static str, kind: &'static str, description: Text) -> Self {
        Self {
            name,
            kind,
            description,
        }
    }
}

/// One crate the project depends on, and why.
///
/// The name is not translated: it is what appears in `Cargo.toml` and on
/// crates.io, and a reader looking it up needs the real spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    pub name: &'static str,
    /// What the project uses it for.
    pub role: Text,
}

impl Citation {
    pub const fn new(name: &'static str, role: Text) -> Self {
        Self { name, role }
    }
}

/// A piece of a section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading(Text),
    Paragraph(Text),
    List(Vec<Text>),
    Callout {
        kind: CalloutKind,
        text: Text,
    },
    /// A command to run. Never translated.
    Code {
        caption: Text,
        lines: Vec<&'static str>,
    },
    Table {
        headers: [Text; 3],
        rows: Vec<ParameterRow>,
    },
    /// A figure shipped with the application.
    Image {
        asset: &'static str,
        caption: Text,
    },
    /// Crates the project stands on.
    ///
    /// Kept apart from [`Block::Table`] rather than reusing it: the table is
    /// what the completeness check reads to know which settings are
    /// documented, and a crate name is not a setting.
    Citations(Vec<Citation>),
}

/// A page of the manual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub id: &'static str,
    pub title: Text,
    pub blocks: Vec<Block>,
}

/// A group of pages, shown as one entry in the navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    pub id: &'static str,
    pub title: Text,
    pub sections: Vec<Section>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_answers_in_the_requested_language() {
        let text = Text::new("Working directory", "Répertoire de travail");
        assert_eq!(text.get(Language::English), "Working directory");
        assert_eq!(text.get(Language::French), "Répertoire de travail");
    }

    #[test]
    fn the_toggle_alternates() {
        let mut language = Language::default();
        assert_eq!(language, Language::English);
        language = language.other();
        assert_eq!(language, Language::French);
        assert_eq!(language.other(), Language::English);
    }

    #[test]
    fn each_language_has_a_label_and_a_code() {
        for language in [Language::English, Language::French] {
            assert!(!language.label().is_empty());
            assert_eq!(language.code().len(), 2);
        }
    }
}
