use std::ops::{Deref, Neg};

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

pub use crate::your_venue::errors::DeriverseErrorKind;
use crate::your_venue::types::capped_i64::CappedI64;

pub const MAX_NUMBER: i64 = i64::MAX >> 1;

pub mod constants;
pub mod helper;
pub mod instruction_constants;
pub mod instruction_data;
pub mod instrument;
pub mod token;

pub mod capped_i64 {
    use crate::trading_venue::error::{ErrorInfo, TradingVenueError};

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Default, PartialOrd, Ord)]
    #[repr(transparent)]
    pub struct CappedI64 {
        pub value: i64,
    }

    impl std::fmt::Display for CappedI64 {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    impl Neg for CappedI64 {
        type Output = Self;

        fn neg(self) -> Self::Output {
            Self { value: -self.value }
        }
    }

    impl PartialEq<i64> for CappedI64 {
        fn eq(&self, other: &i64) -> bool {
            self.value == *other
        }
    }

    impl PartialOrd<i64> for CappedI64 {
        fn partial_cmp(&self, other: &i64) -> Option<std::cmp::Ordering> {
            self.value.partial_cmp(other)
        }
    }

    impl From<i64> for CappedI64 {
        fn from(value: i64) -> Self {
            CappedI64 { value }
        }
    }

    impl From<CappedI64> for i64 {
        fn from(value: CappedI64) -> Self {
            value.value
        }
    }

    pub trait CappedNumber: Sized + Copy {
        type RawType: Into<Self> + From<Self> + Copy;

        #[inline]
        fn new(value: Self::RawType) -> Self {
            value.into()
        }

        // As the type copy taking a link will result in the same performance
        fn validate<T: Into<Self::RawType>>(value: T) -> Result<(), TradingVenueError>;

        #[inline]
        fn new_checked(value: Self::RawType) -> Result<Self, TradingVenueError> {
            Self::validate(value)?;

            Ok(Self::new(value))
        }

        fn get(&self) -> Self::RawType;

        fn add<T: Into<Self::RawType>>(&self, other: T) -> Self;

        fn sub<T: Into<Self::RawType>>(&self, other: T) -> Self;

        fn checked_add<T: Into<Self::RawType>>(&self, other: T) -> Option<Self>;

        fn checked_sub<T: Into<Self::RawType>>(&self, other: T) -> Option<Self>;

        #[inline]
        fn checked_sub_capped<T: Into<Self::RawType>>(
            &self,
            other: T,
        ) -> Result<Self, TradingVenueError> {
            let value =
                self.checked_sub(other.into())
                    .ok_or(TradingVenueError::CheckedMathError(ErrorInfo::StaticStr(
                        "Checked sub error",
                    )))?;

            Self::validate(value)?;

            Ok(value)
        }

        #[inline]
        fn checked_add_capped<T: Into<Self::RawType>>(
            &self,
            other: T,
        ) -> Result<Self, TradingVenueError> {
            let value =
                self.checked_add(other.into())
                    .ok_or(TradingVenueError::CheckedMathError(ErrorInfo::StaticStr(
                        "Checked add error",
                    )))?;

            Self::validate(value)?;

            Ok(value)
        }
    }

    impl CappedNumber for CappedI64 {
        type RawType = i64;

        #[inline]
        fn get(&self) -> Self::RawType {
            self.value
        }

        #[inline]
        fn add<T: Into<Self::RawType>>(&self, other: T) -> Self {
            Self {
                value: self.value + other.into(),
            }
        }

        #[inline]
        fn sub<T: Into<Self::RawType>>(&self, other: T) -> Self {
            Self {
                value: self.value - other.into(),
            }
        }

        #[inline]
        fn checked_add<T: Into<Self::RawType>>(&self, other: T) -> Option<Self> {
            self.value
                .checked_add(other.into())
                .map(|v| Self { value: v })
        }

        #[inline]
        fn checked_sub<T: Into<Self::RawType>>(&self, other: T) -> Option<Self> {
            self.value
                .checked_sub(other.into())
                .map(|v| Self { value: v })
        }

        #[inline]
        fn validate<T: Into<Self::RawType>>(value: T) -> Result<(), TradingVenueError> {
            if value.into().unsigned_abs() as i64 > MAX_NUMBER {
                return Err(TradingVenueError::CheckedMathError(ErrorInfo::StaticStr(
                    "Max number overflow",
                )));
            }

            Ok(())
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Zeroable, Pod, Default)]
/// A type-safe wrapper around 'u32' that represents the version of a
/// contract Ensures that version are handled correctly and prevents
/// accidental misuse of raw integer values.
pub struct Version(pub u32);

impl Deref for Version {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Zeroable, Pod, Default)]
/// A type-safe wrapper around 'u32' that represents the Tag of an account
/// Ensures that Tags are handled correctly and prevents
/// accidental misuse of raw integer values.
pub struct Tag(pub u32);

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, Zeroable, Pod, PartialEq)]
/// Discriminator is a unique identifier of every account in the system.
/// Should be stored in the first 8 bytes of accounts data.
pub struct Discriminator {
    pub tag: Tag,
    pub version: Version,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Zeroable, Pod, Default)]
