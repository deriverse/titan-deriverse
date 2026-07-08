use bytemuck::{Pod, Zeroable};
use solana_pubkey::Pubkey;

use crate::your_venue::types::{Discriminator, token_mask::TokenMask};

/// Token State
///
/// 1. **`id`** - Unique id for token. DRVS token always has id = 0
/// 2. **`mask`**
///     - Token decimals = 0xFF
///     - Base crncy flag = 0x40000000
/// 4. **`base_crncy_index`** - Index of BaseCrncyRecord in CommunityState
#[repr(C)]
#[derive(Pod, Zeroable, Clone, Copy, Default, PartialEq, Debug)]
pub struct TokenState {
    pub discriminator: Discriminator,
    pub address: Pubkey,
    pub program_address: Pubkey,
    pub id: u32,
    pub mask: TokenMask,
    pub reserved: u32,
    pub base_crncy_index: u32,
}
