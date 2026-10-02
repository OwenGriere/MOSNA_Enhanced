//! The installer's console: cargo's own colours, and a colour per kind of line
//! for everything else.
//!
//! Cargo is asked for colour (`CARGO_TERM_COLOR=always`), which it writes as
//! ANSI escape codes; they are read here into coloured runs of text. The other
//! programs — rustup, winget, the Visual Studio installer, `mosna-install` —
//! print plain text, so their lines are coloured by what they say: an error,
//! a warning, information, a success, or the start of a stage.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

use crate::theme;

/// What a line of plain text reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Step,
    Error,
    Warning,
    Info,
    Success,
    Plain,
}

impl Kind {
    pub fn color(self) -> Color32 {
        match self {
            Kind::Step => theme::LOG_STEP,
            Kind::Error => theme::LOG_ERROR,
            Kind::Warning => theme::LOG_WARNING,
            Kind::Info => theme::LOG_INFO,
            Kind::Success => theme::LOG_SUCCESS,
            Kind::Plain => theme::LOG_PLAIN,
        }
    }
}

/// What a line reports, from its words. In French and in English: the
/// installer's own messages are French, the tools it runs mostly English.
pub fn classify(line: &str) -> Kind {
    let text = line.trim_start().to_lowercase();
    let starts = |prefixes: &[&str]| prefixes.iter().any(|prefix| text.starts_with(prefix));
    let mentions = |words: &[&str]| words.iter().any(|word| text.contains(word));

    if text.starts_with("==>") {
        Kind::Step
    } else if starts(&["error", "erreur", "échec", "fatal"])
        || mentions(&["error:", "error[", "échec", "a échoué", "failed"])
    {
        Kind::Error
    } else if starts(&["warning", "warn", "avertissement", "attention"]) || mentions(&["warning:"])
    {
        Kind::Warning
    } else if starts(&[
        "finished",
        "successfully",
        "supprimé",
        "removed",
        "installed",
    ]) || mentions(&["déjà installé", "est installé", "installed successfully"])
    {
        Kind::Success
    } else if starts(&[
        "info",
        "note",
        "help",
        "= note",
        "= help",
        "téléchargement",
        "windows va",
    ]) {
        Kind::Info
    } else {
        Kind::Plain
    }
}

/// A run of text in one colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    /// `None` where the escape codes left the default colour.
    pub color: Option<Color32>,
}

/// Split a line on its ANSI escape codes. Codes other than colours are
/// dropped, and so is anything before a carriage return, which a terminal
/// would have overwritten.
pub fn runs(line: &str) -> Vec<Run> {
    let line = line.rsplit('\r').next().unwrap_or(line);
    let mut runs: Vec<Run> = Vec::new();
    let mut state = Sgr::default();
    let mut text = String::new();
    let mut chars = line.chars().peekable();

    let flush = |text: &mut String, state: &Sgr, runs: &mut Vec<Run>| {
        if !text.is_empty() {
            runs.push(Run {
                text: std::mem::take(text),
                color: state.color(),
            });
        }
    };

    while let Some(c) = chars.next() {
        if c != '\x1b' {
            text.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte.
            Some('[') => {
                let mut parameters = String::new();
                let mut last = None;
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        last = Some(c);
                        break;
                    }
                    parameters.push(c);
                }
                if last == Some('m') {
                    flush(&mut text, &state, &mut runs);
                    state.apply(&parameters);
                }
            }
            // OSC, a hyperlink for instance: up to BEL or ESC \.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    flush(&mut text, &state, &mut runs);
    runs
}

/// The line without its escape codes, for the log file.
pub fn plain(line: &str) -> String {
    runs(line).into_iter().map(|run| run.text).collect()
}

/// The line as egui draws it. With `colored` false, in one plain colour.
pub fn layout(line: &str, colored: bool) -> LayoutJob {
    let font = FontId::monospace(theme::MONO_SIZE);
    let mut job = LayoutJob::default();
    let runs = runs(line);
    let default = if !colored {
        theme::LOG_PLAIN
    } else if runs.iter().any(|run| run.color.is_some()) {
        // Cargo has coloured what matters; the rest is its plain text.
        theme::LOG_PLAIN
    } else {
        classify(&plain(line)).color()
    };
    for run in runs {
        let color = if colored {
            run.color.unwrap_or(default)
        } else {
            default
        };
        job.append(&run.text, 0.0, TextFormat::simple(font.clone(), color));
    }
    job
}

// ---------------------------------------------------------------------------
// Select Graphic Rendition: the colour state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
struct Sgr {
    bold: bool,
    color: Option<Foreground>,
}

#[derive(Debug, Clone, Copy)]
enum Foreground {
    /// One of the eight colours, and whether in its bright form.
    Basic(u8, bool),
    Rgb(Color32),
}

impl Sgr {
    fn apply(&mut self, parameters: &str) {
        let codes: Vec<u16> = parameters
            .split([';', ':'])
            .map(|code| code.parse().unwrap_or(0))
            .collect();
        let mut codes = codes.into_iter();
        while let Some(code) = codes.next() {
            match code {
                0 => *self = Sgr::default(),
                1 => self.bold = true,
                22 => self.bold = false,
                30..=37 => self.color = Some(Foreground::Basic((code - 30) as u8, false)),
                90..=97 => self.color = Some(Foreground::Basic((code - 90) as u8, true)),
                39 => self.color = None,
                38 | 48 => {
                    let color = match codes.next() {
                        Some(5) => codes.next().map(indexed),
                        Some(2) => match (codes.next(), codes.next(), codes.next()) {
                            (Some(r), Some(g), Some(b)) => Some(Foreground::Rgb(
                                Color32::from_rgb(r as u8, g as u8, b as u8),
                            )),
                            _ => None,
                        },
                        _ => None,
                    };
                    // A background colour is read past, not drawn.
                    if code == 38 {
                        self.color = color;
                    }
                }
                _ => {}
            }
        }
    }

