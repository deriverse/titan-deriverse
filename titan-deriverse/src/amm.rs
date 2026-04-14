use drv_models::state::{
    instrument::InstrAccountHeader,
    types::{CappedI64, OrderSide},
};
use jupiter_amm_interface::AmmError;

use crate::Result;
use crate::helper::CappedNumber;

#[derive(Clone, Default, PartialEq, Debug)]
pub struct DeriverseAmm {
    pub k: i128,
    pub a_tokens: CappedI64,
    pub b_tokens: CappedI64,
    pub df: f64,
    pub rdf: f64,
}

impl DeriverseAmm {
    pub fn new(instr_header: &InstrAccountHeader) -> Self {
        DeriverseAmm {
            k: instr_header.asset_tokens.value as i128 * instr_header.crncy_tokens.value as i128,
            a_tokens: instr_header.asset_tokens,
            b_tokens: instr_header.crncy_tokens,
            df: instr_header.dec_factor as f64,
            rdf: 1f64 / instr_header.dec_factor as f64,
        }
    }

    fn trade_sum<T: Into<i64>, U: Into<i64>>(&self, a: T, b: U) -> Result<CappedI64> {
        let sum = (a.into() as f64 * b.into() as f64) * self.rdf;

        if sum.is_sign_negative() || sum.is_nan() {
            return Err("Arithmetic Overflow".into());
        }

        CappedI64::new_checked(sum as i64)
    }

    pub fn get_amm_qty(&self, price: i64, side: OrderSide) -> Result<CappedI64> {
        CappedI64::new_checked(match side {
            OrderSide::Bid => ((((self.k as f64 * self.df / price as f64).sqrt()) as i64)
                .checked_sub(self.a_tokens.value))
            .ok_or(AmmError::Custom("Arithmetic Overflow".to_string()))?
            .max(0),
            OrderSide::Ask => (self
                .a_tokens
                .value
                .checked_sub(((self.k as f64 * self.df / price as f64).sqrt()) as i64))
            .ok_or(AmmError::Custom("Arithmetic Overflow".to_string()))?
            .max(0),
        })
    }

    pub fn get_amm_px<T: Into<i64>>(&self, q: T, side: OrderSide) -> Result<i64> {
        let q = q.into();
        Ok(match side {
            OrderSide::Bid => {
                let new_tokens = (self
                    .a_tokens
                    .value
                    .checked_add(q)
                    .ok_or(AmmError::Custom("Arithmetic Overflow".to_string()))?)
                    as i128;
                (((self.k as f64) * self.df) / (new_tokens * new_tokens) as f64) as i64
            }
            OrderSide::Ask => {
                if q >= self.a_tokens.value {
                    i64::MAX >> 1
                } else {
                    let new_tokens = (self
                        .a_tokens
                        .value
                        .checked_sub(q)
                        .ok_or(AmmError::Custom("Arithmetic Overflow".to_string()))?)
                        as i128;
                    (((self.k as f64) * self.df) / (new_tokens * new_tokens) as f64) as i64
                }
            }
        })
    }

    pub fn get_amm_sum<T: Into<i64>>(&self, traded_qty: T, side: OrderSide) -> Result<CappedI64> {
        let traded_qty = traded_qty.into();

        CappedI64::new_checked(match side {
            OrderSide::Bid => {
                if self.a_tokens == 0 {
                    0
                } else {
                    let new_asset_tokens = (self.a_tokens.value + traded_qty) as i128;
                    let mut sum = (self.b_tokens.value as i128)
                        .checked_sub(self.k / new_asset_tokens)
                        .ok_or(AmmError::Custom("Arithmetic Overflow".to_string()))?
                        .max(0) as i64;
                    if sum > 0 && new_asset_tokens * ((self.b_tokens.value - sum) as i128) < self.k
                    {
                        sum -= 1;
                    }
                    sum
                }
            }
            OrderSide::Ask => {
                let new_tokens = self.a_tokens.value - traded_qty;
                if new_tokens <= 0 {
                    0
                } else {
                    (self.k / new_tokens as i128)
                        .checked_sub(self.b_tokens.value as i128)
                        .ok_or(AmmError::Custom("Arithmetic Overflow".to_string()))?
                        .max(0) as i64
                }
            }
        })
    }

    pub fn get_reversed_amm_px<T: Into<i64>>(&self, sum: T) -> Result<i64> {
        if self.b_tokens == 0 {
            Ok(i64::MAX >> 1)
        } else {
            let new_crncy = (self
                .b_tokens
                .checked_add(sum)
                .ok_or(AmmError::Custom("Arithmetic Overflow".to_string()))?)
            .value as i128;
            Ok((((new_crncy * new_crncy) as f64 * self.df) / self.k as f64) as i64)
        }
    }

    pub fn get_reversed_amm_qty<T: Into<i64>>(&self, traded_sum: T) -> Result<CappedI64> {
        let traded_sum = traded_sum.into();
        if self.b_tokens == 0 {
            Ok(CappedI64::new(0))
        } else {
            let new_crncy = (self
                .b_tokens
                .checked_add(traded_sum)
                .ok_or(AmmError::Custom("Arithmetic Overflow".to_string()))?)
            .value as i128;
            let mut qty = self.a_tokens.value - (self.k / new_crncy) as i64;
            if qty > 0 && new_crncy * ((self.a_tokens.value - qty) as i128) < self.k {
                qty -= 1;
            }
            Ok(CappedI64::new(qty))
        }
    }

    pub fn get_reversed_amm_sum(&self, price: i64) -> Result<CappedI64> {
        if self.b_tokens == 0 {
            Ok(CappedI64::new(0))
        } else {
            CappedI64::new_checked(
                -((self
                    .b_tokens
                    .checked_sub(((self.k as f64 * price as f64 / self.df).sqrt()) as i64))
                .ok_or(AmmError::Custom("Arithmetic Overflow".to_string()))?)
                .value
                .max(0),
            )
        }
    }

    pub fn partial_fill(amm_px: i64, price: i64, side: OrderSide) -> bool {
        match side {
            OrderSide::Bid => amm_px < price,
            OrderSide::Ask => amm_px > price,
        }
    }

    pub fn last_line(amm_px: i64, line_px: i64, side: OrderSide) -> bool {
        match side {
            OrderSide::Bid => amm_px >= line_px,
            OrderSide::Ask => amm_px <= line_px,
        }
    }

    pub fn cover_line(amm_px: i64, price: i64, line_px: i64, side: OrderSide) -> bool {
        match side {
            OrderSide::Bid => amm_px.max(price) <= line_px,
            OrderSide::Ask => amm_px.min(price) >= line_px,
        }
    }

    pub fn line_is_unreachable(price: i64, line_px: i64, side: OrderSide) -> bool {
        match side {
            OrderSide::Bid => price > line_px,
            OrderSide::Ask => price < line_px,
        }
    }
}
