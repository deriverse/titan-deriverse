use async_trait::async_trait;
use bytemuck::Zeroable;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
pub mod amm;
pub mod errors;
pub mod lines_linked_list;
pub mod order_book;
pub mod orders_linked_list;
pub mod types;

use crate::{
    account_caching::AccountsCache,
    trading_venue::{
        FromAccount, QuoteRequest, QuoteResult, SwapType, TradingVenue,
        error::{ErrorInfo, TradingVenueError},
        protocol::PoolProtocol,
        token_info::TokenInfo,
        venue_creation::{ParsedInstruction, PoolCreation},
    },
    your_venue::{
        amm::DeriverseAmm,
        order_book::OrderBook,
        types::{
            OrderSide, Version,
            capped_i64::{CappedI64, CappedNumber},
            constants::{
                FEE_RATE_STEP,
                account_type::{
                    INSTR, SPOT_ASK_ORDERS, SPOT_ASKS_TREE, SPOT_BID_ORDERS, SPOT_BIDS_TREE,
                    SPOT_CLIENT_INFOS, SPOT_LINES,
                },
            },
            helper::{Helper, Side},
            instr_mask::{InstrFlag, SimpleInstrMask},
            instruction_constants::instruction_constants::{
                DrvInstruction, NewInstrumentInstruction, SwapInstruction,
            },
            instruction_data::SwapData,
            instrument::{InstrAccountHeader, OffChainInstrAccountHeader},
            token::TokenState,
        },
    },
};

pub const DRV_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("DRVSpZ2YUYYKgZP8XtLhAGtT1zYSCKzeHfb4DgRnrgqD");

pub const VERSION: Version = Version(1);