    fn color(&self) -> Option<Color32> {
        match self.color? {
            // Bold brightens, as terminals do: cargo's "Compiling" and
            // "warning" are bold.
            Foreground::Basic(index, bright) => Some(basic(index, bright || self.bold)),
            Foreground::Rgb(color) => Some(color),
        }
    }
}

/// A colour of the 256-colour palette.
fn indexed(index: u16) -> Foreground {
    match index {
        0..=7 => Foreground::Basic(index as u8, false),
        8..=15 => Foreground::Basic(index as u8 - 8, true),
        16..=231 => {
            let index = index - 16;
            let level = |value: u16| {
                if value == 0 {
                    0
                } else {
                    (55 + value * 40) as u8
                }
            };
            Foreground::Rgb(Color32::from_rgb(
                level(index / 36),
                level(index / 6 % 6),
                level(index % 6),
            ))
        }
        _ => {
            let grey = (8 + (index.min(255) - 232) * 10) as u8;
            Foreground::Rgb(Color32::from_rgb(grey, grey, grey))
        }
    }
}

/// The eight colours, tuned for the dark console.
fn basic(index: u8, bright: bool) -> Color32 {
    let (normal, light) = match index {
        0 => ((0x6E, 0x68, 0x5C), (0x8E, 0x88, 0x7C)),
        1 => ((0xE0, 0x5A, 0x4C), (0xF0, 0x6A, 0x5C)),
        2 => ((0x6F, 0xB8, 0x5E), (0x8B, 0xD0, 0x7A)),
        3 => ((0xDB, 0xAE, 0x3E), (0xF2, 0xC2, 0x4B)),
        4 => ((0x5F, 0x94, 0xCF), (0x7F, 0xB2, 0xE5)),
        5 => ((0xB0, 0x78, 0xC8), (0xC9, 0x94, 0xDE)),
        6 => ((0x4F, 0xAE, 0xB0), (0x6C, 0xC8, 0xC9)),
        _ => ((0xC9, 0xC1, 0xAF), (0xF4, 0xEE, 0xE2)),
    };
    let (r, g, b) = if bright { light } else { normal };
    Color32::from_rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_s_compiling_line_keeps_its_green() {
        let line = "\x1b[1m\x1b[92m   Compiling\x1b[0m mosna-core v0.1.0";
        let runs = runs(line);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "   Compiling");
        assert_eq!(runs[0].color, Some(basic(2, true)));
        assert_eq!(runs[1].text, " mosna-core v0.1.0");
        assert_eq!(runs[1].color, None);
    }

    #[test]
    fn bold_brightens_a_basic_colour() {
        let runs = runs("\x1b[1;33mwarning\x1b[0m: unused");
        assert_eq!(runs[0].color, Some(basic(3, true)));
    }

    #[test]
    fn the_escape_codes_do_not_reach_the_log_file() {
        let line = "\x1b[1m\x1b[91merror[E0308]\x1b[0m\x1b[1m: mismatched types\x1b[0m\x1b[K";
        assert_eq!(plain(line), "error[E0308]: mismatched types");
    }

    #[test]
    fn a_carriage_return_keeps_what_a_terminal_would_show() {
        assert_eq!(plain("   Building [===>   ] 10/200\rdone"), "done");
    }

    #[test]
    fn a_hyperlink_is_reduced_to_its_text() {
        let line = "see \x1b]8;;https://example.org\x1b\\the docs\x1b]8;;\x1b\\ here";
        assert_eq!(plain(line), "see the docs here");
    }

    #[test]
    fn extended_colours_are_read() {
        assert_eq!(
            runs("\x1b[38;2;10;20;30mx")[0].color,
            Some(Color32::from_rgb(10, 20, 30))
        );
        assert_eq!(
            runs("\x1b[38;5;196mx")[0].color,
            Some(Color32::from_rgb(255, 0, 0))
        );
        // A background colour neither colours the text nor eats the next code.
        assert_eq!(runs("\x1b[48;5;196;31mx")[0].color, Some(basic(1, false)));
    }

    #[test]
    fn plain_lines_are_classified_by_what_they_say() {
        assert_eq!(classify("==> Installation de Rust"), Kind::Step);
        assert_eq!(classify("error: could not compile `mosna`"), Kind::Error);
        assert_eq!(classify("ÉCHEC : La compilation a échoué."), Kind::Error);
        assert_eq!(classify("warning: unused import"), Kind::Warning);
        assert_eq!(classify("info: downloading component 'rustc'"), Kind::Info);
        assert_eq!(
            classify("Outils C++ de Microsoft : déjà installés."),
            Kind::Success
        );
        assert_eq!(classify("    Finished `release` profile"), Kind::Success);
        assert_eq!(classify("MOSNA est dans C:\\Users\\Me"), Kind::Plain);
    }

    #[test]
    fn a_coloured_line_is_left_to_its_own_colours() {
        let job = layout("\x1b[1;91merror\x1b[0m: failed to build", true);
        assert_eq!(job.sections[0].format.color, basic(1, true));
        assert_eq!(job.sections[1].format.color, theme::LOG_PLAIN);
    }

    #[test]
    fn without_colour_every_line_is_plain() {
        let job = layout("error: nope", false);
        assert_eq!(job.sections[0].format.color, theme::LOG_PLAIN);
    }
}
