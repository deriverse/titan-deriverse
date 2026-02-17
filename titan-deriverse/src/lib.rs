use anyhow::{anyhow, bail, Result};
use bytemuck::{from_bytes, Pod, Zeroable};
use drv_models::{
    constants::{candles::CANDLES, voting::FEE_RATE_STEP},
    instruction_constants::{DrvInstruction, SwapInstruction},
    instruction_data::SwapData,
    new_types::instrument::InstrId,
    state::{
        candles::{Candle, CandlesAccountHeader},
        community_account_header::CommunityAccountHeader,
        instrument::InstrAccountHeader,
        token::TokenState,
        types::{
            account_type::{
                COMMUNITY, INSTR, ROOT, SPOT_15M_CANDLES, SPOT_1M_CANDLES, SPOT_ASKS_TREE,
                SPOT_ASK_ORDERS, SPOT_BIDS_TREE, SPOT_BID_ORDERS, SPOT_CLIENT_INFOS,
                SPOT_CLIENT_INFOS2, SPOT_DAY_CANDLES, SPOT_LINES,
            },
            OrderSide,
        },
    },
};

use jupiter_amm_interface::{AccountMap, Amm, Quote, Side, Swap, SwapAndAccountMetas, SwapParams};
use serde::{Deserialize, Serialize};
use serde_json::from_value;
use solana_sdk::{account::Account, instruction::AccountMeta, pubkey::Pubkey};