/// Detect every pool your venue created in a confirmed transaction.
///
/// Titan tracks new pools live by feeding the decompiled instructions of
/// confirmed transactions through this function; each returned
/// [`PoolCreation::pool`] is then built into a venue via
/// [`YourVenue::from_account`]. See [`crate::trading_venue::venue_creation`] for
/// the contract and `tests/venue_creation.rs` for the worked Raydium reference.
pub fn parse_pool_creations(instructions: &[ParsedInstruction]) -> Vec<PoolCreation> {
    const INSTRUMENT_INDEX: usize = 13;
    const ASSET_INDEX: usize = 6;
    const CRNCY_INDEX: usize = 12;

    instructions
        .iter()
        .filter(|ix| ix.program_id == DRV_PROGRAM_ID)
        .filter(|ix| ix.data[0] == NewInstrumentInstruction::INSTRUCTION_NUMBER)
        .filter_map(|ix| {
            let instr = *ix.accounts.get(INSTRUMENT_INDEX)?;
            let asset_mint = *ix.accounts.get(ASSET_INDEX)?;
            let crncy_mint = *ix.accounts.get(CRNCY_INDEX)?;

            Some(PoolCreation {
                // Instr id is a placeceholder
                protocol: PoolProtocol::Deriverse { instr_id: 0 },
                pool: instr,
                mints: vec![asset_mint, crncy_mint],
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
struct ContextAccounts {
    instr_header: Pubkey,
    a_token_state_acc: Pubkey,
    b_token_state_acc: Pubkey,
    lines: Pubkey,
    bid_orders: Pubkey,
    ask_orders: Pubkey,
    a_mint: Pubkey,
    b_mint: Pubkey,
}

impl From<ContextAccounts> for Vec<Pubkey> {
    fn from(value: ContextAccounts) -> Self {
        let vec = vec![
            value.instr_header,
            value.a_token_state_acc,
            value.b_token_state_acc,
            value.lines,
            value.bid_orders,
            value.ask_orders,
            value.a_mint,
            value.b_mint,
        ];

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
            a_mint: instr_header.asset_mint,
            b_mint: instr_header.crncy_mint,
        }
    }
}

/// Your venue's off-chain state. Add whatever the quote math needs.
#[derive(Clone)]
pub struct Deriverse {
    accounts_ctx: ContextAccounts,
    instr_header: Box<InstrAccountHeader>,
    a_token_state: TokenState,
    b_token_state: TokenState,
    order_book: OrderBook,
    amm: DeriverseAmm,
    fee_rate_factor: f64,
    // a, b
    token_infos: [TokenInfo; 2],
}

impl Deriverse {
    pub const TOKEN_A_INDEX: usize = 0;
    pub const TOKEN_B_INDEX: usize = 1;

    fn quote(&self, request: QuoteRequest) -> Result<QuoteResult, TradingVenueError> {
        let Deriverse {
            instr_header,
            b_token_state,
            order_book,
            amm,
            fee_rate_factor,
            ..
        } = self;

        if request.swap_type == SwapType::ExactOut {
            return Err(TradingVenueError::ExactOutNotSupported);
        }

        let mut amm = amm.clone();

        let buy = b_token_state.address == request.input_mint;

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

        let mut client_tokens = CappedI64::new(0);
        let mut client_mints = CappedI64::new(0);
        let swap_fees = CappedI64::new(0);

        let input_amount = CappedI64::new_checked(request.amount as i64)?;

        let mut last_line_px = if buy {
            order_book.best_ask().map(|line| line.price)
        } else {
            order_book.best_bid().map(|line| line.price)
        };

        if buy && (price > px || order_book.cross(price, OrderSide::Ask)) {
            let input_sum =
                CappedI64::new_checked((input_amount.value as f64 / (1.0 + fee_rate)) as i64)?;

            let estimated_fees = (input_amount.sub(input_sum).sub(swap_fees)).max(0.into());

            let mut remaining_sum = input_sum;
            let mut qty = CappedI64::new(0);
            let mut total_fees = CappedI64::new(0);
            let mut amm_px;
            let traded_qty;
            let traded_mints;
            let mut next_amm_px;

            let mut lines = order_book.iter_asks();
            let mut exhausted = false;

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
                    remaining_sum = remaining_sum.sub(traded_mints);

                    qty = qty.checked_add_capped(traded_qty)?;
                    amm.a_tokens = amm.a_tokens.checked_sub_capped(traded_qty)?;
                    amm.b_tokens = amm.b_tokens.checked_add_capped(traded_mints)?;

                    total_fees = total_fees
                        .checked_add_capped((traded_mints.value as f64 * fee_rate) as i64)?;

                    break;
                }

                if let Some((_, line)) = line {
                    last_line_px = Some(line.price);

                    let line_sum = order_book.line_sum(&line, OrderSide::Ask, remaining_sum)?;

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
                            remaining_sum = remaining_sum.sub(traded_mints);

                            qty = qty.checked_add_capped(traded_qty)?;

                            amm.a_tokens = amm.a_tokens.checked_sub_capped(traded_qty)?;
                            amm.b_tokens = amm.b_tokens.checked_add_capped(traded_mints)?;
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

                            remaining_sum = remaining_sum.sub(traded_mints);
                            qty = qty.checked_add_capped(traded_qty)?;

                            amm.a_tokens = amm.a_tokens.checked_sub_capped(traded_qty)?;
                            amm.b_tokens = amm.b_tokens.checked_add_capped(traded_mints)?;
                        } else {
                            traded_qty = amm.get_amm_qty(line.price, OrderSide::Ask)?;
                            traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Ask)?;
                            if traded_qty != 0 && traded_mints != 0 {
                                remaining_sum = remaining_sum.sub(traded_mints);
                                qty = qty.checked_add_capped(traded_qty)?;

                                amm.a_tokens = amm.a_tokens.checked_sub_capped(traded_qty)?;
                                amm.b_tokens = amm.b_tokens.checked_add_capped(traded_mints)?;
                            }
                            if remaining_sum > 0 {
                                let (traded_qty, _, traded_fees) = self.order_book.reversed_fill(
                                    &line,
                                    remaining_sum,
                                    fee_rate,
                                    OrderSide::Ask,
                                )?;

                                qty = qty.checked_add_capped(traded_qty)?;

                                total_fees = total_fees.checked_add_capped(traded_fees)?;
                                remaining_sum = CappedI64::new(0);
                            }
                        }
                        if traded_qty != 0 && traded_mints != 0 {
                            total_fees = total_fees.checked_add_capped(
                                (traded_mints.value as f64 * fee_rate) as i64,
                            )?;
                        }

                        break;
                    }

                    next_amm_px = amm.get_reversed_amm_px(remaining_sum.sub(line_sum))?;
                    if DeriverseAmm::cover_line(next_amm_px, price, line.price, OrderSide::Ask) {
                        //let init_qty =
                        //    (remaining_sum as f64 * self.amm.df / line.price as f64) as i64;

                        //let (traded_qty, traded_sum, traded_fees) =
                        //self.order_book
                        //    .fill(&line, init_qty, fee_rate, OrderSide::Ask)?;
                        let (traded_qty, traded_sum, traded_fees) = self.order_book.reversed_fill(
                            &line,
                            remaining_sum,
                            fee_rate,
                            OrderSide::Ask,
                        )?;

                        qty = qty.checked_add_capped(traded_qty)?;

                        total_fees = total_fees.checked_add_capped(traded_fees)?;

                        remaining_sum = remaining_sum.sub(traded_sum);
                        continue;
                    }

                    traded_mints = amm
                        .get_reversed_amm_sum(line.price.min(price))?
                        .min(remaining_sum);

                    traded_qty = amm.get_reversed_amm_qty(traded_mints)?;

                    if traded_qty != 0 && traded_mints != 0 {
                        remaining_sum = remaining_sum.sub(traded_mints);
                        qty = qty.checked_add_capped(traded_qty)?;

                        amm.a_tokens = amm.a_tokens.checked_sub_capped(traded_qty)?;
                        amm.b_tokens = amm.b_tokens.checked_add_capped(traded_mints)?;

                        total_fees = total_fees
                            .checked_add_capped((traded_mints.value as f64 * fee_rate) as i64)?;
                    }

                    if DeriverseAmm::cover_line(amm_px, price, line.price, OrderSide::Ask) {
                        let (traded_qty, _, traded_fees) = self.order_book.reversed_fill(
                            &line,
                            remaining_sum,
                            fee_rate,
                            OrderSide::Ask,
                        )?;

                        qty = qty.checked_add_capped(traded_qty)?;

                        total_fees = total_fees.checked_add_capped(traded_fees)?;

                        remaining_sum = CappedI64::new(0);
                    }

                    break;
                }
            }

            client_tokens = client_tokens.checked_add_capped(qty)?;

            if remaining_sum == 1 {
                if estimated_fees > 0 {
                    total_fees = estimated_fees.checked_add_capped(1)?;
                } else {
                    remaining_sum = CappedI64::new(0);
                }
            } else if remaining_sum == 0 {
                total_fees = estimated_fees;
            }

            let traded_sum = input_sum.sub(remaining_sum);

            let protocol_estimated_fees =
                CappedI64::new((traded_sum.value as f64 * fee_rate) as i64);
            total_fees = total_fees.max(protocol_estimated_fees);

            if remaining_sum > 1 {
                if exhausted && fee_rate > 0.0 {
                    total_fees = total_fees.checked_add_capped(1)?;
                }
            }

            client_mints =
                client_mints.checked_sub_capped(traded_sum.add(total_fees).add(swap_fees))?;
        } else if !buy && (price < px || order_book.cross(price, OrderSide::Bid)) {
            let mut remaining_qty = input_amount;
            let mut sum = CappedI64::new(0);
            let mut total_fees = CappedI64::new(0);
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

                    remaining_qty = remaining_qty.sub(traded_qty);
                    sum = sum.checked_add_capped(traded_mints)?;
                    amm.a_tokens = amm.a_tokens.checked_add_capped(traded_qty)?;
                    amm.b_tokens = amm.b_tokens.checked_sub_capped(traded_mints)?;

                    total_fees = total_fees
                        .checked_add_capped((traded_mints.value as f64 * fee_rate) as i64)?;

                    break;
                }

                if let Some((_, line)) = line {
                    last_line_px = Some(line.price);

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

                            remaining_qty = remaining_qty.sub(traded_qty);

                            sum = sum.checked_add_capped(traded_mints)?;
                            amm.a_tokens = amm.a_tokens.checked_add_capped(traded_qty)?;
                            amm.b_tokens = amm.b_tokens.checked_sub_capped(traded_mints)?;
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
                            remaining_qty = remaining_qty.sub(traded_qty);
                            sum = sum.checked_add_capped(traded_mints)?;
                            amm.a_tokens = amm.a_tokens.checked_add_capped(traded_qty)?;
                            amm.b_tokens = amm.b_tokens.checked_sub_capped(traded_mints)?;
                        } else {
                            traded_qty = amm.get_amm_qty(line.price, OrderSide::Bid)?;
                            traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Bid)?;

                            if traded_qty != 0 && traded_mints != 0 {
                                remaining_qty = remaining_qty.sub(traded_qty);

                                sum = sum.checked_add_capped(traded_mints)?;
                                amm.a_tokens = amm.a_tokens.checked_add_capped(traded_qty)?;
                                amm.b_tokens = amm.b_tokens.checked_sub_capped(traded_mints)?;
                            }

                            if remaining_qty > 0 {
                                let (traded_qty, traded_sum, traded_fees) = self.order_book.fill(
                                    &line,
                                    remaining_qty,
                                    fee_rate,
                                    OrderSide::Bid,
                                )?;

                                total_fees = total_fees.checked_add_capped(traded_fees)?;
                                sum = sum.checked_add_capped(traded_sum)?;

                                remaining_qty = remaining_qty.sub(traded_qty);
                            }
                        }

                        if traded_mints != 0 && traded_qty != 0 {
                            total_fees = total_fees.checked_add_capped(
                                (traded_mints.value as f64 * fee_rate) as i64,
                            )?;
                        }
                        break;
                    }

                    next_amm_px = amm.get_amm_px(remaining_qty.sub(line.qty), OrderSide::Bid)?;

                    if DeriverseAmm::cover_line(next_amm_px, price, line.price, OrderSide::Bid) {
                        let (traded_qty, traded_sum, traded_fees) =
                            self.order_book
                                .fill(&line, remaining_qty, fee_rate, OrderSide::Bid)?;

                        total_fees = total_fees.checked_add_capped(traded_fees)?;
                        sum = sum.checked_add_capped(traded_sum)?;

                        remaining_qty = remaining_qty.sub(traded_qty);

                        continue;
                    }

                    traded_qty = amm
                        .get_amm_qty(line.price.max(price), OrderSide::Bid)?
                        .min(remaining_qty);
                    traded_mints = amm.get_amm_sum(traded_qty, OrderSide::Bid)?;

                    if traded_qty != 0 && traded_mints != 0 {
                        remaining_qty = remaining_qty.sub(traded_qty);

                        sum = sum.checked_add_capped(traded_mints)?;
                        amm.a_tokens = amm.a_tokens.checked_add_capped(traded_qty)?;
                        amm.b_tokens = amm.b_tokens.checked_sub_capped(traded_mints)?;

                        total_fees = total_fees
                            .checked_add_capped((traded_mints.value as f64 * fee_rate) as i64)?;
                    }

                    if DeriverseAmm::cover_line(amm_px, price, line.price, OrderSide::Bid) {
                        let (traded_qty, traded_sum, traded_fees) =
                            self.order_book
                                .fill(&line, remaining_qty, fee_rate, OrderSide::Bid)?;

                        total_fees = total_fees.checked_add_capped(traded_fees)?;
                        sum = sum.checked_add_capped(traded_sum)?;

                        remaining_qty = remaining_qty.sub(traded_qty);
                    }
                }

                break;
            }

            client_tokens = client_tokens.checked_sub_capped(input_amount.sub(remaining_qty))?;
            client_mints = client_mints.checked_add_capped(sum)?;

            client_mints = client_mints.checked_sub_capped(total_fees.add(swap_fees))?;
        }

        // Deriverse explicitly forib 0 in_amount, zero input support is only available for quoting and will return `InvalidQuantity` on smart contracts side
        if request.amount == 0 {
            if client_tokens != 0 || client_mints != 0 {
                return Err(TradingVenueError::AmmMethodError(ErrorInfo::StaticStr(
                    "Zero amount trade should not exchange any tokens",
                )));
            }
        } else {
            if client_tokens == 0 || client_mints == 0 {
                return Err(TradingVenueError::AmmMethodError(ErrorInfo::StaticStr(
                    "Not enough liquidity",
                )));
            }
        }

        let marginal_px = if buy {
            let final_amm_px = amm.get_reversed_amm_px(0)?;

            if amm.k == 0 || amm.a_tokens == 0 {
                last_line_px.ok_or(TradingVenueError::AmmMethodError(ErrorInfo::StaticStr(
                    "Quote is called on inactive market",
                )))?
            } else {
                match last_line_px {
                    Some(lpx) => final_amm_px.max(lpx),
                    None => final_amm_px,
                }
            }
        } else {
            let final_amm_px = amm.get_amm_px(0i64, OrderSide::Bid)?;

            if amm.k == 0 || amm.a_tokens == 0 {
                last_line_px.ok_or(TradingVenueError::AmmMethodError(ErrorInfo::StaticStr(
                    "Quote is called on inactive market",
                )))?
            } else {
                match last_line_px {
                    Some(lpx) => final_amm_px.min(lpx),
                    None => final_amm_px,
                }
            }
        };

        if buy {
            Ok(QuoteResult {
                input_mint: request.input_mint,
                output_mint: request.output_mint,
                amount: (-client_mints.value) as u64,
                expected_output: client_tokens.value as u64,
                not_enough_liquidity: request.amount as f64 != (-client_mints.value) as f64,
                price: amm.df / (marginal_px as f64 * (1.0 + fee_rate)),
            })
        } else {
            Ok(QuoteResult {
                input_mint: request.input_mint,
                output_mint: request.output_mint,
                amount: (-client_tokens.value) as u64,
                expected_output: client_mints.value as u64,
                not_enough_liquidity: request.amount as f64 != (-client_tokens.value) as f64,
                price: marginal_px as f64 * amm.rdf * (1.0 - fee_rate),
            })
        }
    }
}

