//! Your venue's venue-creation parsing test.
//!
//! Fill this in with a self-contained fixture for your venue's pool-creation
//! instruction. It should mirror `tests/venue_creation.rs`: no RPC, no network,
//! just the decompiled instruction shape that your parser must recognize.

use solana_pubkey::{Pubkey, pubkey};

use titan_integration_template::trading_venue::protocol::PoolProtocol;
use titan_integration_template::trading_venue::venue_creation::{ParsedInstruction, PoolCreation};
use titan_integration_template::your_venue::types::instruction_constants::instruction_constants::{
    DrvInstruction, NewInstrumentInstruction, SwapInstruction,
};
use titan_integration_template::your_venue::{DRV_PROGRAM_ID, parse_pool_creations};

const POOL: Pubkey = pubkey!("3vzgUvXqpqdKH55R4xVcAbThoXt5xusVLojquA3s1YHh");
const TOKEN_A_MINT: Pubkey = pubkey!("cbbtcf3aa214zXHbiAZQwf4122FBYbraNdFqgw4iMij");
const TOKEN_B_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

fn your_venue_pool_creation() -> ParsedInstruction {
    let mut accounts = vec![Pubkey::default(); 20];

    accounts[6] = TOKEN_A_MINT;
    accounts[13] = POOL;
    accounts[12] = TOKEN_B_MINT;

    let mut new_instrument_data = vec![0; 7];

    new_instrument_data[0] = NewInstrumentInstruction::INSTRUCTION_NUMBER;

    ParsedInstruction {
        program_id: DRV_PROGRAM_ID,
        accounts: accounts,
        data: new_instrument_data,
    }
}

fn unrelated_instruction() -> ParsedInstruction {
    let mut accounts = vec![Pubkey::default(); 11];

    accounts
        .iter_mut()
        .for_each(|account| *account = Pubkey::new_unique());

    let mut data = vec![0; 7];

    data[0] = SwapInstruction::INSTRUCTION_NUMBER;

    ParsedInstruction {
        program_id: DRV_PROGRAM_ID,
        accounts: accounts,
        data: data,
    }
}

#[test]
fn parses_your_venue_pool_creation() {
    let creations = parse_pool_creations(&[your_venue_pool_creation()]);

    assert_eq!(
        creations,
        vec![PoolCreation {
            protocol: PoolProtocol::Deriverse { instr_id: 0 },
            pool: POOL,
            mints: vec![TOKEN_A_MINT, TOKEN_B_MINT],
        }],
    );
}

#[test]
fn ignores_transactions_without_a_creation() {
    let creations = parse_pool_creations(&[unrelated_instruction()]);
    assert!(
        creations.is_empty(),
        "a transaction without a pool creation creates no pools, got {creations:?}"
    );
}
