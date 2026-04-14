use bytemuck::{Zeroable, bytes_of};
use drv_models::{
    instruction_constants::{DrvInstruction, ExtendCandles},
    instruction_data::ExtendCandlesData,
    new_types::instrument::InstrId,
    state::types::account_type::{INSTR, ROOT},
};
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;

use crate::{helper::Helper, program_id};

pub struct ExtendCandlesBuilder;

impl ExtendCandlesBuilder {
    pub fn extend_candles(
        client: Pubkey,
        asset_token_id: u32,
        crncy_token_id: u32,
        instr_id: InstrId,
        maps_addr: Pubkey,
    ) -> Instruction {
        Instruction::new_with_bytes(
            program_id::id(),
            bytes_of(&ExtendCandlesData {
                tag: ExtendCandles::INSTRUCTION_NUMBER,
                instr_id,
                ..Zeroable::zeroed()
            }),
            vec![
                AccountMeta {
                    pubkey: client,
                    is_signer: true,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: Pubkey::new_acc(ROOT),
                    is_signer: false,
                    is_writable: false,
                },
                AccountMeta {
                    pubkey: Pubkey::new_spot_acc(INSTR, asset_token_id, crncy_token_id),
                    is_signer: false,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: maps_addr,
                    is_signer: false,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: solana_system_interface::program::id(),
                    is_signer: false,
                    is_writable: false,
                },
            ],
        )
    }
}
