use std::io;

use anyhow::Result;
use bytemuck::{Pod, Zeroable, from_bytes};
use drv_models::{
    constants::{SWAP_FEE_RATE, candles::CANDLES, voting::FEE_RATE_STEP},
    instruction_constants::{DrvInstruction, SwapInstruction},
    instruction_data::SwapData,
    new_types::instrument::InstrId,
    state::{
        candles::{Candle, CandlesAccountHeader},
        community_account_header::CommunityAccountHeader,
        instrument::InstrAccountHeader,
        token::TokenState,
        types::{
            OrderSide,
            account_type::{
                COMMUNITY, INSTR, ROOT, SPOT_1M_CANDLES, SPOT_15M_CANDLES, SPOT_ASK_ORDERS,
                SPOT_ASKS_TREE, SPOT_BID_ORDERS, SPOT_BIDS_TREE, SPOT_CLIENT_INFOS,
                SPOT_CLIENT_INFOS2, SPOT_DAY_CANDLES, SPOT_LINES,
            },
            instr_mask::{InstrFlag, SimpleInstrMask},
        },
    },
};

use jupiter_amm_interface::{
    AccountProvider, Amm, AmmError, Quote, Side, Swap, SwapAndAccountMetas, SwapMode, SwapParams,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::from_value;
use solana_account::ReadableAccount;
use solana_instruction::AccountMeta;
use solana_pubkey::Pubkey;

use crate::{
    amm::DeriverseAmm,
    helper::{Helper, get_by_tag},
    instrument::OffChainInstrAccountHeader,
    order_book::OrderBook,
};

pub mod amm;
pub mod helper;
pub mod instrument;
pub mod lines_linked_list;
pub mod order_book;
pub mod orders_linked_list;

#[cfg(test)]
pub mod custom_sdk;
#[cfg(test)]
pub mod tests;

#[cfg(any(not(test), feature = "mainnet-test"))]
pub mod program_id {

    use drv_models::new_types::version::Version;
    use solana_pubkey::declare_id;

    declare_id!("DRVSpZ2YUYYKgZP8XtLhAGtT1zYSCKzeHfb4DgRnrgqD");
    pub const VERSION: Version = Version(1);
}

#[cfg(all(test, not(feature = "mainnet-test")))]
pub mod program_id {
    use drv_models::new_types::version::Version;
    use solana_pubkey::declare_id;

    declare_id!("hSuxfshizdWKiWCVBPhrLBq1yuwLPrGnfmii3JUn613");
    pub const VERSION: Version = Version(1);
}

#[derive(Clone, Debug, PartialEq)]
struct ContextAccounts {
    instr_header: Pubkey,
    a_token_state_acc: Pubkey,
    b_token_state_acc: Pubkey,
    lines: Pubkey,
    bid_orders: Pubkey,
    ask_orders: Pubkey,
    community_acc: Pubkey,
    a_mint: Pubkey,
    b_mint: Pubkey,
    pub candles: Option<(Pubkey, Pubkey, Pubkey)>,
}

impl From<ContextAccounts> for Vec<Pubkey> {
    fn from(value: ContextAccounts) -> Self {
        let mut vec = vec![
            value.instr_header,
            value.a_token_state_acc,
            value.b_token_state_acc,
            value.community_acc,
            value.lines,
            value.bid_orders,
            value.ask_orders,
            value.a_mint,
            value.b_mint,
        ];

        if let Some(candles) = value.candles {
            vec.extend_from_slice(&[candles.0, candles.1, candles.2]);
        }

        vec
    }
}

impl ContextAccounts {
    pub fn build(instr_header: &InstrAccountHeader) -> Self {
        ContextAccounts {
            instr_header: Pubkey::new_spot_acc(
                INSTR,
                instr_header.asset_token_id,
                instr_header.crncy_token_id,
            ),
            a_token_state_acc: instr_header.asset_mint.new_token_acc(),
            b_token_state_acc: instr_header.crncy_mint.new_token_acc(),
            bid_orders: Pubkey::new_spot_acc(
                SPOT_BID_ORDERS,
                instr_header.asset_token_id,
                instr_header.crncy_token_id,
            ),
            ask_orders: Pubkey::new_spot_acc(
                SPOT_ASK_ORDERS,
                instr_header.asset_token_id,
                instr_header.crncy_token_id,
            ),
            lines: Pubkey::new_spot_acc(
                SPOT_LINES,
                instr_header.asset_token_id,
                instr_header.crncy_token_id,
            ),
            community_acc: Pubkey::new_acc(COMMUNITY),
            a_mint: instr_header.asset_mint,
            b_mint: instr_header.crncy_mint,
            candles: Some((
                Pubkey::new_spot_acc(
                    SPOT_1M_CANDLES,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
                Pubkey::new_spot_acc(
                    SPOT_15M_CANDLES,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
                Pubkey::new_spot_acc(
                    SPOT_DAY_CANDLES,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstructionBuilderParams {
    ata_init: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamsWrapper {
    instruction_builder_params: InstructionBuilderParams,
}

#[derive(Debug, Clone, PartialEq, Zeroable)]
pub struct CandleParams {
    count: u32,
    buffer_len: u32,
    capacity: u32,
}

impl CandleParams {
    pub fn new<const TAG: u32, T: ReadableAccount>(account: &T) -> Self {
        let header: &CandlesAccountHeader<0> =
            from_bytes(&account.data()[..std::mem::size_of::<CandlesAccountHeader<0>>()]);

        let buffer_len = (account.data().len() - std::mem::size_of::<CandlesAccountHeader<0>>())
            / std::mem::size_of::<Candle>();

        Self {
            count: header.count,
            buffer_len: buffer_len as u32,
            capacity: get_by_tag::<TAG>(CANDLES).capacity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Zeroable)]
pub struct Candles {
    candle_1m: CandleParams,
    candle_15m: CandleParams,
    candle_day: CandleParams,
}

#[derive(Clone, Debug, PartialEq)]
struct Deriverse {
    accounts_ctx: ContextAccounts,
    instr_header: Box<InstrAccountHeader>,
    a_token_state: TokenState,
    b_token_state: TokenState,
    order_book: OrderBook,
    amm: DeriverseAmm,
    fee_rate_factor: f64,
    instruction_builder_params: InstructionBuilderParams,
    candles: Option<Candles>,
    a_program_id: Pubkey,
    b_program_id: Pubkey,
}

pub trait AccountsHolder {
    fn from_account<T: Pod>(&self, account_addr: &Pubkey) -> Result<T, AmmError>;
}

impl<P: AccountProvider> AccountsHolder for P {
    fn from_account<T: Pod>(&self, account_addr: &Pubkey) -> Result<T, AmmError> {
        let acc = self.try_get(account_addr)?;

        Ok(*bytemuck::from_bytes(
            &acc.data()[0..std::mem::size_of::<T>()],
        ))
    }
}

impl Amm for Deriverse {
    fn from_keyed_account(
        keyed_account: &jupiter_amm_interface::KeyedAccount,
        _: &jupiter_amm_interface::AmmContext,
    ) -> Result<Self, AmmError>
    where
        Self: Sized,
    {
        let instr_header = Box::new(*bytemuck::from_bytes::<InstrAccountHeader>(
            &keyed_account.account.data.as_slice()[..std::mem::size_of::<InstrAccountHeader>()],
        ));

        let accounts_ctx = ContextAccounts::build(instr_header.as_ref());

        let params: ParamsWrapper = if let Some(ref params) = keyed_account.params {
            from_value(params.clone())?
        } else {
            return Err(AmmError::Custom(
                "Need params were not provided in KeydAccount".to_string(),
            ));
        };

        Ok(Deriverse {
            instr_header,
            accounts_ctx,
            a_token_state: TokenState::zeroed(),
            b_token_state: TokenState::zeroed(),
            order_book: OrderBook::default(),
            amm: DeriverseAmm::default(),
            fee_rate_factor: 0.0,
            a_program_id: solana_system_interface::program::id(),
            b_program_id: solana_system_interface::program::id(),
            instruction_builder_params: params.instruction_builder_params,
            candles: None,
        })
    }

    fn label(&self) -> &'static str {
        "Deriverse"
    }

    fn program_id(&self) -> Pubkey {
        program_id::id()
    }

    fn key(&self) -> Pubkey {
        self.accounts_ctx.instr_header
    }

    fn has_dynamic_accounts(&self) -> bool {
        false
    }

    fn get_accounts_len(&self) -> usize {
        SwapInstruction::MIN_ACCOUNTS
            + (self.a_program_id != self.b_program_id) as usize
            + self.instruction_builder_params.ata_init as usize * 2
    }

    fn get_reserve_mints(&self) -> Vec<Pubkey> {
        vec![self.a_token_state.address, self.b_token_state.address]
    }

    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        self.accounts_ctx.clone().into()
    }

    fn update(
        &mut self,
        account_provider: impl jupiter_amm_interface::AccountProvider,
    ) -> std::result::Result<(), jupiter_amm_interface::AmmError> {
        let ContextAccounts {
            instr_header,
            a_token_state_acc,
            b_token_state_acc,
            lines,
            community_acc,
            a_mint,
            b_mint,
            bid_orders,
            ask_orders,
            candles,
        } = &self.accounts_ctx;

        *self.instr_header = account_provider.from_account(instr_header)?;
        self.a_token_state = account_provider.from_account(a_token_state_acc)?;
        self.b_token_state = account_provider.from_account(b_token_state_acc)?;

        self.fee_rate_factor = account_provider
            .from_account::<CommunityAccountHeader>(community_acc)?
            .spot_fee_rate as f64
            * FEE_RATE_STEP;

        let lines_acc = account_provider.try_get(lines)?;

        let ask_orders_acc = account_provider.try_get(ask_orders)?;

        let bid_orders_acc = account_provider.try_get(bid_orders)?;

        self.order_book = OrderBook::new(
            &self.instr_header,
            lines_acc,
            bid_orders_acc,
            ask_orders_acc,
        );
        self.amm = DeriverseAmm::new(&self.instr_header);

        let a_mint_acc = account_provider.try_get(a_mint)?;
        self.a_program_id = *a_mint_acc.owner();

        let b_mint_acc = account_provider.try_get(b_mint)?;
        self.b_program_id = *b_mint_acc.owner();

        if let Some((candle_1m, candle_15m, candle_day)) = candles {
            let candle_1m_acc = account_provider.try_get(candle_1m)?;
            let candle_15m_acc = account_provider.try_get(candle_15m)?;
            let candle_day_acc = account_provider.try_get(candle_day)?;

            self.candles = Some(Candles {
                candle_1m: CandleParams::new::<SPOT_1M_CANDLES, _>(&candle_1m_acc),
                candle_15m: CandleParams::new::<SPOT_15M_CANDLES, _>(&candle_15m_acc),
                candle_day: CandleParams::new::<SPOT_DAY_CANDLES, _>(&candle_day_acc),
            })
        }

        Ok(())
    }

    fn quote(
        &self,
        quote_params: &jupiter_amm_interface::QuoteParams,
    ) -> Result<jupiter_amm_interface::Quote, AmmError> {
        let Deriverse {
            instr_header,
            b_token_state,
            order_book,
            amm,
            fee_rate_factor,
            ..
        } = self;

        // reversed swap
        if quote_params.swap_mode == SwapMode::ExactOut {
            return Err(AmmError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Exact out is not supported",
            )));
        }

        let mut amm = amm.clone();

        let buy = b_token_state.address == quote_params.input_mint;

        let px = instr_header.market_px();
        let price = {
            let max_diff = if instr_header.mask.get_flag(InstrFlag::SimilarAssets) {
                px >> 4
            } else {
                px >> 3
            };

            if buy { px + max_diff } else { px - max_diff }
        };

        let fee_rate = if instr_header.mask.get_flag(InstrFlag::FixedFees) {
            instr_header.fixed_fee_rate
        } else if instr_header.mask.get_flag(InstrFlag::ZeroFees) {
            0.0
        } else {
            instr_header.day_volatility * fee_rate_factor
        };

        let swap_fee_rate = if instr_header.mask.get_flag(InstrFlag::ZeroFees) {
            0.0
        } else {
            SWAP_FEE_RATE
        };

        let mut client_tokens: i64 = 0;
        let mut client_mints: i64 = 0;
        let mut fees_amount: i64 = 0;
        let mut swap_fees: i64 = 0;

        if buy && (price > px || order_book.cross(price, OrderSide::Ask)) {
            let input_sum = (quote_params.amount as f64 / (1.0 + fee_rate + swap_fee_rate)) as i64;

            if swap_fee_rate > 0.0 {
                swap_fees = (input_sum as f64 * swap_fee_rate) as i64;
            }

            let estimated_fees = (quote_params.amount as i64 - input_sum - swap_fees).max(0);

            let mut remaining_sum = input_sum;
            let mut qty = 0_i64;
            let mut total_fees = 0_i64;
            let mut amm_px;
            let traded_qty;
            let traded_mints;
            let mut next_amm_px;
            let mut exhausted = false;

            let mut lines = order_book.iter_asks();

            loop {
                let line = lines.next();

                amm_px = amm.get_reversed_amm_px(remaining_sum)?;

                if line.is_none() {
                    if DeriverseAmm::partial_fill(amm_px, price, OrderSide::Ask) {
                        exhausted = true;
                        let preliminary_qty = amm.get_amm_qty(price, OrderSide::Ask)?;
                        traded_mints = amm.get_amm_sum(preliminary_qty, OrderSide::Ask)?;
                        traded_qty = amm.get_reversed_amm_qty(traded_mints)?;
                        if traded_qty == 0 || traded_mints == 0 {
                            break;
                        }
                    } else {
                        traded_qty = amm.get_reversed_amm_qty(remaining_sum)?;
                        if traded_qty == 0 {
                            break;
                        }
                        traded_mints = remaining_sum;
                    }
                    remaining_sum -= traded_mints;

                    qty = qty
                        .checked_add(traded_qty)
                        .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                    amm.a_tokens = amm
                        .a_tokens
                        .checked_sub(traded_qty)
                        .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                    amm.b_tokens = amm
                        .b_tokens
                        .checked_add(traded_mints)
                        .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                    total_fees = total_fees
                        .checked_add((traded_mints as f64 * fee_rate) as i64)
                        .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                    break;
                }

                if let Some((_, line)) = line {
                    let line_sum = order_book.line_sum(&line, OrderSide::Ask, remaining_sum);

                    // Proff of assumption - remaining_qty <= line_qty if remaining_sum <= line_sum
                    // remaining_qty =
                    //     remaining_sum * amm.df / line.price;
                    //
                    // line_sum = line_qty * line_price / amm.df
                    // line_qty = line_sum * amm.df / line.price

                    if remaining_sum <= line_sum {
                        if DeriverseAmm::last_line(amm_px, line.price, OrderSide::Ask) {
                            if DeriverseAmm::partial_fill(amm_px, price, OrderSide::Ask) {
                                traded_qty = amm.get_amm_qty(price, OrderSide::Ask)?;
                                traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Ask)?;
                                if traded_qty == 0 || traded_mints == 0 {
                                    break;
                                }
                            } else {
                                traded_qty = amm.get_reversed_amm_qty(remaining_sum)?;
                                if traded_qty == 0 {
                                    break;
                                }
                                traded_mints = remaining_sum;
                            }

                            remaining_sum -= traded_mints;
                            qty = qty
                                .checked_add(traded_qty)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                            amm.a_tokens = amm
                                .a_tokens
                                .checked_sub(traded_qty)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                            amm.b_tokens = amm
                                .b_tokens
                                .checked_add(traded_mints)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        } else if DeriverseAmm::line_is_unreachable(
                            price,
                            line.price,
                            OrderSide::Ask,
                        ) {
                            traded_qty = amm.get_amm_qty(price, OrderSide::Ask)?;
                            traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Ask)?;
                            if traded_qty == 0 || traded_mints == 0 {
                                break;
                            }

                            remaining_sum -= traded_mints;
                            qty = qty
                                .checked_add(traded_qty)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                            amm.a_tokens = amm
                                .a_tokens
                                .checked_sub(traded_qty)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                            amm.b_tokens = amm
                                .b_tokens
                                .checked_add(traded_mints)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        } else {
                            traded_qty = amm.get_amm_qty(line.price, OrderSide::Ask)?;
                            traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Ask)?;
                            if traded_qty != 0 && traded_mints != 0 {
                                remaining_sum -= traded_mints;
                                qty = qty
                                    .checked_add(traded_qty)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                                amm.a_tokens = amm
                                    .a_tokens
                                    .checked_sub(traded_qty)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                                amm.b_tokens = amm
                                    .b_tokens
                                    .checked_add(traded_mints)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                            }
                            if remaining_sum > 0 {
                                let init_qty =
                                    (remaining_sum as f64 * self.amm.df / line.price as f64) as i64;

                                let (traded_qty, traded_sum, traded_fees) = self.order_book.fill(
                                    &line,
                                    init_qty,
                                    fee_rate,
                                    OrderSide::Ask,
                                )?;

                                qty = qty
                                    .checked_add(traded_qty)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                                total_fees = total_fees
                                    .checked_add(traded_fees)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                                remaining_sum -= traded_sum;
                            }
                        }
                        if traded_qty != 0 && traded_mints != 0 {
                            total_fees = total_fees
                                .checked_add((traded_mints as f64 * fee_rate) as i64)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        }

                        break;
                    }

                    next_amm_px = amm.get_reversed_amm_px(remaining_sum - line_sum)?;
                    if DeriverseAmm::cover_line(next_amm_px, price, line.price, OrderSide::Ask) {
                        let init_qty =
                            (remaining_sum as f64 * self.amm.df / line.price as f64) as i64;

                        let (traded_qty, traded_sum, traded_fees) =
                            self.order_book
                                .fill(&line, init_qty, fee_rate, OrderSide::Ask)?;

                        qty = qty
                            .checked_add(traded_qty)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                        total_fees = total_fees
                            .checked_add(traded_fees)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                        remaining_sum -= traded_sum;
                        continue;
                    }

                    traded_mints = amm
                        .get_reversed_amm_sum(line.price.min(price))?
                        .min(remaining_sum);

                    traded_qty = amm.get_reversed_amm_qty(traded_mints)?;

                    if traded_qty != 0 && traded_mints != 0 {
                        remaining_sum -= traded_mints;
                        qty = qty
                            .checked_add(traded_qty)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                        amm.a_tokens = amm
                            .a_tokens
                            .checked_sub(traded_qty)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        amm.b_tokens = amm
                            .b_tokens
                            .checked_add(traded_mints)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                        total_fees = total_fees
                            .checked_add((traded_mints as f64 * fee_rate) as i64)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                    }

                    if DeriverseAmm::cover_line(amm_px, price, line.price, OrderSide::Ask) {
                        let init_qty =
                            (remaining_sum as f64 * self.amm.df / line.price as f64) as i64;

                        let (traded_qty, traded_sum, traded_fees) =
                            self.order_book
                                .fill(&line, init_qty, fee_rate, OrderSide::Ask)?;

                        qty = qty
                            .checked_add(traded_qty)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                        total_fees = total_fees
                            .checked_add(traded_fees)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                        remaining_sum -= traded_sum;
                    }

                    break;
                }
            }

            client_tokens += qty;

            let mut incr = false;

            if remaining_sum == 1 {
                if estimated_fees > 0 {
                    total_fees = estimated_fees + 1;
                    incr = true;
                } else {
                    remaining_sum = 0;
                }
            } else if remaining_sum == 0 {
                total_fees = estimated_fees;
            }

            let traded_sum = input_sum - remaining_sum;

            total_fees = total_fees.max((traded_sum as f64 * fee_rate) as i64);
            if exhausted && fee_rate > 0.0 && !incr {
                total_fees += 1;
            }

            client_mints -= traded_sum;

            if remaining_sum > 1 && swap_fee_rate > 0.0 {
                swap_fees = (traded_sum as f64 * swap_fee_rate) as i64;
            }

            client_mints -= total_fees + swap_fees;

            fees_amount = total_fees;
        } else if !buy && (price < px || order_book.cross(price, OrderSide::Bid)) {
            let mut remaining_qty = quote_params.amount as i64;
            let mut sum = 0_i64;
            let mut total_fees = 0_i64;
            let mut amm_px;
            let traded_qty;
            let traded_mints;
            let mut next_amm_px;

            let mut lines = order_book.iter_bids();

            loop {
                let line = lines.next();

                amm_px = amm.get_amm_px(remaining_qty, OrderSide::Bid)?;

                if line.is_none() {
                    if DeriverseAmm::partial_fill(amm_px, price, OrderSide::Bid) {
                        traded_qty = amm.get_amm_qty(price, OrderSide::Bid)?;
                        traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Bid)?;
                        if traded_qty == 0 || traded_mints == 0 {
                            break;
                        }
                    } else {
                        traded_mints = amm.get_amm_sum(remaining_qty, OrderSide::Bid)?;
                        if traded_mints == 0 {
                            break;
                        }
                        traded_qty = remaining_qty;
                    }

                    remaining_qty -= traded_qty;
                    sum = sum
                        .checked_add(traded_mints)
                        .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                    amm.a_tokens = amm
                        .a_tokens
                        .checked_add(traded_qty)
                        .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                    amm.b_tokens = amm
                        .b_tokens
                        .checked_sub(traded_mints)
                        .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                    total_fees = total_fees
                        .checked_add((traded_mints as f64 * fee_rate) as i64)
                        .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                    break;
                }

                if let Some((_, line)) = line {
                    if remaining_qty <= line.qty {
                        if DeriverseAmm::last_line(amm_px, line.price, OrderSide::Bid) {
                            if DeriverseAmm::partial_fill(amm_px, price, OrderSide::Bid) {
                                traded_qty = amm.get_amm_qty(price, OrderSide::Bid)?;
                                traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Bid)?;
                                if traded_qty == 0 || traded_mints == 0 {
                                    break;
                                }
                            } else {
                                traded_mints = amm.get_amm_sum(remaining_qty, OrderSide::Bid)?;
                                if traded_mints == 0 {
                                    break;
                                }
                                traded_qty = remaining_qty;
                            }

                            remaining_qty -= traded_qty;
                            sum = sum
                                .checked_add(traded_mints)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                            amm.a_tokens = amm
                                .a_tokens
                                .checked_add(traded_qty)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                            amm.b_tokens = amm
                                .b_tokens
                                .checked_sub(traded_mints)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        } else if DeriverseAmm::line_is_unreachable(
                            price,
                            line.price,
                            OrderSide::Bid,
                        ) {
                            traded_qty = amm.get_amm_qty(price, OrderSide::Bid)?;
                            traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Bid)?;
                            if traded_qty == 0 || traded_mints == 0 {
                                break;
                            }
                            remaining_qty -= traded_qty;
                            sum = sum
                                .checked_add(traded_mints)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                            amm.a_tokens = amm
                                .a_tokens
                                .checked_add(traded_qty)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                            amm.b_tokens = amm
                                .b_tokens
                                .checked_sub(traded_mints)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        } else {
                            traded_qty = amm.get_amm_qty(line.price, OrderSide::Bid)?;
                            traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Bid)?;

                            if traded_qty != 0 && traded_mints != 0 {
                                remaining_qty -= traded_qty;
                                sum = sum
                                    .checked_add(traded_mints)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                                amm.a_tokens = amm
                                    .a_tokens
                                    .checked_add(traded_qty)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                                amm.b_tokens = amm
                                    .b_tokens
                                    .checked_sub(traded_mints)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                            }

                            if remaining_qty > 0 {
                                let (traded_qty, traded_sum, traded_fees) = self.order_book.fill(
                                    &line,
                                    remaining_qty,
                                    fee_rate,
                                    OrderSide::Bid,
                                )?;

                                total_fees = total_fees
                                    .checked_add(traded_fees)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                                sum = sum
                                    .checked_add(traded_sum)
                                    .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                                remaining_qty -= traded_qty;
                            }
                        }

                        if traded_mints != 0 && traded_qty != 0 {
                            total_fees = total_fees
                                .checked_add((traded_mints as f64 * fee_rate) as i64)
                                .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        }
                        break;
                    }

                    next_amm_px = amm.get_amm_px(remaining_qty - line.qty, OrderSide::Bid)?;

                    if DeriverseAmm::cover_line(next_amm_px, price, line.price, OrderSide::Bid) {
                        let (traded_qty, traded_sum, traded_fees) =
                            self.order_book
                                .fill(&line, remaining_qty, fee_rate, OrderSide::Bid)?;

                        total_fees = total_fees
                            .checked_add(traded_fees)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        sum = sum
                            .checked_add(traded_sum)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                        remaining_qty -= traded_qty;

                        continue;
                    }

                    traded_qty = amm
                        .get_amm_qty(line.price.max(price), OrderSide::Bid)?
                        .min(remaining_qty);
                    traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Bid)?;

                    if traded_qty != 0 && traded_mints != 0 {
                        remaining_qty -= traded_qty;
                        sum = sum
                            .checked_add(traded_mints)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        amm.a_tokens = amm
                            .a_tokens
                            .checked_add(traded_qty)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        amm.b_tokens = amm
                            .b_tokens
                            .checked_sub(traded_mints)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                        total_fees = total_fees
                            .checked_add((traded_mints as f64 * fee_rate) as i64)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                    }

                    if DeriverseAmm::cover_line(next_amm_px, price, line.price, OrderSide::Bid) {
                        let (traded_qty, traded_sum, traded_fees) =
                            self.order_book
                                .fill(&line, remaining_qty, fee_rate, OrderSide::Bid)?;

                        total_fees = total_fees
                            .checked_add(traded_fees)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;
                        sum = sum
                            .checked_add(traded_sum)
                            .ok_or(AmmError::Custom("Arithmetic overflow".to_string()))?;

                        remaining_qty -= traded_qty;
                    }
                }

                break;
            }
            client_tokens -= quote_params.amount as i64 - remaining_qty;
            client_mints += sum;

            if swap_fee_rate > 0.0 {
                swap_fees = (sum as f64 * swap_fee_rate) as i64;
            }

            client_mints -= total_fees + swap_fees;
            fees_amount = total_fees;
        }

        if client_tokens == 0 || client_mints == 0 {
            return Err(AmmError::Custom("Swap failed".to_string()));
        }

        if buy {
            Ok(Quote {
                in_amount: (-client_mints) as u64,
                out_amount: client_tokens as u64,
                fee_amount: (fees_amount + swap_fees) as u64,
                fee_mint: b_token_state.address,
                fee_pct: Decimal::from(fees_amount + swap_fees) / Decimal::from(-1 * client_mints),
            })
        } else {
            Ok(Quote {
                in_amount: (-client_tokens) as u64,
                out_amount: client_mints as u64,
                fee_amount: (fees_amount + swap_fees) as u64,
                fee_mint: b_token_state.address,
                fee_pct: Decimal::from(fees_amount + swap_fees) / Decimal::from(client_mints),
            })
        }
    }

    fn get_swap_and_account_metas(
        &self,
        swap_params: &SwapParams,
    ) -> Result<jupiter_amm_interface::SwapAndAccountMetas, AmmError> {
        let Deriverse {
            instr_header,
            accounts_ctx,
            a_token_state,
            b_token_state,
            a_program_id,
            b_program_id,
            instruction_builder_params,
            ..
        } = self;

        let SwapParams {
            destination_mint,
            source_mint,
            source_token_account,
            destination_token_account,
            token_transfer_authority,
            ..
        } = swap_params;

        let (side, a_account, b_account) = if b_token_state.address == *source_mint {
            if a_token_state.address != *destination_mint {
                return Err(AmmError::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid destination mint is provided",
                )));
            }
            (Side::Bid, destination_token_account, source_token_account)
        } else if b_token_state.address == *destination_mint {
            if a_token_state.address != *source_mint {
                return Err(AmmError::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid source mint is provided",
                )));
            }
            (Side::Ask, source_token_account, destination_token_account)
        } else {
            return Err(AmmError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "None of source mint and destination mint matches crcny mint {}",
                    b_token_state.address
                ),
            )));
        };

        let root = Pubkey::new_acc(ROOT);

        let mut account_metas = vec![
            AccountMeta {
                pubkey: *token_transfer_authority,
                is_signer: true,
                is_writable: true,
            },
            AccountMeta {
                pubkey: root,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: instr_header.asset_mint,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: instr_header.crncy_mint,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: Pubkey::get_drv_auth(),
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: a_token_state.program_address,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: b_token_state.program_address,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: self.accounts_ctx.instr_header,
                is_signer: false,
                is_writable: true,
            },
        ];

        match side {
            Side::Bid => account_metas.extend_from_slice(&[
                AccountMeta {
                    pubkey: Pubkey::new_spot_acc(
                        SPOT_ASKS_TREE,
                        instr_header.asset_token_id,
                        instr_header.crncy_token_id,
                    ),
                    is_signer: false,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: Pubkey::new_spot_acc(
                        SPOT_ASK_ORDERS,
                        instr_header.asset_token_id,
                        instr_header.crncy_token_id,
                    ),
                    is_signer: false,
                    is_writable: true,
                },
            ]),
            Side::Ask => account_metas.extend_from_slice(&[
                AccountMeta {
                    pubkey: Pubkey::new_spot_acc(
                        SPOT_BIDS_TREE,
                        instr_header.asset_token_id,
                        instr_header.crncy_token_id,
                    ),
                    is_signer: false,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: Pubkey::new_spot_acc(
                        SPOT_BID_ORDERS,
                        instr_header.asset_token_id,
                        instr_header.crncy_token_id,
                    ),
                    is_signer: false,
                    is_writable: true,
                },
            ]),
        }

        account_metas.extend_from_slice(&[
            AccountMeta {
                pubkey: Pubkey::new_spot_acc(
                    SPOT_LINES,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: instr_header.maps_address,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: Pubkey::new_spot_acc(
                    SPOT_CLIENT_INFOS,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: Pubkey::new_spot_acc(
                    SPOT_CLIENT_INFOS2,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: Pubkey::new_spot_acc(
                    SPOT_1M_CANDLES,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: Pubkey::new_spot_acc(
                    SPOT_15M_CANDLES,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: Pubkey::new_spot_acc(
                    SPOT_DAY_CANDLES,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: accounts_ctx.community_acc,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *a_account,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: *b_account,
                is_signer: false,
                is_writable: true,
            },
        ]);

        account_metas.push(AccountMeta {
            pubkey: *a_program_id,
            is_signer: false,
            is_writable: false,
        });

        if b_program_id != a_program_id {
            account_metas.push(AccountMeta {
                pubkey: *b_program_id,
                is_signer: false,
                is_writable: false,
            });
        }

        if instruction_builder_params.ata_init {
            account_metas.push(AccountMeta {
                pubkey: solana_system_interface::program::id(),
                is_signer: false,
                is_writable: false,
            });
            account_metas.push(AccountMeta {
                pubkey: spl_associated_token_account::id(),
                is_signer: false,
                is_writable: false,
            });
        }

        Ok(SwapAndAccountMetas {
            swap: Swap::Deriverse {
                side,
                instr_id: *instr_header.instr_id,
            },
            account_metas,
        })
    }

    fn is_active(&self) -> bool {
        let suspended_instrument = self.instr_header.mask.get_flag(InstrFlag::Suspended);

        let market_requirements =
            self.order_book.total_lines_count != 0 || self.instr_header.ps != 0;

        let candles_requirements = if let Some(Candles {
            ref candle_1m,
            ref candle_15m,
            ref candle_day,
        }) = self.candles
        {
            (candle_1m.count + 3 < candle_1m.buffer_len
                || candle_1m.buffer_len >= candle_1m.capacity)
                && (candle_15m.count + 1 < candle_15m.buffer_len
                    || candle_15m.buffer_len >= candle_15m.capacity)
                && (candle_day.count + 1 < candle_day.buffer_len
                    || candle_day.buffer_len >= candle_day.capacity)
        } else {
            true
        };

        market_requirements && candles_requirements && !suspended_instrument
    }
}

fn from_swap(swap: Swap, in_amount: u64) -> SwapData {
    if let Swap::Deriverse { side, instr_id } = swap {
        SwapData {
            tag: SwapInstruction::INSTRUCTION_NUMBER,
            input_crncy: (side == Side::Bid) as u8,
            instr_id: InstrId(instr_id),
            price: 0,
            amount: in_amount as i64,
            ..SwapData::zeroed()
        }
    } else {
        panic!("Incorrect swap")
    }
}
