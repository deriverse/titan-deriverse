use anyhow::{Result, anyhow, bail};
use drv_models::{
    constants::trading_limitations::MAX_SUM,
    state::{instrument::InstrAccountHeader, types::OrderSide},
};

#[derive(Clone, Default, PartialEq, Debug)]
pub struct DeriverseAmm {
    pub k: i128,
    pub a_tokens: i64,
    pub b_tokens: i64,
    pub df: f64,
    pub rdf: f64,
}

impl DeriverseAmm {
    pub fn new(instr_header: &InstrAccountHeader) -> Self {
        DeriverseAmm {
            k: instr_header.asset_tokens as i128 * instr_header.crncy_tokens as i128,
            a_tokens: instr_header.asset_tokens,
            b_tokens: instr_header.crncy_tokens,
            df: instr_header.dec_factor as f64,
            rdf: 1f64 / instr_header.dec_factor as f64,
        }
    }

    pub fn trade_sum(&self, a: i64, b: i64) -> Result<i64> {
        let sum = (a as f64 * b as f64) * self.rdf;

        if sum.is_sign_negative() || sum.is_nan() || sum > MAX_SUM {
            bail!("Arithmetic overflow")
        }

        Ok(sum as i64)
    }

    pub fn get_amm_qty(&self, price: i64, side: OrderSide) -> Result<i64> {
        Ok(match side {
            OrderSide::Bid => ((((self.k as f64 * self.df / price as f64).sqrt()) as i64)
                .checked_sub(self.a_tokens))
            .ok_or(anyhow!("Arithmetic overflow"))?
            .max(0),
            OrderSide::Ask => (self
                .a_tokens
                .checked_sub(((self.k as f64 * self.df / price as f64).sqrt()) as i64))
            .ok_or(anyhow!("Arithmetic overflow"))?
            .max(0),
        })
    }

    pub fn get_amm_px(&self, q: i64, side: OrderSide) -> Result<i64> {
        Ok(match side {
            OrderSide::Bid => {
                let new_tokens = (self
                    .a_tokens
                    .checked_add(q)
                    .ok_or(anyhow!("Arithmetic overflow"))?)
                    as i128;
                (((self.k as f64) * self.df) / (new_tokens * new_tokens) as f64) as i64
            }
            OrderSide::Ask => {
                if q >= self.a_tokens {
                    i64::MAX >> 1
                } else {
                    let new_tokens = (self
                        .a_tokens
                        .checked_sub(q)
                        .ok_or(anyhow!("Arithmetic overflow"))?)
                        as i128;
                    (((self.k as f64) * self.df) / (new_tokens * new_tokens) as f64) as i64
                }
            }
        })
    }

    pub fn get_amm_sum(&self, traded_qty: i64, side: OrderSide) -> Result<i64> {
        Ok(match side {
            OrderSide::Bid => {
                if self.a_tokens == 0 {
                    0
                } else {
                    (self.b_tokens as i128)
                        .checked_sub(self.k / (self.a_tokens + traded_qty) as i128)
                        .ok_or(anyhow!("Arithmetic overflow"))?
                        .max(0) as i64
                }
            }
            OrderSide::Ask => {
                let new_tokens = self.a_tokens - traded_qty;
                if new_tokens <= 0 {
                    0
                } else {
                    (self.k / new_tokens as i128)
                        .checked_sub(self.b_tokens as i128)
                        .ok_or(anyhow!("Arithmetic overflow"))?
                        .max(0) as i64
                }
            }
        })
    }

    pub fn get_reversed_amm_px(&self, sum: i64) -> Result<i64> {
        if self.b_tokens == 0 {
            Ok(i64::MAX >> 1)
        } else {
            let new_crncy = (self
                .b_tokens
                .checked_add(sum)
                .ok_or(anyhow!("Arithmetic overflow"))?) as i128;
            Ok((((new_crncy * new_crncy) as f64 * self.df) / self.k as f64) as i64)
        }
    }

    pub fn get_reversed_amm_qty(&self, traded_sum: i64) -> Result<i64> {
        if self.b_tokens == 0 {
            Ok(0)
        } else {
            let new_crncy = (self
                .b_tokens
                .checked_add(traded_sum)
                .ok_or(anyhow!("Arithmetic overflow"))?) as i128;
            Ok(self.a_tokens - (self.k / new_crncy) as i64)
        }
    }

    pub fn get_reversed_amm_sum(&self, price: i64) -> Result<i64> {
        if self.b_tokens == 0 {
            Ok(0)
        } else {
            Ok(-((self
                .b_tokens
                .checked_sub(((self.k as f64 * price as f64 / self.df).sqrt()) as i64))
            .ok_or(anyhow!("Arithmetic overflow"))?)
            .max(0))
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
