use std::ops::Deref;

use bytemuck::{Pod, Zeroable};
use solana_pubkey::Pubkey;

use crate::your_venue::types::{
    Discriminator, InstrId, capped_i64::CappedI64, instr_mask::InstrMask,
};

/// Instr Account Header describe the state of an Instrument
///
/// 1. **`asset_token_id`** - Id of asset token.
///     Pre: asset_token_id <= root.tokens_count
/// 2. **`crncy_token_id`** - Id of crncy token.
///     Pre: crncy_token_id <= root.tokens_count
/// 3. **`mask`**
///     * `PERP` = `0x40000000` — Perp instrument flag.
///     * `READY_TO_PERP_UPGRADE` = `0x01000000` — Indicates readiness to upgrade to a Perp.
///     * `ZERO_FEES` = `0x1` — Zero fees flag.
///     * `FIXED_FEES` = `0x2` — Fixed fees flag.
///     * `SIMILAR_ASSETS` = `0x4` — Similar assets flag.
///     * `USD_STABLECOIN` = `0x8` — USD stablecoin flag.
///     * `FOREX` = `0x10` — Forex flag.
/// 4. **`asset_tokens`** - Amount of assets tokens
/// 5. **`crncy_tokens`** - Amount of crncy tokens
/// 6. **`ps`** - Pool supply
/// 7. **`pool_fees`** - Pool fees
/// 8. **`last_px`** - The most resent **valid** price used by the system for calculations
/// 9. **`last_close`** - The most resent price of a closed trade
/// 10. **`alltime_trades`** - Record of ALL executed trades
/// 11. **`prev_trades`** - Record of executed trades during prev day
/// 12. **`day_volatility`** - Price volatitliy. Calculated by sqrt(variance)
/// 13. **`perp_last_px`** - The most resent price. `perp_underlying_px` is used for calculations
/// 14. **`perp_last_close`** - The most resent price for closed trade
/// 15. **`perp_alltime_trades`** - Amount of trades made on perp from taker side
/// 16. **`perp_prev_day_trades`** - Amounts of trades made on perp during prev day
/// 17. **`perp_open_int`** - Open interest. Total amount of open positions on perp
/// 18. **`maps_address`** - Address of maps account. Account store spot related memory maps, specifically in constants::spot
/// 19. **`perp_maps_address`** - Address of perp maps account. Account store perp related memory maps, specifically in constants::perp
/// 20. **`lut_address`** - Address of LUT, which contains accounts asosiated with spot/perp engines
/// 22. **`drv_count`** - v2
/// 23. **`asset_token_decs_count`** - Decimals of asset token. [MIN_DECS_COUNT..=MAX_DECS_COUNT]
/// 24. **`crncy_token_decs_count`** - Decimals of crncy token. [MIN_DECS_COUNT..=MAX_DECS_COUNT]
/// 25. **`slot`** - Last record of InstrAccountHeader writable assess for spot
/// 26. **`creator`** - Record of instrument creator(can be any client)
/// 27. **`last_time`** - Last time record of token statistic savings
/// 28. **`distrib_time`** - Time record of last dividents distribution
/// 29. **`base_crncy_index`** - Index of cnrcy record in Vec<BaseCrncyRecord> in CommunityState
/// 30. **`instance_counter`** - v2
/// 31. **`variance_counter`** - Amount of times variance was recalculated
/// 32. **`bids_tree_nodes_count`** - reserved
/// 33. **`bids_tree_lines_entry`** - Pointer on root node in bids lines RB Tree which stored in bids_tree_acc on spot
/// 34. **`bids_tree_orders_entry`** - Pointer on root node in bids orders RB Tree which stored in bids_tree_acc on spot
/// 35. **`asks_tree_nodes_count`** - reserved
/// 36. **`asks_tree_lines_entry`** - Pointer on root node in asks lines RB Tree which stored in asks_tree_acc on spot
/// 37. **`asks_tree_orders_entry`** - Pointer on root node in asks orders RB Tree which stored in asks_tree_acc on spot
/// 38. **`bid_lines_begin`** - Head of bid lines linked list on spot
/// 39. **`bid_lines_end`** -reserved
/// 40. **`bid_lines_count`** - Total amount of bid lines on spot
/// 41. **`ask_lines_begin`** - Head of ask lines linked list on spot
/// 42. **`ask_lines_end`** - reserved
/// 43. **`asl_lines_count`** - Total amount of ask lines on spot
/// 44. **`bid_orders_count`** - Total amount of bid orders on spot
/// 45. **`ask_orders_count`** - Total amount of ask orders on spot
/// 46. **`fixing_time`** - Time record of last price fix (variance update)
/// 47. **`fixing_crncy_tokens`** - Amount of crncy tokens exchange since last price fixing(variance update)
/// 48. **`fixing_asset_tokens`** - Amount of asset tokens exchange since last price fixing(variance update)
/// 49. **`counter`** - Total orders counter etc. order id record
/// 50. **`protocol_fees`** - Fees collected by protocol
/// 51. **`hits_counter`** - Amount of executed trades, used for candles buffers update
/// 52. **`last_asset_tokens`** - Amount of asset_tokens traded during last trade on spot
/// 53. **`last_crny_tokens`** - Amount of crncy_tokens traded during last trade on spot
/// 54. **`perp_underlying_px`** - Current price, used in calculations. Derived either from last_px either from oracle
/// 55. **`best_bid`** - Current best line price on bid side on spot
/// 56. **`best_ask`** - Current best line price on ask side on spot
/// 57. **`fixing_price`** - Price during last price fisxing(variance update)
/// 58. **`variance`**
/// 59. **`avg_spread`** - Record of average spread
/// 60. **`last_spread`** - The most resent spread calculation etc. 1 - best_bid / best_ask
/// 61. **`last_spread_time`** - Time record of the last spread calculation
/// 62. **`total_spread_period`** - Duration during which avg_spread has been calculated
/// 63. **`day_asset_tokens`** - Assets tokens traded during the day period
/// 64. **`day_cnry_tokens`** - Crncy tokens traded during the day period
/// 65. **`day_low`** - Lowest price during the day period
/// 66. **`day_high`** - Highest price during the day period
/// 67. **`prev_day_asset_tokens`** - Assets tokens traded during previous day period
/// 68. **`prev_day_cnry_tokens`** - Crncy tokens traded during previous day period
/// 69. **`alltime_asset_tokens`** - Total amount of assets tokens traded
/// 70. **`alltime_crncy_tokens`** - Total amount of crncy tokens traded
/// 71. **`day_trades`** - Amount of trades executed during current day period
/// 72. **`lp_day_trades`** - Amount of trades on lp during day paeriod
/// 73. **`lp_alltime_fees`** - All time fees record from lp
/// 74. **`lp_day_fees`** - Record of fees collected from lp during last day period
/// 75. **`lp_prev_day_fees`** - Reocrd of fees collected during last
/// 76. **`lp_time`** - reserved
/// 77. **`fees_time`** - reserved
/// 73. **`lp_prev_day_trades`** - Amount of trades on lp during previous day paeriod
/// 74. **`creation_time`** - Time of new instrument instruction execution
/// 75. **`dec_factor`** - Decimal factor for an instrument. Used to convert betweena assets and crncy
/// 76. **`perp_clients_count`** - Amount of reserved seats on perp
/// 77. **`perp_slot`** - Last record of InstrAccountHeader writable assess for perp
/// 78. **`perp_time`** - Last time record of manipulation with perp
/// 79. **`perp_funding_rate_slot`** - reserved
/// 80. **`perp_funding_rate_time`** - Record of last funding rate recalculation
/// 81. **`perp_long_px_tree_nodes_count`** - reserved
/// 82. **`perp_long_px_tree_entry`** - Pointer on root node in long px RB Tree which is stored in long_px_tree_acc on perp
/// 83. **`perp_short_px_tree_nodes_count`** - reserved
/// 84. **`perp_short_px_tree_entry`** - Pointer on root node in short px RB Tree which is stored in short_px_tree_acc on perp
/// 85. **`perp_rebalance_time_tree_nodes_count`** - reserved
/// 86. **`perp_rebalance_time_tree_entry`** - Pointer on root node in short px RB Tree which is stored in rebalance_time_tree_acc on perp
/// 87. **`perp_bids_tree_nodes_count`** - reserved
/// 88. **`perp_bids_tree_lines_entry`** - Pointer on root node in bids lines RB Tree which stored in bids_tree_acc on perp
/// 89. **`perp_bids_tree_orders_entry`** - Pointer on root node in bids orders RB Tree which stored in bids_tree_acc on perp
/// 90. **`perp_asks_tree_nodes_count`** - reserved
/// 91. **`perp_asks_tree_lines_entry`** - Pointer on root node in asks lines RB Tree which stored in asks_tree_acc on perp
/// 92. **`perp_asks_tree_orders_entry`** - Pointer on root node in asks orders RB Tree which stored in asks_tree_acc on perp
/// 93. **`perp_bid_lines_begin`** - Head of bid lines linked list on perp
/// 94. **`perp_bid_lines_end`** - reserved
/// 95. **`perp_bid_lines_count`** - Total amount of bid lines on perp
/// 96. **`perp_ask_lines_begin`** - Head of ask lines linked list on perp
/// 97. **`perp_ask_lines_end`** - reserved
/// 98. **`perp_ask_lines_count`** - Total amount of ask lines on perp
/// 99. **`perp_bid_orders_count`** - Total amount of bid orders on perp
/// 100. **`perp_ask_orders_count`** - Total amount of ask orders on perp
/// 101. **`perp_day_trades`** - Amount of trades executed during current day period
/// 102. **`perp_long_spot_price_for_withdrowal`** - Record of price before long margin call
/// 103. **`perp_short_spot_price_for_withdrowal`** - Record of price before short margin call
/// 104. **`perp_spot_price_for_withdrowal`** - Valid record of price before margin call, used during funds calculation
/// 105. **`perp_soc_loss_long_rate`** - Current social loss long rate
/// 106. **`perp_soc_loss_short_rate`** - Current social loss short rate
/// 107. **`perp_price_delta`** - Price delta between spot market price and perp underlying price. Used during funding payments calculations
/// 108. **`perp_funding_rate`** - Current funding rate
/// 109. **`perp_funding_funds`** - Total amount of funds paid and received by clients during funding payments
/// 110. **`perp_soc_loss_funds`** - Total amount of funds paid and received by clients during socialized losses
/// 111. **`perp_insurance_fund`** - Amount of funds stored in protocols insurance funds
/// 112. **`perp_last_asset_tokens`** - Amount of asset_tokens traded during last trade on perp
/// 113. **`perp_last_crny_tokens`** - Amount of crncy_tokens traded during last trade on perp
/// 114. **`perp_best_bid`** - Current best line price on bid side on perp
/// 115. **`perp_best_ask`** - Current best line price on ask side on perp
/// 114. **`perp_day_asset_tokens`** - Assets tokens traded during the day period on perp
/// 117. **`perp_day_cnry_tokens`** - Crncy tokens traded during the day period on perp
/// 118. **`perp_day_low`** - Lowest price during the day period on perp
/// 119. **`perp_day_high`** - Highest price during the day period on perp
/// 120. **`perp_prev_day_asset_tokens`** - Assets tokens traded during previous day period on perp
/// 121. **`perp_prev_day_cnry_tokens`** - Crncy tokens traded during previous day period on perp
/// 122. **`perp_alltime_asset_tokens`** - Total amount of assets tokens traded on perp
/// 123. **`perp_alltime_crncy_tokens`** - Total amount of crncy tokens traded on perp
/// 124. **`max_leverage`** - Dynamic value of currently max leverage, based on market volatility. max_leverage <= MAX_PERP_LEVERAGE
/// 125. **`liquidation_threshold`** - Threshold for liquidation process, based on makret volaitlity. liquidation_threshold <= MIN_LIQUIDATION_THRESHOLD
/// 126. **`seats_reserve`** - Current amount of funds spent on seats purchasing
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, Default, PartialEq, Debug)]
pub struct InstrAccountHeader {
    // Basic information about the instrument
    pub discriminator: Discriminator,
    pub instr_id: InstrId,
    pub asset_token_id: u32,
    pub crncy_token_id: u32,
    pub asset_token_decs_count: u32,
    pub crncy_token_decs_count: u32,
    pub mask: InstrMask,

