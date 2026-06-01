use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, pubkey::Pubkey,
};

entrypoint!(process_instruction);

/// Written to the last bytes of the first writable account on every invocation.
/// Tests can assert this marker to verify that a CPI to this stub was made.
pub const INVOCATION_MARKER: [u8; 4] = [0xCA, 0xFE, 0xBA, 0xBE];

fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> ProgramResult {
    for account in accounts {
        if account.is_writable && account.owner == program_id {
            let mut data = account.try_borrow_mut_data()?;
            let len = data.len();
            if len >= INVOCATION_MARKER.len() {
                data[len - INVOCATION_MARKER.len()..].copy_from_slice(&INVOCATION_MARKER);
            }
            break;
        }
    }
    Ok(())
}

// Satisfies anchor's IDL extraction step: anchor build runs
//   cargo +nightly test __anchor_private_print_idl --features idl-build -- --show-output --quiet
// and parses the stdout looking for lines delimited by "--- IDL begin/end program ---".
// This test prints a minimal empty IDL in that format so anchor can proceed.
#[test]
#[allow(non_snake_case)]
fn __anchor_private_print_idl() {
    println!("--- IDL begin address ---");
    println!("11111111111111111111111111111111");
    println!("--- IDL begin program ---");
    println!("{}", r#"{"address":"11111111111111111111111111111111","metadata":{"name":"stub_program","version":"0.1.0","spec":"0.1.0"},"instructions":[]}"#);
    println!("--- IDL end program ---");
}