impl FromAccount for Deriverse {
    /// its impossible to get full token_infos of mints from pool account in deriverse. Those values get created and update in **update_state**
    fn from_account(_: &Pubkey, account: &Account) -> Result<Self, TradingVenueError> {
        let instr_header = Box::new(*bytemuck::from_bytes::<InstrAccountHeader>(
            &account.data.as_slice()[..std::mem::size_of::<InstrAccountHeader>()],
        ));

        let accounts_ctx = ContextAccounts::build(instr_header.as_ref());

        Ok(Self {
            accounts_ctx,
            instr_header,
            a_token_state: TokenState::zeroed(),
            b_token_state: TokenState::zeroed(),
            order_book: OrderBook::default(),
            amm: DeriverseAmm::default(),
            fee_rate_factor: 0.0,
            token_infos: [TokenInfo::default(), TokenInfo::default()],
        })
    }
}

#[async_trait]
impl TradingVenue for Deriverse {
    /// Additionally validate that no additinonal candles allocation is needed
    fn initialized(&self) -> bool {
        let suspended_instrument = self.instr_header.mask.get_flag(InstrFlag::Suspended);

        let market_requirements = self.order_book.ask_lines_count != 0
            || self.order_book.bid_line_count != 0
            || self.instr_header.ps != 0;

        let candles_requirements = !self
            .instr_header
            .mask
            .get_flag(InstrFlag::ExpandableCandles);
        market_requirements && candles_requirements && !suspended_instrument
    }

