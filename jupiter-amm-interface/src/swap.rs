#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Side {
    Bid,
    Ask,
}

/// This enum is a jupiter backend enum and does not map 1:1 to the onchain aggregator Swap enum
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Swap {
    #[default]
    Placeholder,
    Deriverse {
        side: Side,
        instr_id: u32,
        swap_fee_rate: f64,
    },
}
