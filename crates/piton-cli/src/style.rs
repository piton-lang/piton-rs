//! The colours of the CLI. Output goes through `anstream`. It keeps the colours
//! only for a terminal that allows colour. Output to a pipe stays plain text.
//! Output from a run with `NO_COLOR` also stays plain text.

use anstyle::{AnsiColor, Style};

pub const ERROR: Style = AnsiColor::Red.on_default().bold();
pub const WARNING: Style = AnsiColor::Yellow.on_default().bold();
pub const SUCCESS: Style = AnsiColor::Green.on_default().bold();
pub const HELP: Style = AnsiColor::Cyan.on_default().bold();
pub const NOTE: Style = AnsiColor::Blue.on_default().bold();
/// The line-number gutter beside quoted source.
pub const GUTTER: Style = AnsiColor::Blue.on_default().bold();
/// Text that the reader can type or find, such as a template, a command or a
/// path.
pub const NAME: Style = AnsiColor::Cyan.on_default();
/// A removed line in a diff. It is red, but not bold like an error label.
pub const ERROR_LINE: Style = AnsiColor::Red.on_default();
/// An added line in a diff.
pub const SUCCESS_LINE: Style = AnsiColor::Green.on_default();
pub const HEADING: Style = Style::new().bold();
pub const DIM: Style = Style::new().dimmed();

/// `text` in `style`.
pub fn paint(style: Style, text: impl std::fmt::Display) -> String {
    format!("{style}{text}{style:#}")
}
