use drv_models::{
    instruction_constants::{DrvInstruction, ExtendCandles},
    state::types::account_type::ROOT,
};
use solana_sdk::{
    message::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::{helper::Helper, program_id};

pub struct ExtendCandlesBuilder;

impl ExtendCandlesBuilder {
    pub fn extend_candles(
        client: Pubkey,
        candles_tag: u32,
        asset_token_id: u32,
        crncy_token_id: u32,
    ) -> Instruction {
        Instruction::new_with_bytes(
            program_id::id(),
            &[ExtendCandles::INSTRUCTION_NUMBER],
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
                    pubkey: Pubkey::new_spot_acc(candles_tag, asset_token_id, crncy_token_id),
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
