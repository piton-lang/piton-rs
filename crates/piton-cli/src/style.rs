//! The CLI's colours. Output is printed through `anstream`, which keeps these
//! only when it goes to a terminal that allows colour: piped output, and any
//! run with `NO_COLOR` set, stay plain text.

use anstyle::{AnsiColor, Style};

pub const ERROR: Style = AnsiColor::Red.on_default().bold();
pub const WARNING: Style = AnsiColor::Yellow.on_default().bold();
pub const SUCCESS: Style = AnsiColor::Green.on_default().bold();
pub const HELP: Style = AnsiColor::Cyan.on_default().bold();
pub const NOTE: Style = AnsiColor::Blue.on_default().bold();
/// The line-number gutter beside quoted source.
pub const GUTTER: Style = AnsiColor::Blue.on_default().bold();
/// Something the reader can type or look for: a template, a command, a path.
pub const NAME: Style = AnsiColor::Cyan.on_default();
/// A removed line in a diff: red, without the weight of an error label.
pub const ERROR_LINE: Style = AnsiColor::Red.on_default();
/// An added line in a diff.
pub const SUCCESS_LINE: Style = AnsiColor::Green.on_default();
pub const HEADING: Style = Style::new().bold();
pub const DIM: Style = Style::new().dimmed();

/// `text` in `style`.
pub fn paint(style: Style, text: impl std::fmt::Display) -> String {
    format!("{style}{text}{style:#}")
}
