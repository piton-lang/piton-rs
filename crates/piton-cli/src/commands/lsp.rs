//! `piton lsp` — run the language server over stdio.

use crate::EXIT_SUCCESS;

pub fn run() -> u8 {
    piton_lsp::serve();
    EXIT_SUCCESS
}
