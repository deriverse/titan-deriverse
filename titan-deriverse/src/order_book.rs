use crate::Result;
use bytemuck::cast_slice;
use drv_models::{
    constants::nulls::NULL_ORDER,
    state::{
        instrument::InstrAccountHeader,
        spots::spot_account_header::SPOT_TRADE_ACCOUNT_HEADER_SIZE,
        types::{CappedI64, OrderSide, PxOrders},
    },
};
use solana_account::{Account, ReadableAccount};

use crate::{
    helper::CappedNumber,
    lines_linked_list::{Lines, LinesIter, LinesSugar},
    orders_linked_list::{Orders, OrdersSugar},
};

#[derive(Clone, Default, Debug, PartialEq)]
pub struct OrderBook {
    pub lines: Lines,
    pub bid_orders: Orders,
    pub ask_orders: Orders,
    pub bid_begin_line: u32,
    pub ask_begin_line: u32,
    pub ask_lines_count: usize,
    pub bid_line_count: usize,
    pub rdf: f64,
}

impl OrderBook {
    pub fn new(
        instr_header: &InstrAccountHeader,
        lines_acc: impl ReadableAccount,
        bid_orders: impl ReadableAccount,
        ask_orders: impl ReadableAccount,
    ) -> Self {
        let lines = if lines_acc.data().len() <= SPOT_TRADE_ACCOUNT_HEADER_SIZE {
            vec![]
        } else {
            Lines::new_lines(cast_slice(
                &lines_acc.data()[SPOT_TRADE_ACCOUNT_HEADER_SIZE..],
            ))
        };

        let bid_orders = if bid_orders.data().len() <= SPOT_TRADE_ACCOUNT_HEADER_SIZE {
            vec![]
        } else {
            Orders::new_orders(cast_slice(
                &bid_orders.data()[SPOT_TRADE_ACCOUNT_HEADER_SIZE..],
            ))
        };

        let ask_orders = if ask_orders.data().len() <= SPOT_TRADE_ACCOUNT_HEADER_SIZE {
            vec![]
        } else {
            Orders::new_orders(cast_slice(
                &ask_orders.data()[SPOT_TRADE_ACCOUNT_HEADER_SIZE..],
            ))
        };

        OrderBook {
            bid_begin_line: instr_header.bid_lines_begin,
            ask_begin_line: instr_header.ask_lines_begin,
            ask_lines_count: instr_header.ask_lines_count as usize,
            bid_line_count: instr_header.bid_lines_count as usize,
            lines,
            bid_orders,
            ask_orders,
            rdf: 1f64 / instr_header.dec_factor as f64,
        }
    }

