# Titan-Deriverse Integration

A Titan aggregator integration for the Deriverse protocol.

## Internal State Construction

1. **Derive the key account** using the `key` method. The **key** account for Deriverse is an instrument account PDA derived from a pair of token mints (mint order matters).

2. **Construct the internal state** using `from_keyed_account`.

3. **Initialize the state** by calling `update`.

4. **Get a quote** using `quote` to calculate the expected outcome of a trade.

## Quote Examples

For a Deriverse instrument pair with `asset: TOKEN_A` and `currency: TOKEN_B`:

**Example 1: Swap TOKEN_A → TOKEN_B**
- Trade: 10 TOKEN_A → TOKEN_B
- Quote params:
  - `input_mint`: TOKEN_A
  - `output_mint`: TOKEN_B
  - `amount`: `10 * 10^TOKEN_A.decimals`

**Example 2: Swap TOKEN_B → TOKEN_A**
- Trade: 10 TOKEN_B → TOKEN_A
- Quote params:
  - `input_mint`: TOKEN_B
  - `output_mint`: TOKEN_A
  - `amount`: `10 * 10^TOKEN_B.decimals`

## Instruction Data

The swap instruction includes a Deriverse variant:

```rust
pub enum Swap {
    // ... other variants
    Deriverse {
        side: Side,
        instr_id: u32,
    },
}
```
`jupiter-amm-interface` copy contains extended `Swap` enum

The instruction data can be built using `lib::from_swap`.

## Usage Example
```rust
fn build_key_account() -> KeyedAccount {
    let a_token_state = {
        let addr = TOKEN_A.new_token_acc();
        let acc = RPC.get_account(&addr).unwrap();
        unsafe { *(acc.data.as_ptr() as *const TokenState) }
    };

    let b_token_state = {
        let addr = TOKEN_B.new_token_acc();
        let acc = RPC.get_account(&addr).unwrap();
        unsafe { *(acc.data.as_ptr() as *const TokenState) }
    };

    let keyd_addr = Pubkey::new_spot_acc(INSTR, a_token_state.id, b_token_state.id);
    let keyd_acc = RPC.get_account(&keyd_addr).unwrap();

    KeyedAccount {
        key: keyd_addr,
        account: keyd_acc,
        params: None,
    }
}

 let mut deriverse = Deriverse::from_keyed_account(
     &build_key_account,
     &AmmContext {
         clock_ref: ClockRef::default(),
     },
 )
 .unwrap();
 
 let accounts_to_update = deriverse.get_accounts_to_update();
 
 // load accounts to accounts_map
 
 deriverse.update(&accounts_map).unwrap();
 
 let in_amount = get_dec_factor((deriverse.b_token_state.mask & 0xFF) as u8) as u64;

 let quote_result = deriverse
     .quote(&jupiter_amm_interface::QuoteParams {
         amount: in_amount,
         input_mint: TOKEN_A,
         output_mint: TOKEN_B,
         swap_mode: jupiter_amm_interface::SwapMode::ExactIn,
     })
     .unwrap();
```

## Testing
```bash
cargo integration_tests
```
Execute only off chain tests

```bash
cargo test
```
Execute all tests
