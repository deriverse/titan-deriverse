use anyhow::{Result, anyhow, bail};
use bytemuck::{Pod, Zeroable};
use drv_models::{
    constants::{
        instructions::{DrvInstruction, SwapInstruction},
        voting::FEE_RATE_STEP,
    },
    instruction_data::SwapData,
    new_types::instrument::InstrId,
    state::{
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
        },
    },
};

use jupiter_amm_interface::{
    AccountMap, Amm, Quote, Side, Swap, SwapAndAccountMetas, SwapMode, SwapParams,
};
use rust_decimal::Decimal;
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

use crate::{
    amm::DeriverseAmm, helper::Helper, instrument::OffChainInstrAccountHeader,
    lines_linked_list::OrderBook,
};

pub mod amm;
pub mod helper;
pub mod instrument;
pub mod lines_linked_list;

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
    community_acc: Pubkey,
    a_mint: Pubkey,
    b_mint: Pubkey,
}

impl From<ContextAccounts> for Vec<Pubkey> {
    fn from(value: ContextAccounts) -> Self {
        vec![
            value.instr_header,
            value.a_token_state_acc,
            value.b_token_state_acc,
            value.community_acc,
            value.lines,
            value.a_mint,
            value.b_mint,
        ]
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
            lines: Pubkey::new_spot_acc(
                SPOT_LINES,
                instr_header.asset_token_id,
                instr_header.crncy_token_id,
            ),
            community_acc: Pubkey::new_acc(COMMUNITY),
            a_mint: instr_header.asset_mint,
            b_mint: instr_header.crncy_mint,
        }
    }
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

        let accounts_ctx = ContextAccounts::build(instr_header.as_ref());

        Ok(Deriverse {
            instr_header,
            accounts_ctx,
            a_token_state: TokenState::zeroed(),
            b_token_state: TokenState::zeroed(),
            order_book: OrderBook::default(),
            amm: DeriverseAmm::default(),
            fee_rate_factor: 0.0,
            a_program_id: solana_sdk::system_program::id(),
            b_program_id: solana_sdk::system_program::id(),
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

    fn get_accounts_len(&self) -> usize {
        SwapInstruction::MIN_ACCOUNTS
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

        self.order_book = OrderBook::new(&self.instr_header, lines_acc);
        self.amm = DeriverseAmm::new(&self.instr_header);

        let a_mint_acc = account_map
            .get(a_mint)
            .ok_or(anyhow!("Invalid provided address {}", a_mint))?;
        self.a_program_id = a_mint_acc.owner;

        let b_mint_acc = account_map
            .get(b_mint)
            .ok_or(anyhow!("Invalid provided address {}", b_mint))?;
        self.b_program_id = b_mint_acc.owner;

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
            ..
        } = self;

        let mut amm = amm.clone();

        // reversed swap
        if quote_params.swap_mode == SwapMode::ExactOut {
            bail!("Exact out is not supported")
        }

        let buy = b_token_state.address == quote_params.input_mint;

        let px = instr_header.market_px();
        let price = {
            let max_diff = px >> 3;

            if buy { px + max_diff } else { px - max_diff }
        };

        let fee_rate = instr_header.day_volatility * fee_rate_factor;

        let mut client_tokens: i64 = 0;
        let mut client_mints: i64 = 0;
        let mut fees_amount: i64 = 0;

        if buy && (price > px || order_book.cross(price, OrderSide::Ask)) {
            let input_sum = (quote_params.amount as f64 / (1.0 + fee_rate)) as i64;
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
                    let line_sum = amm.trade_sum(line.qty, line.price)?;

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
                                let fill_qty =
                                    (remaining_sum as f64 * amm.df / line.price as f64) as i64;

                                qty = qty
                                    .checked_add(fill_qty)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;
                                total_fees = total_fees
                                    .checked_add((remaining_sum as f64 * fee_rate) as i64)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;

                                remaining_sum = 0;
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
                        qty = qty
                            .checked_add(line.qty)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        total_fees = total_fees
                            .checked_add((line_sum as f64 * fee_rate) as i64)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        remaining_sum -= line_sum;
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
                        qty = qty
                            .checked_add(line.qty)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        total_fees = total_fees
                            .checked_add((line_sum as f64 * fee_rate) as i64)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        remaining_sum -= line_sum;
                    }

                    break;
                }
            }

            client_tokens += qty;
            client_mints -= quote_params.amount as i64 - remaining_sum;

            client_mints -= total_fees;
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
                                // fill
                                let fill_sum = amm.trade_sum(remaining_qty, line.price)?;
                                total_fees = total_fees
                                    .checked_add((fill_sum as f64 * fee_rate) as i64)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;
                                sum = sum
                                    .checked_add(fill_sum)
                                    .ok_or(anyhow!("Arithmetic Overflow"))?;

                                remaining_qty = 0;
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
                        let fill_sum = amm.trade_sum(line.qty, line.price)?;

                        total_fees = total_fees
                            .checked_add((fill_sum as f64 * fee_rate) as i64)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        remaining_qty -= line.qty;
                        sum = sum
                            .checked_add(fill_sum)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

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
                        let fill_sum = amm.trade_sum(line.qty, line.price)?;

                        total_fees = total_fees
                            .checked_add((fill_sum as f64 * fee_rate) as i64)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;

                        remaining_qty -= line.qty;
                        sum = sum
                            .checked_add(fill_sum)
                            .ok_or(anyhow!("Arithmetic Overflow"))?;
                    }
                }

                break;
            }
            client_tokens -= quote_params.amount as i64 - remaining_qty;
            client_mints += sum;

            client_mints -= total_fees;
            fees_amount = total_fees;
        }

        if client_tokens == 0 || client_mints == 0 {
            bail!("Swap failed")
        }

        if buy {
            Ok(Quote {
                in_amount: (-1 * client_mints) as u64,
                out_amount: client_tokens as u64,
                fee_amount: fees_amount as u64,
                fee_mint: b_token_state.address,
                fee_pct: Decimal::from(fees_amount) / Decimal::from(-1 * client_mints),
            })
        } else {
            Ok(Quote {
                in_amount: (-1 * client_tokens) as u64,
                out_amount: client_mints as u64,
                fee_amount: fees_amount as u64,
                fee_mint: b_token_state.address,
                fee_pct: Decimal::from(fees_amount) / Decimal::from(client_mints),
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

        let account_metas = vec![
            AccountMeta {
                pubkey: *token_transfer_authority,
                is_signer: true,
                is_writable: false,
            },
            AccountMeta {
                pubkey: root,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: self.accounts_ctx.instr_header,
                is_signer: false,
                is_writable: true,
            },
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
                    SPOT_ASKS_TREE,
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
            AccountMeta {
                pubkey: Pubkey::new_spot_acc(
                    SPOT_ASK_ORDERS,
                    instr_header.asset_token_id,
                    instr_header.crncy_token_id,
                ),
                is_signer: false,
                is_writable: true,
            },
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
                pubkey: accounts_ctx.a_token_state_acc,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: accounts_ctx.b_token_state_acc,
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
            AccountMeta {
                pubkey: Pubkey::get_drv_auth(),
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: solana_sdk::system_program::id(),
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *a_program_id,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: *b_program_id,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: spl_associated_token_account::id(),
                is_signer: false,
                is_writable: false,
            },
        ];

        Ok(SwapAndAccountMetas {
            swap: Swap::Deriverse {
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
        self.order_book.total_lines_count != 0 && self.instr_header.ps != 0
    }
}

fn from_swap(swap: Swap, in_amount: u64) -> SwapData {
    if let Swap::Deriverse { side, instr_id } = swap {
        SwapData {
            tag: 26,
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