use crate::{
    amm::DeriverseAmm,
    helper::{get_by_tag, Helper},
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

#[cfg(not(test))]
pub mod program_id {

    use drv_models::new_types::version::Version;
    use solana_sdk::declare_id;

    declare_id!("DRVSpZ2YUYYKgZP8XtLhAGtT1zYSCKzeHfb4DgRnrgqD");
    pub const VERSION: Version = Version(1);
}

#[cfg(test)]
pub mod program_id {
    use drv_models::new_types::version::Version;
    use solana_sdk::declare_id;

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
/// Referral system on swap. Any client can form a swap transaction with their parameters and receive a part of fees from swap execution
pub struct SwapReferralParams {
    fee_rate_factor: f64,
    client_mint_token_acc: Pubkey,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstructionBuilderParams {
    ata_init: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamsWrapper {
    swap_ref_params: Option<SwapReferralParams>,
    instruction_builder_params: InstructionBuilderParams,
}

#[derive(Debug, Clone, PartialEq, Zeroable)]
pub struct CandleParams {
    count: u32,
    buffer_len: u32,
    capacity: u32,
}

impl CandleParams {
    pub fn new<const TAG: u32>(account: &Account) -> Self {
        let header: &CandlesAccountHeader<0> =
            from_bytes(&account.data[..std::mem::size_of::<CandlesAccountHeader<0>>()]);

        let buffer_len = (account.data.len() - std::mem::size_of::<CandlesAccountHeader<0>>())
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
    swap_referral_params: Option<SwapReferralParams>,
    instruction_builder_params: InstructionBuilderParams,
    candles: Option<Candles>,
    a_program_id: Pubkey,
    b_program_id: Pubkey,
}

pub trait AccountsHolder {
    fn from_account<T: Pod>(&self, account_addr: &Pubkey) -> Result<T>;
}

impl AccountsHolder for AccountMap {
    fn from_account<T: Pod>(&self, account_addr: &Pubkey) -> Result<T> {
        let acc = self
            .get(account_addr)
            .ok_or(anyhow!("Invalid provided address {}", account_addr))?;

        Ok(*bytemuck::from_bytes(
            &acc.data.as_slice()[0..std::mem::size_of::<T>()],
        ))
    }
}

impl Amm for Deriverse {
    fn from_keyed_account(
        keyed_account: &jupiter_amm_interface::KeyedAccount,
        _: &jupiter_amm_interface::AmmContext,
    ) -> Result<Self>
    where
        Self: Sized,
    {
        let instr_header = Box::new(*bytemuck::from_bytes::<InstrAccountHeader>(
            &keyed_account.account.data.as_slice()[..std::mem::size_of::<InstrAccountHeader>()],
        ));

        let mut accounts_ctx = ContextAccounts::build(instr_header.as_ref());

        let params: ParamsWrapper = if let Some(ref params) = keyed_account.params {
            from_value(params.clone())?
        } else {
            bail!("Need params were not provided in KeydAccount");
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
            swap_referral_params: params.swap_ref_params,
            instruction_builder_params: params.instruction_builder_params,
            candles: None,
        })
    }

    fn label(&self) -> String {
        "Deriverse".to_string()
    }

    fn program_id(&self) -> Pubkey {
        program_id::id()
    }

    fn key(&self) -> Pubkey {
        self.accounts_ctx.instr_header
    }

    fn has_dynamic_accounts(&self) -> bool {
        true
    }

    fn get_accounts_len(&self) -> usize {
        SwapInstruction::MIN_ACCOUNTS
            + (self.a_program_id != self.b_program_id) as usize
            + self.swap_referral_params.is_some() as usize
            + self.instruction_builder_params.ata_init as usize * 2
    }

    fn get_reserve_mints(&self) -> Vec<Pubkey> {
        vec![self.a_token_state.address, self.b_token_state.address]
    }

    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        self.accounts_ctx.clone().into()
    }

    fn update(&mut self, account_map: &jupiter_amm_interface::AccountMap) -> Result<()> {
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

        *self.instr_header = account_map.from_account(instr_header)?;
        self.a_token_state = account_map.from_account(a_token_state_acc)?;
        self.b_token_state = account_map.from_account(b_token_state_acc)?;

        self.fee_rate_factor = account_map
            .from_account::<CommunityAccountHeader>(community_acc)?
            .spot_fee_rate as f64
            * FEE_RATE_STEP;

        let lines_acc = account_map
            .get(lines)
            .ok_or(anyhow!("Invalid lines account"))?;

        let ask_orders_acc = account_map
            .get(ask_orders)
            .ok_or(anyhow!("Invalid ask order account"))?;

        let bid_orders_acc = account_map
            .get(bid_orders)
            .ok_or(anyhow!("Invalid bid order account"))?;

        self.order_book = OrderBook::new(
            &self.instr_header,
            lines_acc,
            bid_orders_acc,
            ask_orders_acc,
        );
        self.amm = DeriverseAmm::new(&self.instr_header);

        let a_mint_acc = account_map
            .get(a_mint)
            .ok_or(anyhow!("Invalid provided address {}", a_mint))?;
        self.a_program_id = a_mint_acc.owner;

        let b_mint_acc = account_map
            .get(b_mint)
            .ok_or(anyhow!("Invalid provided address {}", b_mint))?;
        self.b_program_id = b_mint_acc.owner;

        if let Some((candle_1m, candle_15m, candle_day)) = candles {
            let candle_1m_acc = account_map
                .get(candle_1m)
                .ok_or(anyhow!("Invalid provided address {}", candle_1m))?;
            let candle_15m_acc = account_map
                .get(candle_15m)
                .ok_or(anyhow!("Invalid provided address {}", candle_15m))?;
            let candle_day_acc = account_map
                .get(candle_day)
                .ok_or(anyhow!("Invalid provided address {}", candle_day))?;

            self.candles = Some(Candles {
                candle_1m: CandleParams::new::<SPOT_1M_CANDLES>(&candle_1m_acc),
                candle_15m: CandleParams::new::<SPOT_15M_CANDLES>(&candle_15m_acc),
                candle_day: CandleParams::new::<SPOT_DAY_CANDLES>(&candle_day_acc),
            })
        }

        Ok(())
    }

    fn quote(
        &self,
        quote_params: &jupiter_amm_interface::QuoteParams,
    ) -> Result<jupiter_amm_interface::Quote> {
        let Deriverse {
            instr_header,
            b_token_state,
            order_book,
            amm,
            fee_rate_factor,
            swap_referral_params,
            ..
        } = self;

        let mut amm = amm.clone();

        let buy = b_token_state.address == quote_params.input_mint;

        let px = instr_header.market_px();
        let price = {
            let max_diff = px >> 3;

            if buy {
                px + max_diff
            } else {
                px - max_diff
            }
        };

        let fee_rate = instr_header.day_volatility * fee_rate_factor;

        let mut client_tokens: i64 = 0;
        let mut client_mints: i64 = 0;

        if buy && (price > px || order_book.cross(price, OrderSide::Ask)) {
            let input_sum = (quote_params.amount as f64
                / (1.0
                    + fee_rate
                    + swap_referral_params
                        .as_ref()
                        .map(|params| params.fee_rate_factor)
                        .unwrap_or(0.0))) as i64;
            let mut remaining_sum = input_sum;
            let mut qty = 0_i64;
            let mut total_fees = 0_i64;
            let mut amm_px;
            let traded_qty;
            let traded_mints;
            let mut next_amm_px;

            let mut lines = order_book.iter_asks();

            loop {
                let line = lines.next();

                amm_px = amm.get_reversed_amm_px(remaining_sum)?;

                if line.is_none() {
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
                        .ok_or(anyhow!("Arithmetic Overflow"))?;
                    amm.a_tokens = amm
                        .a_tokens
                        .checked_sub(traded_qty)
                        .ok_or(anyhow!("Arithmetic Overflow"))?;
                    amm.b_tokens = amm
                        .b_tokens
                        .checked_add(traded_mints)
                        .ok_or(anyhow!("Arithmetic Overflow"))?;

                    total_fees = total_fees
                        .checked_add((traded_mints as f64 * fee_rate) as i64)
                        .ok_or(anyhow!("Arithmetic Overflow"))?;

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
                                .ok_or(anyhow!("Arithmetic Overflow"))?;

                            amm.a_tokens = amm
                                .a_tokens
                                .checked_sub(traded_qty)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
                            amm.b_tokens = amm
                                .b_tokens
                                .checked_add(traded_mints)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
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
                                .ok_or(anyhow!("Arithmetic Overflow"))?;

                            amm.a_tokens = amm
                                .a_tokens
                                .checked_sub(traded_qty)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
                            amm.b_tokens = amm
                                .b_tokens
                                .checked_add(traded_mints)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
                        } else {
                            traded_qty = amm.get_amm_qty(line.price, OrderSide::Ask)?;
                            traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Ask)?;
                            if traded_qty != 0 && traded_mints != 0 {
                                remaining_sum -= traded_mints;
                                qty = qty
                                    .checked_add(traded_qty)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;

                                amm.a_tokens = amm
                                    .a_tokens
                                    .checked_sub(traded_qty)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;
                                amm.b_tokens = amm
                                    .b_tokens
                                    .checked_add(traded_mints)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;
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
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;

                                total_fees = total_fees
                                    .checked_add(traded_fees)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;

                                remaining_sum -= traded_sum;
                            }
                        }
                        if traded_qty != 0 && traded_mints != 0 {
                            total_fees = total_fees
                                .checked_add((traded_mints as f64 * fee_rate) as i64)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
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
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        total_fees = total_fees
                            .checked_add(traded_fees)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

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
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        amm.a_tokens = amm
                            .a_tokens
                            .checked_sub(traded_qty)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;
                        amm.b_tokens = amm
                            .b_tokens
                            .checked_add(traded_mints)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        total_fees = total_fees
                            .checked_add((traded_mints as f64 * fee_rate) as i64)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;
                    }

                    if DeriverseAmm::cover_line(amm_px, price, line.price, OrderSide::Ask) {
                        let init_qty =
                            (remaining_sum as f64 * self.amm.df / line.price as f64) as i64;

                        let (traded_qty, traded_sum, traded_fees) =
                            self.order_book
                                .fill(&line, init_qty, fee_rate, OrderSide::Ask)?;

                        qty = qty
                            .checked_add(traded_qty)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        total_fees = total_fees
                            .checked_add(traded_fees)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        remaining_sum -= traded_sum;
                    }

                    break;
                }
            }

            client_tokens += qty;
            let traded_sum = input_sum - remaining_sum;
            client_mints -= traded_sum;

            let additional_fees = if let Some(params) = swap_referral_params {
                (traded_sum as f64 * params.fee_rate_factor) as i64
            } else {
                0
            };

            client_mints -= total_fees + additional_fees;
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
                        .ok_or(anyhow!("Arithmetic Overflow"))?;
                    amm.a_tokens = amm
                        .a_tokens
                        .checked_add(traded_qty)
                        .ok_or(anyhow!("Arithmetic Overflow"))?;
                    amm.b_tokens = amm
                        .b_tokens
                        .checked_sub(traded_mints)
                        .ok_or(anyhow!("Arithmetic Overflow"))?;

                    total_fees = total_fees
                        .checked_add((traded_mints as f64 * fee_rate) as i64)
                        .ok_or(anyhow!("Arithmetic Overflow"))?;
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
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
                            amm.a_tokens = amm
                                .a_tokens
                                .checked_add(traded_qty)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
                            amm.b_tokens = amm
                                .b_tokens
                                .checked_sub(traded_mints)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
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
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
                            amm.a_tokens = amm
                                .a_tokens
                                .checked_add(traded_qty)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
                            amm.b_tokens = amm
                                .b_tokens
                                .checked_sub(traded_mints)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
                        } else {
                            traded_qty = amm.get_amm_qty(line.price, OrderSide::Bid)?;
                            traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Bid)?;

                            if traded_qty != 0 && traded_mints != 0 {
                                remaining_qty -= traded_qty;
                                sum = sum
                                    .checked_add(traded_mints)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;
                                amm.a_tokens = amm
                                    .a_tokens
                                    .checked_add(traded_qty)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;
                                amm.b_tokens = amm
                                    .b_tokens
                                    .checked_sub(traded_mints)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;
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
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;
                                sum = sum
                                    .checked_add(traded_sum)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;

                                remaining_qty -= traded_qty;
                            }
                        }

                        if traded_mints != 0 && traded_qty != 0 {
                            total_fees = total_fees
                                .checked_add((traded_mints as f64 * fee_rate) as i64)
                                .ok_or(anyhow!("Arithmetic Overflow"))?;
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
                            .ok_or(anyhow!("Arithmetic Overflow"))?;
                        sum = sum
                            .checked_add(traded_sum)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

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
                            .ok_or(anyhow!("Arithmetic Overflow"))?;
                        amm.a_tokens = amm
                            .a_tokens
                            .checked_add(traded_qty)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;
                        amm.b_tokens = amm
                            .b_tokens
                            .checked_sub(traded_mints)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        total_fees = total_fees
                            .checked_add((traded_mints as f64 * fee_rate) as i64)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;
                    }

                    if DeriverseAmm::cover_line(next_amm_px, price, line.price, OrderSide::Bid) {
                        let (traded_qty, traded_sum, traded_fees) =
                            self.order_book
                                .fill(&line, remaining_qty, fee_rate, OrderSide::Bid)?;

                        total_fees = total_fees
                            .checked_add(traded_fees)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;
                        sum = sum
                            .checked_add(traded_sum)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        remaining_qty -= traded_qty;
                    }
                }

                break;
            }
            client_tokens -= quote_params.amount as i64 - remaining_qty;
            client_mints += sum;

            let additional_fees = if let Some(params) = swap_referral_params {
                (sum as f64 * params.fee_rate_factor) as i64
            } else {
                0
            };

            client_mints -= total_fees + additional_fees;
        }

        if client_tokens == 0 || client_mints == 0 {
            bail!("Swap failed")
        }

        if buy {
            Ok(Quote {
                in_amount: (-1 * client_mints) as u64,
                out_amount: client_tokens as u64,
            })
        } else {
            Ok(Quote {
                in_amount: (-1 * client_tokens) as u64,
                out_amount: client_mints as u64,
            })
        }
    }

    fn get_swap_and_account_metas(
        &self,
        swap_params: &SwapParams,
    ) -> Result<jupiter_amm_interface::SwapAndAccountMetas> {
        let Deriverse {
            instr_header,
            accounts_ctx,
            a_token_state,
            b_token_state,
            a_program_id,
            b_program_id,
            swap_referral_params,
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
                bail!("Invalid destination mint is provided");
            }
            (Side::Bid, destination_token_account, source_token_account)
        } else if b_token_state.address == *destination_mint {
            if a_token_state.address != *source_mint {
                bail!("Invalid source mint is provided");
            }
            (Side::Ask, source_token_account, destination_token_account)
        } else {
            bail!(
                "None of source mint and destination mint matches crcny mint {}",
                b_token_state.address
            );
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

        if let Some(params) = swap_referral_params {
            account_metas.extend_from_slice(&[AccountMeta {
                pubkey: params.client_mint_token_acc,
                is_signer: false,
                is_writable: true,
            }]);
        }

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
                swap_fee_rate: swap_referral_params
                    .clone()
                    .map(|params| params.fee_rate_factor)
                    .unwrap_or(0.0),
                side,
                instr_id: *instr_header.instr_id,
            },
            account_metas,
        })
    }

    fn clone_amm(&self) -> Box<dyn Amm + Send + Sync>
    where
        Self: Amm,
    {
        Box::new(self.clone())
    }

    fn is_active(&self) -> bool {
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

        market_requirements && candles_requirements
    }
}

fn from_swap(swap: Swap, in_amount: u64) -> SwapData {
    if let Swap::Deriverse {
        side,
        instr_id,
        swap_fee_rate,
    } = swap
    {
        SwapData {
            tag: SwapInstruction::INSTRUCTION_NUMBER,
            input_crncy: (side == Side::Bid) as u8,
            instr_id: InstrId(instr_id),
            price: 0,
            amount: in_amount as i64,
            ref_fee_rate: swap_fee_rate,
            ..SwapData::zeroed()
        }
    } else {
        panic!("Incorrect swap")
    }
}
