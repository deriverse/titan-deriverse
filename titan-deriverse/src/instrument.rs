use drv_models::state::instrument::InstrAccountHeader;

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
