use std::{fmt::Debug, marker::PhantomData};

use bytemuck::cast_slice;
use drv_models::{
    constants::nulls::NULL_ORDER,
    state::{
        instrument::InstrAccountHeader,
        spots::spot_account_header::SPOT_TRADE_ACCOUNT_HEADER_SIZE,
        types::{OrderSide, PxOrders},
    },
};
use solana_sdk::account::Account;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct OrderBook {
    pub lines: Lines,
    pub bid_begin_line: u32,
    pub ask_begin_line: u32,
    pub total_lines_count: usize,
}

impl OrderBook {
    pub fn new(instr_header: &InstrAccountHeader, lines_acc: &Account) -> Self {
        let lines = if lines_acc.data.len() <= SPOT_TRADE_ACCOUNT_HEADER_SIZE {
            vec![]
        } else {
            Lines::new_lines(cast_slice(
                &lines_acc.data.as_slice()[SPOT_TRADE_ACCOUNT_HEADER_SIZE..],
            ))
        };

        OrderBook {
            bid_begin_line: instr_header.bid_lines_begin,
            ask_begin_line: instr_header.ask_lines_begin,
            total_lines_count: instr_header
                .ask_lines_count
                .max(instr_header.bid_lines_count) as usize,
            lines,
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
}

pub type Lines = Vec<PxOrders>;

pub trait LinesSugar {
    fn new_lines(slice: &[PxOrders]) -> Self;
    fn iter_from<'a>(&'a self, start_idx: u32, lines_count: usize) -> LinesIter<'a>;
}

pub trait LinesMutSugar {
    fn iter_mut_from<'a>(&'a mut self, start_idx: u32) -> LinesIterMut<'a>;
}

impl LinesSugar for Lines {
    fn new_lines(slice: &[PxOrders]) -> Self {
        slice.to_vec()
    }

    fn iter_from<'a>(&'a self, start_idx: u32, lines_count: usize) -> LinesIter<'a> {
        LinesIter {
            slice: self.as_slice(),
            current: Some(start_idx),
            remaining: lines_count,
        }
    }
}

pub struct LinesIter<'a> {
    slice: &'a [PxOrders],
    current: Option<u32>,
    remaining: usize,
}

impl<'a> Iterator for LinesIter<'a> {
    type Item = (u32, PxOrders);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let idx = match self.current.take() {
            Some(i) => i,
            None => return None,
        };

        if idx == NULL_ORDER {
            return None;
        }

        let entry = self.slice[idx as usize];
        self.remaining = self.remaining.saturating_sub(1);

        let next_idx = entry.next;
        if next_idx == NULL_ORDER || next_idx == idx {
            self.current = None;
        } else {
            self.current = Some(next_idx);
        }

        Some((idx, entry))
    }
}

pub struct LinesIterMut<'a> {
    ptr: *mut PxOrders,
    current: Option<u32>,
    remaining: usize,
    market: PhantomData<&'a ()>,
}

impl<'a> Iterator for LinesIterMut<'a> {
    type Item = (u32, &'a mut PxOrders);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let idx = match self.current.take() {
            Some(i) => i,
            None => return None,
        };

        if idx == NULL_ORDER {
            return None;
        }

        let entry: &'a mut PxOrders = unsafe { &mut *self.ptr.add(idx as usize) };

        self.remaining = self.remaining.saturating_sub(1);

        let next_idx = entry.next;
        if next_idx == NULL_ORDER || next_idx == idx {
            self.current = None;
        } else {
            self.current = Some(next_idx);
        }

        Some((idx, entry))
    }
}