    // Spot page (Instrument Selector)
    pub last_px: i64,
    pub last_close: i64,
    pub best_bid: i64,
    pub best_ask: i64,

    // Perp page
    pub perp_last_px: i64,
    pub perp_last_close: i64,
    pub perp_open_int: CappedI64,
    pub variance: f64,
    pub max_leverage: f64,
    pub prev_day_trades: i64,
    pub perp_insurance_fund: CappedI64,
    pub perp_price_delta: f64,
    pub short_ema_px: f64,

    // Liquidity page
    pub lp_prev_day_fees: CappedI64,
    pub asset_tokens: CappedI64,
    pub crncy_tokens: CappedI64,
    pub ps: CappedI64,
    pub pool_fees: CappedI64,

    pub protocol_fees_alloc: f64,
    pub reserved_value2: i64,
    pub reserved_value3: i64,

    pub last_trade_asset_tokens: CappedI64,
    pub last_trade_crncy_tokens: CappedI64,
    pub day_low: i64,
    pub day_high: i64,
    pub day_asset_tokens: CappedI64,
    pub day_crncy_tokens: CappedI64,
    pub perp_clients_count: u32,
    pub perp_day_trades: u32,
    pub perp_best_bid: i64,
    pub perp_best_ask: i64,
    pub perp_day_low: i64,
    pub perp_day_high: i64,
    pub perp_day_asset_tokens: CappedI64,
    pub perp_day_crncy_tokens: CappedI64,
    pub perp_alltime_trades: i64,
    pub perp_prev_day_trades: i64,
    pub perp_last_trade_asset_tokens: CappedI64,
    pub perp_last_trade_crncy_tokens: CappedI64,
    pub alltime_trades: i64,

