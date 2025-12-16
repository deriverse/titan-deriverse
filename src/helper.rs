use drv_models::{
    constants::seeds::DRVS_SEED, new_types::version::Version, state::types::account_type,
};
use solana_sdk::pubkey::Pubkey;

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
