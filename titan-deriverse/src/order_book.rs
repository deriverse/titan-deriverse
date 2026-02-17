use anyhow::bail;
use bytemuck::cast_slice;
use drv_models::{
    constants::{nulls::NULL_ORDER, trading_limitations::MAX_SUM},
    state::{
        instrument::InstrAccountHeader,
        spots::spot_account_header::SPOT_TRADE_ACCOUNT_HEADER_SIZE,
        types::{OrderSide, PxOrders},
    },
};
use solana_sdk::account::Account;

use crate::{
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
    pub total_lines_count: usize,
    pub rdf: f64,
}

impl OrderBook {
    pub fn new(
        instr_header: &InstrAccountHeader,
        lines_acc: &Account,
        bid_orders: &Account,
        ask_orders: &Account,
    ) -> Self {
        let lines = if lines_acc.data.len() <= SPOT_TRADE_ACCOUNT_HEADER_SIZE {
            vec![]
        } else {
            Lines::new_lines(cast_slice(
                &lines_acc.data.as_slice()[SPOT_TRADE_ACCOUNT_HEADER_SIZE..],
            ))
        };

        let bid_orders = if bid_orders.data.len() <= SPOT_TRADE_ACCOUNT_HEADER_SIZE {
            vec![]
        } else {
            Orders::new_orders(cast_slice(
                &bid_orders.data.as_slice()[SPOT_TRADE_ACCOUNT_HEADER_SIZE..],
            ))
        };

        let ask_orders = if ask_orders.data.len() <= SPOT_TRADE_ACCOUNT_HEADER_SIZE {
            vec![]
        } else {
            Orders::new_orders(cast_slice(
                &ask_orders.data.as_slice()[SPOT_TRADE_ACCOUNT_HEADER_SIZE..],
            ))
        };

        OrderBook {
            bid_begin_line: instr_header.bid_lines_begin,
            ask_begin_line: instr_header.ask_lines_begin,
            total_lines_count: instr_header
                .ask_lines_count
                .max(instr_header.bid_lines_count) as usize,
            lines,
            bid_orders,
            ask_orders,
            rdf: 1f64 / instr_header.dec_factor as f64,
        }
    }

    pub fn iter_bids<'a>(&'a self) -> LinesIter<'a> {
        self.lines
            .iter_from(self.bid_begin_line, self.total_lines_count)
    }

    pub fn iter_asks<'a>(&'a self) -> LinesIter<'a> {
        self.lines
            .iter_from(self.ask_begin_line, self.total_lines_count)
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

    pub fn trade_sum(&self, a: i64, b: i64) -> anyhow::Result<i64> {
        let sum = (a as f64 * b as f64) * self.rdf;

        if sum.is_sign_negative() || sum.is_nan() || sum > MAX_SUM {
            bail!("Arithmetic overflow")
        }

        Ok(sum as i64)
    }

    pub fn line_sum(&self, line: &PxOrders, side: OrderSide, remaining_sum: i64) -> i64 {
        let orders = match side {
            OrderSide::Bid => &self.bid_orders,
            OrderSide::Ask => &self.ask_orders,
        };

        let mut orders = orders.iter_from(line.begin);
        let mut sum = 0;

        while let Some((_, order)) = orders.next() {
            sum += order.sum;
            if sum > remaining_sum {
                break;
            }
        }

        sum
    }

    pub fn fill(
        &self,
        line: &PxOrders,
        mut remaining_qty: i64,
        fee_rate: f64,
        side: OrderSide,
    ) -> anyhow::Result<(i64, i64, i64)> {
        let px = line.price;
        let orders = match side {
            OrderSide::Bid => &self.bid_orders,
            OrderSide::Ask => &self.ask_orders,
        };
        let mut orders = orders.iter_from(line.begin);

        let mut total_traded_qty: i64 = remaining_qty;
        let mut total_traded_sum: i64 = 0;
        let mut total_fees: i64 = 0;

        while let Some((_, order)) = orders.next()
            && remaining_qty > 0
        {
            let (traded_qty, traded_crncy) = if order.qty <= remaining_qty {
                (order.qty, order.sum)
            } else {
                (remaining_qty, self.trade_sum(remaining_qty, px)?)
            };

            remaining_qty -= traded_qty;
            total_traded_sum += traded_crncy;
            total_fees += (traded_crncy as f64 * fee_rate) as i64;
        }

        total_traded_qty -= remaining_qty;

        Ok((total_traded_qty, total_traded_sum, total_fees))
    }
}
