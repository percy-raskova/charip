use tower_lsp::lsp_types::Position;

use crate::vault::Rangeable as _;

use super::Context;

pub fn check_in_code_block(context: &Context, line: usize, character: usize) -> bool {
    context.vault.get_document(context.path).is_some_and(|doc| {
        doc.codeblocks.iter().any(|block| {
            block.includes_position(Position {
                line: line as u32,
                character: character as u32,
            })
        })
    })
}
