use solana_sdk::{instruction::Instruction, pubkey::Pubkey};

use crate::mollusk::helpers::*;

pub fn build_get_version_ix(program_id: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        *program_id,
        &anchor_ix_discriminator("get_version"),
        vec![],
    )
}

#[test]
fn get_version_succeeds() {
    let (mollusk, program_id) = setup_mollusk();

    let ix = build_get_version_ix(&program_id);
    let result = mollusk.process_instruction(&ix, &[]);

    assert!(
        !result.program_result.is_err(),
        "get_version failed: {:?}",
        result.program_result
    );
}
