use bytemuck::Zeroable;
use drv_models::{
    constants::instructions::DrvInstruction,
    instruction_data::DepositData,
    state::{token::TokenState, types::account_type::ROOT},
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use spl_associated_token_account::get_associated_token_address_with_program_id;

use crate::{
    Helper,
    custom_sdk::traits::{BuildContext, Context},
    helper::get_dec_factor,
    program_id,
};

pub struct DepositContext {
    pub signer: Pubkey,
    pub client_ata: Pubkey,
    pub token_state: TokenState,
    pub token_state_addr: Pubkey,
    pub token_mint: Pubkey,
    pub root_account: Pubkey,
    pub client_primary_account: Pubkey,
    pub token_program: Pubkey,
    pub client_community_account: Pubkey,
    pub lut_acc: Pubkey,
    pub amount: i64,
    pub deposit_all: bool,
    pub client_account_exists: bool,
    pub lut_slot: u64,
}

pub struct DepositBuildContext {
    pub signer: Pubkey,
    pub token_mint: Pubkey,
    pub amount: i64,
    pub deposit_all: bool,
}

impl BuildContext for DepositBuildContext {}

impl Context for DepositContext {
    type Build = DepositBuildContext;

    fn build(
        rpc: &RpcClient,
        build_ctx: Self::Build,
    ) -> Result<Box<Self>, solana_client::client_error::ClientError> {
        let DepositBuildContext {
            signer,
            token_mint,
            amount,
            deposit_all,
        } = build_ctx;

        let mint_acc = rpc.get_account(&token_mint)?;

        let client_ata =
            get_associated_token_address_with_program_id(&signer, &token_mint, &mint_acc.owner);

        let token_state_addr = token_mint.new_token_acc();

        let token_state = {
            let acc = rpc.get_account(&token_state_addr)?;
            unsafe { *(acc.data.as_ptr() as *const TokenState) }
        };

        let slot = rpc.get_slot()?;

        let lut = solana_sdk::address_lookup_table::instruction::create_lookup_table(
            signer, signer, slot,
        );

        let client_primary_account = signer.new_client_primary_acc();

        Ok(Box::new(Self {
            signer,
            client_ata,
            token_state,
            token_state_addr,
            token_mint,
            root_account: Pubkey::new_acc(ROOT),
            client_primary_account,
            token_program: mint_acc.owner,
            client_community_account: signer.new_client_community_acc(),
            amount,
            deposit_all,
            client_account_exists: rpc.get_account(&client_primary_account).is_ok(),
            lut_acc: lut.1,
            lut_slot: slot,
        }))
    }

    fn create_instruction(&self) -> Instruction {
        let DepositContext {
            signer,
            client_ata,
            token_state,
            token_mint,
            root_account,
            client_primary_account,
            token_program,
            client_community_account,
            amount,
            deposit_all,
            client_account_exists,
            lut_slot,
            token_state_addr,
            lut_acc,
        } = self;

        let mut accounts = vec![
            AccountMeta {
                pubkey: *signer,
                is_signer: true,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *client_ata,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: token_state.program_address,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *token_mint,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *root_account,
                is_signer: false,
                is_writable: !client_account_exists,
            },
            AccountMeta {
                pubkey: *token_state_addr,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *client_primary_account,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: solana_system_interface::program::id(),
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *token_program,
                is_signer: false,
                is_writable: false,
            },
        ];

        if !client_account_exists {
            accounts.push(AccountMeta {
                pubkey: *client_community_account,
                is_signer: false,
                is_writable: true,
            });
            accounts.push(AccountMeta {
                pubkey: *lut_acc,
                is_signer: false,
                is_writable: true,
            });
            accounts.push(AccountMeta {
                pubkey: solana_sdk::address_lookup_table::program::id(),
                is_signer: false,
                is_writable: false,
            });
        }

        let qty = amount * get_dec_factor((token_state.mask & 0xFF) as u8);

        let instruction_data = DepositData {
            tag: drv_models::constants::instructions::DepositInstruction::INSTRUCTION_NUMBER,
            token_id: token_state.id,
            amount: qty,
            deposit_all: *deposit_all as u8,
            lut_slot: *lut_slot as u32,
            ..DepositData::zeroed()
        };

        Instruction::new_with_bytes(
            program_id::ID,
            bytemuck::bytes_of(&instruction_data),
            accounts,
        )
    }
}
