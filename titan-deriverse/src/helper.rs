use drv_models::{
    constants::{MAX_NUMBER, seeds::DRVS_SEED},
    new_types::version::Version,
    state::types::{CappedI64, account_type},
};
use jupiter_amm_interface::AmmError;
use solana_pubkey::Pubkey;

use crate::program_id::{self, VERSION};

pub fn get_seed_bytes_by_id(version: Version, tag: u32, id: u32, id2: u32) -> [u8; 16] {
    let mut res = [0; 16];
    res[0..4].copy_from_slice(&version.to_le_bytes());
    res[4..8].copy_from_slice(&tag.to_le_bytes());
    res[8..12].copy_from_slice(&id.to_le_bytes());
    res[12..16].copy_from_slice(&id2.to_le_bytes());
    res
}

pub fn get_token_seed_bytes(version: Version, mint: &Pubkey) -> [u8; 32] {
    let mut res = [0; 32];
    res[0..28].copy_from_slice(&(*mint).to_bytes()[0..28]);
    res[28..32].copy_from_slice(&version.to_le_bytes());
    res
}

pub fn get_seed_bytes(version: Version, tag: u32) -> [u8; 8] {
    let mut res = [0; 8];
    res[0..4].copy_from_slice(&version.to_le_bytes());
    res[4..8].copy_from_slice(&tag.to_le_bytes());
    res
}

pub fn get_dec_factor(decs_count: u8) -> i64 {
    let mut dec = 1;
    for _ in 0..decs_count {
        dec *= 10;
    }
    dec
}

pub trait Helper {
    fn get_drv_auth() -> Pubkey;
    fn new_spot_acc(tag: u32, asset_token_id: u32, crncy_token_id: u32) -> Pubkey;
    fn new_token_acc(&self) -> Pubkey;
    fn new_acc(tag: u32) -> Pubkey;
    fn new_client_primary_acc(&self) -> Pubkey;
    fn new_client_community_acc(&self) -> Pubkey;
}

impl Helper for Pubkey {
    fn get_drv_auth() -> Pubkey {
        Self::find_program_address(&[DRVS_SEED], &program_id::ID).0
    }

    fn new_spot_acc(tag: u32, asset_token_id: u32, crncy_token_id: u32) -> Pubkey {
        let program_id = program_id::id();
        let (drvs_auth, _) = Pubkey::find_program_address(&[DRVS_SEED], &program_id);
        let seed = get_seed_bytes_by_id(VERSION, tag, asset_token_id, crncy_token_id);
        let seeds = &[&seed, drvs_auth.as_ref()];
        let (acc, _) = Pubkey::find_program_address(seeds, &program_id);
        acc
    }

    fn new_token_acc(&self) -> Pubkey {
        let program_id = program_id::id();
        let (drvs_auth, _) = Pubkey::find_program_address(&[DRVS_SEED], &program_id);
        let seed = get_token_seed_bytes(VERSION, self);
        let seeds = &[&seed, drvs_auth.as_ref()];
        let (acc, _) = Pubkey::find_program_address(seeds, &program_id);
        acc
    }

    fn new_acc(tag: u32) -> Pubkey {
        let program_id = program_id::id();
        let (drvs_auth, _) = Pubkey::find_program_address(&[DRVS_SEED], &program_id);
        let seed = get_seed_bytes(VERSION, tag);
        let seeds = &[&seed, drvs_auth.as_ref()];
        let (acc, _) = Pubkey::find_program_address(seeds, &program_id);
        acc
    }

    fn new_client_primary_acc(&self) -> Pubkey {
        let program_id = program_id::id();
        let seed = get_seed_bytes(VERSION, account_type::CLIENT_PRIMARY);
        let seeds = &[&seed, self.as_ref()];
        let (acc, _) = Pubkey::find_program_address(seeds, &program_id);
        acc
    }

    fn new_client_community_acc(&self) -> Pubkey {
        let program_id = program_id::id();
        let seed = get_seed_bytes(VERSION, account_type::CLIENT_COMMUNITY);
        let seeds = &[&seed, self.as_ref()];
        let (acc, _) = Pubkey::find_program_address(seeds, &program_id);
        acc
    }
}

pub trait CappedNumber: Sized + Copy {
    type RawType: Into<Self> + From<Self> + Copy;

    #[inline]
    fn new(value: Self::RawType) -> Self {
        value.into()
    }

    // As the type copy taking a link will result in the same performance
    fn validate<T: Into<Self::RawType>>(value: T) -> Result<(), AmmError>;

    #[inline]
    fn new_checked(value: Self::RawType) -> Result<Self, AmmError> {
        Self::validate(value)?;

        Ok(Self::new(value))
    }

    fn get(&self) -> Self::RawType;

    fn add<T: Into<Self::RawType>>(&self, other: T) -> Self;

    fn sub<T: Into<Self::RawType>>(&self, other: T) -> Self;

    fn checked_add<T: Into<Self::RawType>>(&self, other: T) -> Option<Self>;

    fn checked_sub<T: Into<Self::RawType>>(&self, other: T) -> Option<Self>;

    #[inline]
    fn checked_sub_capped<T: Into<Self::RawType>>(&self, other: T) -> Result<Self, AmmError> {
        let value = self
            .checked_sub(other.into())
            .ok_or(AmmError::Custom("ArithmeticOverflow".to_string()))?;

        Self::validate(value)?;

        Ok(value)
    }

    #[inline]
    fn checked_add_capped<T: Into<Self::RawType>>(&self, other: T) -> Result<Self, AmmError> {
        let value = self
            .checked_add(other.into())
            .ok_or(AmmError::Custom("ArithmeticOverflow".to_string()))?;

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
    fn validate<T: Into<Self::RawType>>(value: T) -> Result<(), AmmError> {
        if value.into().unsigned_abs() as i64 > MAX_NUMBER {
            return Err(AmmError::Custom("Max Number Overflow".to_string()));
        }

        Ok(())
    }
}
