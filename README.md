## Deriverse Integration Notes

### Zero-amount swaps

The Deriverse on-chain program explicitly rejects zero-amount swaps with an `InvalidQuantity` error. The `quote()` implementation still handles a zero input amount — as required by the integration docs — but such quotes will never be forwarded to the on-chain program.

### `instr_id` in swap instruction construction

Building a Deriverse swap instruction requires an `instr_id` that identifies the instrument (market) on-chain. Because `protocol_to_venue` receives a `&dyn TradingVenue` and cannot access venue-specific fields directly, `PoolProtocol::Deriverse { instr_id }` is used as a carrier: the value is read from the pool account during `update_state()` and exposed through `protocol()`.

If `PoolProtocol` should remain a pure identifier with no extra data, see the comments in `protocol_to_venue()` (`src/swap_route/mod.rs`) for two alternative approaches — moving the conversion into the `TradingVenue` trait, or extracting `instr_id` from the serialized instruction returned by `generate_swap_instruction`.