    fn program_id(&self) -> Pubkey {
        DRV_PROGRAM_ID
    }

    fn program_dependencies(&self) -> Vec<Pubkey> {
        vec![self.program_id()]
    }

    fn market_id(&self) -> Pubkey {
        self.accounts_ctx.instr_header
    }

    fn get_token_info(&self) -> &[TokenInfo] {
        &self.token_infos
    }

    fn protocol(&self) -> PoolProtocol {
        PoolProtocol::Deriverse {
            instr_id: self.instr_header.instr_id.0,
        }
    }

    fn get_required_pubkeys_for_update(&self) -> Result<Vec<Pubkey>, TradingVenueError> {
        Ok(self.accounts_ctx.clone().into())
    }

    async fn update_state(&mut self, cache: &dyn AccountsCache) -> Result<(), TradingVenueError> {
        let required_pubkeys_for_update = self.get_required_pubkeys_for_update()?;

        let accounts = cache
            .get_accounts(required_pubkeys_for_update.as_slice())
            .await?;

        let [
            instr_header,
            a_token_state,
            b_token_state,
            lines,
            bid_orders,
            ask_orders,
            a_mint,
            b_mint,
        ] = accounts
            .into_iter()
            .zip(required_pubkeys_for_update.iter())
            .map(|(account, pubkey)| {
                account.ok_or(TradingVenueError::NoAccountFound(ErrorInfo::Pubkey(
                    *pubkey,
                )))
            })
            .collect::<Result<Vec<Account>, _>>()?
            .try_into()
            .map_err(|_| TradingVenueError::FailedToFetchMultipleAccountData)?;

        self.instr_header = Box::new(
            *bytemuck::try_from_bytes(
                &instr_header.data.as_slice()[..std::mem::size_of::<InstrAccountHeader>()],
            )
            .map_err(|err| {
                TradingVenueError::DeserializationError(ErrorInfo::String(err.to_string()))
            })?,
        );

        self.a_token_state = *bytemuck::try_from_bytes(
            &a_token_state.data.as_slice()[..std::mem::size_of::<TokenState>()],
        )
        .map_err(|err| {
            TradingVenueError::DeserializationError(ErrorInfo::String(err.to_string()))
        })?;
        self.b_token_state = *bytemuck::try_from_bytes(
            &b_token_state.data.as_slice()[..std::mem::size_of::<TokenState>()],
        )
        .map_err(|err| {
            TradingVenueError::DeserializationError(ErrorInfo::String(err.to_string()))
        })?;

        self.fee_rate_factor = self.instr_header.spot_fee_rate as f64 * FEE_RATE_STEP;

        self.order_book = OrderBook::new(&self.instr_header, &lines, &bid_orders, &ask_orders);
        self.amm = DeriverseAmm::new(&self.instr_header);

        self.token_infos[Deriverse::TOKEN_A_INDEX] = TokenInfo {
            pubkey: self.accounts_ctx.a_mint,
            decimals: self.a_token_state.mask.decimals() as i32,
            is_token_2022: a_mint.owner == spl_token_2022::ID,
            transfer_fee: None,
            maximum_fee: None,
        };

        self.token_infos[Deriverse::TOKEN_B_INDEX] = TokenInfo {
            pubkey: self.accounts_ctx.b_mint,
            decimals: self.b_token_state.mask.decimals() as i32,
            is_token_2022: b_mint.owner == spl_token_2022::ID,
            transfer_fee: None,
            maximum_fee: None,
        };

        Ok(())
    }