/// A type-sfe wrapper around 'u32' that represents an id of an instrument
/// Ensures that ids are handled correctly and prevents accidental
/// misuse of raw integer values.
pub struct InstrId(pub u32);

impl Discriminator {
    pub fn new(tag: Tag, version: Version) -> Self {
        Self { tag, version }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Zeroable, Pod, Default)]
/// A type-safe wrapper around 'u32' that represents an id of a client
///
/// Ensures that ids are handled correctly and prevents accidental
/// misuse of raw integer values.
pub struct ClientId(pub u32);

pub mod instr_mask {
    use bytemuck::{Pod, Zeroable};
    use serde::{Deserialize, Serialize};

    #[repr(u32)]
    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
    pub enum InstrFlag {
        PerpActive = 0x40000000,
        ReadyToPerpUpgrade = 0x01000000,
        ZeroFees = 0x1,
        FixedFees = 0x2,
        SimilarAssets = 0x4,
        UsdStablecoin = 0x8,
        Forex = 0x10,
        Suspended = 0x20,
        LongMarginCall = 0x40,
        ShortMarginCall = 0x80,
        ExpandableCandles = 0x100,
    }

    impl std::fmt::Display for InstrFlag {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    pub trait SimpleInstrMask {
        fn get_flag(&self, flag: InstrFlag) -> bool;
        fn set_flag(&mut self, flag: InstrFlag);
        fn clear_flag(&mut self, flag: InstrFlag);
    }

    #[derive(Clone, Copy, Pod, Zeroable, Debug, Default, PartialEq, Eq)]
    #[repr(transparent)]
    pub struct InstrMask(pub u32);

    impl SimpleInstrMask for InstrMask {
        fn get_flag(&self, flag: InstrFlag) -> bool {
            self.0 & flag as u32 != 0
        }
        fn set_flag(&mut self, flag: InstrFlag) {
            self.0 |= flag as u32
        }
        fn clear_flag(&mut self, flag: InstrFlag) {
            self.0 &= !(flag as u32)
        }
    }
}

pub mod token_mask {
    use bytemuck::{Pod, Zeroable};
    use serde::{Deserialize, Serialize};

    #[repr(u32)]
    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
    pub enum TokenFlag {
        Token2022 = 0x80000000,
        BaseCrncy = 0x40000000,
        WrappedToken = 0x20000000,
        SAMCrncy = 0x10000000,
    }

    impl std::fmt::Display for TokenFlag {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    #[derive(Clone, Copy, Pod, Zeroable, Debug, Default, PartialEq, Eq)]
    #[repr(transparent)]
    pub struct TokenMask(pub u32);

    impl TokenMask {
        const DECIMALS_MASK: u32 = 0xFF;
        const BUMP_MASK: u32 = 0xFF00;
        const BUMP_SHIFT: u32 = 8;

        pub fn get_flag(&self, flag: TokenFlag) -> bool {
            self.0 & flag as u32 != 0
        }

        pub fn set_flag(&mut self, flag: TokenFlag) {
            self.0 |= flag as u32;
        }

        pub fn clear_flag(&mut self, flag: TokenFlag) {
            self.0 &= !(flag as u32);
        }

        pub fn decimals(&self) -> u8 {
            (self.0 & Self::DECIMALS_MASK) as u8
        }

        pub fn set_decimals(&mut self, decimals: u8) {
            self.0 = (self.0 & !Self::DECIMALS_MASK) | (decimals as u32);
        }

        pub fn bump_seed(&self) -> u8 {
            ((self.0 & Self::BUMP_MASK) >> Self::BUMP_SHIFT) as u8
        }

