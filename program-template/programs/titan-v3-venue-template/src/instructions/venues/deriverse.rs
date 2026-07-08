use anchor_lang::{prelude::*, solana_program::instruction::Instruction};

pub const PROGRAM_ID: Pubkey = pubkey!("DRVSpZ2YUYYKgZP8XtLhAGtT1zYSCKzeHfb4DgRnrgqD");

pub const SWAP_INSTRUCTION_TAG: u8 = 26;

pub fn swap(
    instr_id: u32,
    input_crncy: u8,
    amount_in: u64,
    account_metas: &[AccountMeta],
) -> Result<Vec<Instruction>> {
    let mut data = Vec::with_capacity(32);
    // tag
    data.push(SWAP_INSTRUCTION_TAG);
    // input_crncy
    data.push(input_crncy);
    // padding_u16
    data.extend_from_slice(&0_u16.to_le_bytes());
    // instr id
    data.extend_from_slice(&instr_id.to_le_bytes());
    // price
    data.extend_from_slice(&0i64.to_le_bytes());
    // amount
    data.extend_from_slice(&(amount_in as i64).to_le_bytes());
    // min_amount_out
    data.extend_from_slice(&0_i64.to_le_bytes());

    Ok(vec![Instruction {
        program_id: PROGRAM_ID,
        accounts: account_metas.to_vec(),
        data,
    }])
}
