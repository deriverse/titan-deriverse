use bytemuck::{Pod, Zeroable};

use crate::your_venue::types::{InstrId, capped_i64::CappedI64};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
/// New Swap Data
///
/// **Used in:** `swap` instruction
///
/// **Tag:** `26`
///
/// ### Fields
/// - `input_crncy`: u8 - Flag if 0 sell `crncy` else sell `asset`
/// - `instr_id` - Instr pair id
/// - `price` - Limit price for a swap
/// - `amount` - Swaps qty in base crncy
/// - `min_amount_out` - Min amount threshold for trade result, 0 by default
pub struct SwapData {
    pub tag: u8,
    pub input_crncy: u8,
    pub padding_u16: u16,
    pub instr_id: InstrId,
    pub price: i64,
    pub amount: CappedI64,
    pub min_amount_out: i64,
}