    fn quote(&self, request: QuoteRequest) -> Result<QuoteResult, TradingVenueError> {
        Deriverse::quote(self, request)
    }

    fn generate_swap_instruction(
        &self,
        request: QuoteRequest,
        user: Pubkey,
    ) -> Result<Instruction, TradingVenueError> {
        let Deriverse {
            instr_header,
            accounts_ctx,
            a_token_state,
            b_token_state,

            token_infos,
            ..
        } = self;

        let QuoteRequest {
            input_mint,
            output_mint,
            amount,
            swap_type,
        } = request;

        if swap_type == SwapType::ExactOut {
            return Err(TradingVenueError::ExactOutNotSupported);
        }

        let a_account = token_infos[Deriverse::TOKEN_A_INDEX].get_associated_token_address(&user);
        let b_account = token_infos[Deriverse::TOKEN_B_INDEX].get_associated_token_address(&user);

        let a_program_id = token_infos[Deriverse::TOKEN_A_INDEX].get_token_program();
        let b_program_id = token_infos[Deriverse::TOKEN_B_INDEX].get_token_program();

        let side = if b_token_state.address == input_mint {
            if a_token_state.address != output_mint {
                return Err(TradingVenueError::InvalidMint(ErrorInfo::String(format!(
                    "Invalid Output Mint was provided {}",
                    output_mint.to_string()
                ))));
            }
            Side::Bid
        } else if b_token_state.address == output_mint {
            if a_token_state.address != input_mint {
                return Err(TradingVenueError::InvalidMint(ErrorInfo::String(format!(
                    "Invalid Input Mint was provided {}",
                    input_mint.to_string()
                ))));
            }
            Side::Ask
        } else {
            return Err(TradingVenueError::InvalidMint(ErrorInfo::String(format!(
                "Invalid Inputs Mints were provided {}, {}",
                input_mint.to_string(),
                output_mint.to_string()
            ))));
        };
        let mut account_metas = vec![
            AccountMeta {
                pubkey: user,
                is_signer: true,
                is_writable: true,
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
                pubkey: accounts_ctx.instr_header,
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
                pubkey: a_account,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: b_account,
                is_signer: false,
                is_writable: true,
            },
        ]);

        account_metas.push(AccountMeta {
            pubkey: a_program_id,
            is_signer: false,
            is_writable: false,
        });

        if b_program_id != a_program_id {
            account_metas.push(AccountMeta {
                pubkey: b_program_id,
                is_signer: false,
                is_writable: false,
            });
        }

        // Currently there is not enough information to identify if ata is init to create it if needed
        // if ata_init {
        //     account_metas.push(AccountMeta {
        //         pubkey: solana_program::system_program::id(),
        //         is_signer: false,
        //         is_writable: false,
        //     });
        //     account_metas.push(AccountMeta {
        //         pubkey: spl_associated_token_account::id(),
        //         is_signer: false,
        //         is_writable: false,
        //     });
        // }

        Ok(Instruction::new_with_bytes(
            DRV_PROGRAM_ID,
            bytemuck::bytes_of(&SwapData {
                tag: SwapInstruction::INSTRUCTION_NUMBER,
                input_crncy: (side == Side::Bid) as u8,
                instr_id: instr_header.instr_id,
                // limit pirce, 0 => market order
                price: 0,
                amount: CappedI64::new(amount as i64),
                ..Zeroable::zeroed()
            }),
            account_metas,
        ))
    }
}