    pub day_volatility: f64,
    pub maps_address: Pubkey,
    pub perp_maps_address: Pubkey,
    pub asset_mint: Pubkey,
    pub crncy_mint: Pubkey,
    pub lut_address: Pubkey,
    pub drv_count: u32,
    pub slot: u32,
    pub creator: Pubkey,
    pub last_time: u32,
    pub distrib_time: u32,
    pub base_crncy_index: u32,
    pub instance_counter: u32,
    pub variance_counter: u32,
    pub bids_tree_nodes_count: u32,
    pub bids_tree_lines_entry: u32,
    pub bids_tree_orders_entry: u32,
    pub asks_tree_nodes_count: u32,
    pub asks_tree_lines_entry: u32,
    pub asks_tree_orders_entry: u32,
    pub bid_lines_begin: u32,
    pub bid_lines_end: u32,
    pub bid_lines_count: u32,
    pub ask_lines_begin: u32,
    pub ask_lines_end: u32,
    pub ask_lines_count: u32,
    pub bid_orders_count: u32,
    pub ask_orders_count: u32,
    pub fixing_time: u32,
    pub fixing_crncy_tokens: CappedI64,
    pub fixing_asset_tokens: CappedI64,
    pub counter: i64,
    pub protocol_fees: CappedI64,
    pub hits_counter: i64,
    pub last_asset_tokens: CappedI64,
    pub last_crncy_tokens: CappedI64,
    pub perp_underlying_px: i64,
    pub fixing_px: i64,
    pub avg_spread: f64,
    pub last_spread: f64,
    pub last_spread_time: u32,
    pub total_spread_period: u32,
    pub prev_day_asset_tokens: CappedI64,
    pub prev_day_crncy_tokens: CappedI64,
    pub alltime_asset_tokens: f64,
    pub alltime_crncy_tokens: f64,
    pub day_trades: u32,
    pub lp_day_trades: u32,
    pub lp_alltime_fees: f64,
    pub lp_day_fees: CappedI64,
    pub lp_prev_day_trades: u32,
    pub lp_time: u32,
    pub fees_time: u32,
    pub creation_time: u32,
    pub dec_factor: i64,
    pub perp_slot: u32,
    pub perp_time: u32,
    pub perp_funding_rate_slot: u32,
    pub perp_funding_rate_time: u32,
    pub perp_long_px_tree_nodes_count: u32,
    pub perp_long_px_tree_entry: u32,
    pub perp_short_px_tree_nodes_count: u32,
    pub perp_short_px_tree_entry: u32,
    pub perp_rebalance_time_tree_nodes_count: u32,
    pub perp_rebalance_time_tree_entry: u32,
    pub perp_bids_tree_nodes_count: u32,
    pub perp_bids_tree_lines_entry: u32,
    pub perp_bids_tree_orders_entry: u32,
    pub perp_asks_tree_nodes_count: u32,
    pub perp_asks_tree_lines_entry: u32,
    pub perp_asks_tree_orders_entry: u32,
    pub perp_bid_lines_begin: u32,
    pub perp_bid_lines_end: u32,
    pub perp_bid_lines_count: u32,
    pub perp_ask_lines_begin: u32,
    pub perp_ask_lines_end: u32,
    pub perp_ask_lines_count: u32,
    pub perp_bid_orders_count: u32,
    pub perp_ask_orders_count: u32,
    pub perp_long_spot_price_for_withdrowal: i64,
    pub perp_short_spot_price_for_withdrowal: i64,
    pub perp_soc_loss_long_rate: f64,
    pub perp_soc_loss_short_rate: f64,
    pub perp_funding_rate: f64,
    pub perp_funding_funds: CappedI64,
    pub perp_soc_loss_funds: CappedI64,
    pub perp_prev_day_asset_tokens: CappedI64,
    pub perp_prev_day_crncy_tokens: CappedI64,
    pub perp_alltime_asset_tokens: f64,
    pub perp_alltime_crncy_tokens: f64,
    pub liquidation_threshold: f64,
    pub seats_reserve: i64,
    pub swap_fees: CappedI64,
    pub similar_assets_min_qty: CappedI64,
    pub fixed_fee_rate: f64,
    pub mid_ema_px: f64,
    pub long_ema_px: f64,
    pub log_seq_no: i64,

    pub asset_bump_seed: u8,
    pub crncy_bump_seed: u8,
    pub spot_fee_rate: u8,
    pub spot_pool_ratio: u8,

    pub reserved_value10: u32,
}

impl Deref for InstrAccountHeader {
    type Target = Discriminator;

    fn deref(&self) -> &Self::Target {
        &self.discriminator
    }
}

pub trait OffChainInstrAccountHeader {
    fn market_px(&self) -> i64;
}

impl OffChainInstrAccountHeader for InstrAccountHeader {
    fn market_px(&self) -> i64 {
        if self.best_ask < self.last_px {
            self.best_ask
        } else if self.best_bid > self.last_px {
            self.best_bid
        } else {
            self.last_px
        }
    }
}

pub const INSTR_ACCOUNT_HEADER_SIZE: usize = std::mem::size_of::<InstrAccountHeader>();