    pub fn iter_bids<'a>(&'a self) -> LinesIter<'a> {
        self.lines
            .iter_from(self.bid_begin_line, self.bid_line_count)
    }

    pub fn iter_asks<'a>(&'a self) -> LinesIter<'a> {
        self.lines
            .iter_from(self.ask_begin_line, self.ask_lines_count)
    }

    fn begin_index(&self, side: OrderSide) -> usize {
        match side {
            OrderSide::Bid => self.bid_begin_line as usize,
            OrderSide::Ask => self.ask_begin_line as usize,
        }
    }

    pub fn begin(&self, side: OrderSide) -> Option<&PxOrders> {
        let idx = self.begin_index(side);

        let line = self.lines.get(idx);
        if let Some(line) = line {
            if line.sref == NULL_ORDER {
                return None;
            }
        }

        line
    }

    pub fn cross(&self, price: i64, side: OrderSide) -> bool {
        let begin = self.begin(side);
        match side {
            OrderSide::Bid => begin.is_some_and(|line| price <= line.price),
            OrderSide::Ask => begin.is_some_and(|line| price >= line.price),
        }
    }

    fn trade_sum<T: Into<i64>, U: Into<i64>>(&self, a: T, b: U) -> Result<CappedI64> {
        let sum = (a.into() as f64 * b.into() as f64) * self.rdf;

        if sum.is_sign_negative() || sum.is_nan() {
            return Err("Arithmetic overflow".into());
        }

        CappedI64::new_checked(sum as i64)
    }

    fn trade_qty<T: Into<i64>>(&self, sum: T, price: i64) -> Result<CappedI64> {
        let qty = (sum.into() as f64 / self.rdf) / price as f64;

        if qty.is_sign_negative() || qty.is_nan() {
            return Err("Arithmetic overflow".into());
        }

        CappedI64::new_checked(qty as i64)
    }

    pub fn line_sum<T: Into<i64> + Copy>(
        &self,
        line: &PxOrders,
        side: OrderSide,
        remaining_sum: T,
    ) -> Result<CappedI64> {
        let orders = match side {
            OrderSide::Bid => &self.bid_orders,
            OrderSide::Ask => &self.ask_orders,
        };

        let mut orders = orders.iter_from(line.begin);
        let mut sum = CappedI64::new(0);

        while let Some((_, order)) = orders.next() {
            sum = sum.checked_add_capped(order.sum)?;

            if sum > remaining_sum.into() {
                break;
            }
        }

        Ok(sum)
    }

    pub fn fill(
        &self,
        line: &PxOrders,
        mut remaining_qty: CappedI64,
        fee_rate: f64,
        side: OrderSide,
    ) -> Result<(CappedI64, CappedI64, CappedI64)> {
        let px = line.price;
        let orders = match side {
            OrderSide::Bid => &self.bid_orders,
            OrderSide::Ask => &self.ask_orders,
        };
        let mut orders = orders.iter_from(line.begin);

        let mut total_traded_qty = remaining_qty;
        let mut total_traded_sum = CappedI64::new(0);
        let mut total_fees = CappedI64::new(0);

        while let Some((_, order)) = orders.next()
            && remaining_qty > 0
        {
            let (traded_qty, traded_crncy) = if order.qty <= remaining_qty {
                (order.qty, order.sum)
            } else {
                (
                    remaining_qty,
                    self.trade_sum(remaining_qty, px)?.min(order.sum),
                )
            };

            remaining_qty = remaining_qty.sub(traded_qty);
            total_traded_sum = total_traded_sum.checked_add_capped(traded_crncy)?;
            total_fees =
                total_fees.checked_add_capped((traded_crncy.value as f64 * fee_rate) as i64)?;
        }

        total_traded_qty = total_traded_qty.sub(remaining_qty);

        Ok((total_traded_qty, total_traded_sum, total_fees))
    }

    pub fn reversed_fill(
        &self,
        line: &PxOrders,
        mut remaining_sum: CappedI64,
        fee_rate: f64,
        side: OrderSide,
    ) -> Result<(CappedI64, CappedI64, CappedI64)> {
        let px = line.price;
        let orders = match side {
            OrderSide::Bid => &self.bid_orders,
            OrderSide::Ask => &self.ask_orders,
        };
        let mut orders = orders.iter_from(line.begin);

        let mut total_traded_sum = remaining_sum;
        let mut total_traded_qty = CappedI64::new(0);
        let mut total_fees = CappedI64::new(0);

        while let Some((_, order)) = orders.next()
            && remaining_sum > 0
        {
            let (traded_qty, traded_crncy) = if order.sum <= remaining_sum {
                (order.qty, order.sum)
            } else {
                let potential_qty = self.trade_qty(remaining_sum, px)?;
                (potential_qty.min(order.qty), remaining_sum)
            };

            remaining_sum = remaining_sum.sub(traded_crncy);
            total_traded_qty = total_traded_qty.checked_add_capped(traded_qty)?;
            total_fees =
                total_fees.checked_add_capped((traded_crncy.value as f64 * fee_rate) as i64)?;
        }

        total_traded_sum = total_traded_sum.sub(remaining_sum);

        Ok((total_traded_qty, total_traded_sum, total_fees))
    }
}
