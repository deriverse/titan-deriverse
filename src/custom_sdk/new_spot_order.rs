use bytemuck::Zeroable;
use drv_models::{
    constants::{DF, instructions::DrvInstruction},
    instruction_data::NewSpotOrderData,
    state::{
        instrument::InstrAccountHeader,
        token::TokenState,
        types::{
            OrderType,
            account_type::{
                COMMUNITY, INSTR, ROOT, SPOT_1M_CANDLES, SPOT_15M_CANDLES, SPOT_ASK_ORDERS,
                SPOT_ASKS_TREE, SPOT_BID_ORDERS, SPOT_BIDS_TREE, SPOT_CLIENT_INFOS,
                SPOT_CLIENT_INFOS2, SPOT_DAY_CANDLES, SPOT_LINES,
            },
        },
    },
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::{
    custom_sdk::traits::{BuildContext, Context},
    helper::{Helper, get_dec_factor},
    program_id,
};

pub struct NewSpotOrderBuildContext {
    pub signer: Pubkey,
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub price: f64,
    pub amount: f64,
}

impl BuildContext for NewSpotOrderBuildContext {}

pub struct NewSpotOrderContext {
    signer: Pubkey,
    root: Pubkey,
    client_primary: Pubkey,
    client_community: Pubkey,
    instr_acc: Pubkey,
    bids_tree: Pubkey,
    asks_tree: Pubkey,
    bid_orders: Pubkey,
    ask_orders: Pubkey,
    lines: Pubkey,
    maps: Pubkey,
    client_info: Pubkey,
    client_info2: Pubkey,
    candles_1m: Pubkey,
    candles_15m: Pubkey,
    candles_day: Pubkey,
    community: Pubkey,
    a_token_state: TokenState,
    instr_state: InstrAccountHeader,
    pub price: f64,
    pub amount: f64,
}

impl Context for NewSpotOrderContext {
    type Build = NewSpotOrderBuildContext;

    fn build(
        rpc: &solana_client::rpc_client::RpcClient,
        build_ctx: Self::Build,
    ) -> Result<Box<Self>, solana_client::client_error::ClientError> {
        let NewSpotOrderBuildContext {
            signer,
            token_a_mint,
            token_b_mint,
            price,
            amount,
        } = build_ctx;

        let a_token_state = {
            let addr = token_a_mint.new_token_acc();
            let acc = rpc.get_account(&addr)?;
            unsafe { *(acc.data.as_ptr() as *const TokenState) }
        };

        let b_token_state = {
            let addr = token_b_mint.new_token_acc();
            let acc = rpc.get_account(&addr)?;
            unsafe { *(acc.data.as_ptr() as *const TokenState) }
        };

        let instr_addr = Pubkey::new_spot_acc(INSTR, a_token_state.id, b_token_state.id);

        let instr_state = {
            let acc = rpc.get_account(&instr_addr)?;
            unsafe { *(acc.data.as_ptr() as *const InstrAccountHeader) }
        };

        Ok(Box::new(Self {
            signer,
            root: Pubkey::new_acc(ROOT),
            client_primary: signer.new_client_primary_acc(),
            client_community: signer.new_client_community_acc(),
            instr_acc: instr_addr,
            bids_tree: Pubkey::new_spot_acc(SPOT_BIDS_TREE, a_token_state.id, b_token_state.id),
            asks_tree: Pubkey::new_spot_acc(SPOT_ASKS_TREE, a_token_state.id, b_token_state.id),
            bid_orders: Pubkey::new_spot_acc(SPOT_BID_ORDERS, a_token_state.id, b_token_state.id),
            ask_orders: Pubkey::new_spot_acc(SPOT_ASK_ORDERS, a_token_state.id, b_token_state.id),
            lines: Pubkey::new_spot_acc(SPOT_LINES, a_token_state.id, b_token_state.id),
            maps: instr_state.maps_address,
            client_info: Pubkey::new_spot_acc(
                SPOT_CLIENT_INFOS,
                a_token_state.id,
                b_token_state.id,
            ),
            client_info2: Pubkey::new_spot_acc(
                SPOT_CLIENT_INFOS2,
                a_token_state.id,
                b_token_state.id,
            ),
            candles_1m: Pubkey::new_spot_acc(SPOT_1M_CANDLES, a_token_state.id, b_token_state.id),
            candles_15m: Pubkey::new_spot_acc(SPOT_15M_CANDLES, a_token_state.id, b_token_state.id),
            candles_day: Pubkey::new_spot_acc(SPOT_DAY_CANDLES, a_token_state.id, b_token_state.id),
            community: Pubkey::new_acc(COMMUNITY),
            a_token_state,
            instr_state,
            price,
            amount,
        }))
    }

    fn create_instruction(&self) -> solana_sdk::instruction::Instruction {
        let NewSpotOrderContext {
            signer,
            root,
            client_primary,
            client_community,
            instr_acc,
            bids_tree,
            asks_tree,
            bid_orders,
            ask_orders,
            lines,
            maps,
            client_info,
            client_info2,
            candles_1m,
            candles_15m,
            candles_day,
            community,
            a_token_state,

            instr_state,
            amount,
            price,
            ..
        } = self;

        let accounts = vec![
            AccountMeta {
                pubkey: *signer,
                is_signer: true,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *root,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *client_primary,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *client_community,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *instr_acc,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *bids_tree,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *asks_tree,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *bid_orders,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *ask_orders,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *lines,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *maps,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *client_info,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *client_info2,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *candles_1m,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *candles_15m,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *candles_day,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *community,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: solana_sdk::system_program::id(),
                is_signer: false,
                is_writable: false,
            },
        ];

        let qty = (amount * get_dec_factor((a_token_state.mask & 0xFF) as u8) as f64) as i64;

        let instruction_data = NewSpotOrderData {
            tag: drv_models::constants::instructions::NewSpotOrderInstruction::INSTRUCTION_NUMBER,
            order_type: OrderType::Limit as u8,
            instr_id: instr_state.instr_id,
            amount: qty,
            side: if qty > 0 { 0 } else { 1 },
            price: (price * DF) as i64,
            ..NewSpotOrderData::zeroed()
        };

        Instruction::new_with_bytes(
            program_id::ID,
            bytemuck::bytes_of(&instruction_data),
            accounts,
        )
    }
}
