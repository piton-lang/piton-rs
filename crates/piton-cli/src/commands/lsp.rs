//! `piton lsp` — run the language server over stdio.

use crate::{EXIT_SUCCESS, VERSION};

pub fn run() -> u8 {
    piton_lsp::serve(VERSION);
    EXIT_SUCCESS
}