        pub fn set_bump_seed(&mut self, bump_seed: u8) {
            self.0 = (self.0 & !Self::BUMP_MASK) | ((bump_seed as u32) << Self::BUMP_SHIFT);
        }
    }

    #[test]
    fn test_token_mask() {
        let mut mask = TokenMask(0);

        mask.set_decimals(9);
        assert_eq!(mask.decimals(), 9);

        mask.set_bump_seed(255);
        assert_eq!(mask.bump_seed(), 255);

        mask.set_flag(TokenFlag::BaseCrncy);
        mask.set_flag(TokenFlag::SAMCrncy);

        assert!(mask.get_flag(TokenFlag::BaseCrncy));
        assert!(mask.get_flag(TokenFlag::SAMCrncy));
        assert!(!mask.get_flag(TokenFlag::WrappedToken));

        assert_eq!(mask.decimals(), 9);
        assert_eq!(mask.bump_seed(), 255);

        mask.clear_flag(TokenFlag::BaseCrncy);
        assert!(!mask.get_flag(TokenFlag::BaseCrncy));

        assert_eq!(mask.decimals(), 9);
        assert_eq!(mask.bump_seed(), 255);
        assert!(mask.get_flag(TokenFlag::SAMCrncy));
    }
}

#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy)]
pub struct SpotTradeAccountHeader {
    pub discriminator: Discriminator,
    pub instr_id: InstrId,
    pub slot: u32,
    pub asset_token_id: u32,
    pub crncy_token_id: u32,
}

pub const SPOT_TRADE_ACCOUNT_HEADER_SIZE: usize = size_of::<SpotTradeAccountHeader>();

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum OrderSide {
    Bid,
    Ask,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
/// Order
///
/// 1. **`qty`** — The total quantity of the order.
/// 2. **`sum`** — The total value of the order, calculated as
///    `price * qty * (1 / instr.decimal_factor)`.
/// 3. **`order_id`** — A unique identifier assigned to the order.
/// 4. **`orig_client_id`** — The identifier of the client who originally created the order.
/// 5. **`client_id`** — A temporary client identifier.
/// 6. **`line`** — The price line to which the order belongs.
/// 7. **`prev`** — A reference to the previous order in the price line’s linked list.
/// 8. **`next`** — A reference to the next order in the price line’s linked list.
/// 9. **`sref`** — The order’s index within the order memory map.
/// 10. **`link`** — The node index of the order within the RB Tree memory map.
/// 11. **`cl_prev`** — A reference to the previous order in the client’s linked list.
/// 12. **`cl_next`** — A reference to the next order in the client’s linked list.
/// 13. **`time`** — The timestamp representing when the order was created.
///
/// # Notes
///
/// - Orders are stored in a RB Tree keyed by `order_id`.
/// - Each price line maintains its own linked list of orders.
/// - Each client also maintains a linked list of their orders.
/// - When an order is not present, the constant `NULL_ORDER` is used to represent a `None` value.
pub struct Order {
    pub qty: CappedI64,
    pub sum: CappedI64,
    pub order_id: i64,
    pub orig_client_id: ClientId,
    pub client_id: ClientId,
    pub line: u32,
    pub prev: u32,
    pub next: u32,
    pub sref: u32,
    pub link: u32,
    pub cl_prev: u32,
    pub cl_next: u32,
    pub time: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
/// PxOrders(Lines)
///
/// Each `PxOrders` structure corresponds to a specific price level and maintains
/// references to the linked orders at that price, as well as its position within
/// the order book structure.
///
/// # Fields
///
/// 1. **`price`** — The price level of the line.
/// 2. **`qty`** — The total aggregated quantity of all orders at this price level.
/// 3. **`next`** — A reference to the next price line in the order book chain.
/// 4. **`prev`** — A reference to the previous price line in the order book chain.
/// 5. **`sref`** — The index of this price line within the lines memory map.
/// 6. **`link`** — A node index of corresponding node in RB Tree.
/// 7. **`begin`** — A reference to the first order in line.
/// 8. **`end`** — A reference to the last order in line.
///
/// # Notes
///
/// - Price lines form a doubly linked list.
/// - Prices are aligned to SpotPrams or PerpParams list.
pub struct PxOrders {
    pub price: i64,
    pub qty: CappedI64,
    pub next: u32,
    pub prev: u32,
    pub sref: u32,
    pub link: u32,
    pub begin: u32,
    pub end: u32,
}
