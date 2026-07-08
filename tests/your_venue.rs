//! Your venue's test suite — the same shared assertions the example passes, run
//! against `Deriverse`. On a fresh template these are red (the `Deriverse`
//! methods are `todo!()`); implement `src/your_venue/mod.rs` and fill in the
//! config below to turn them green.
//!
//! Like the example suite, the tests SKIP when `SOLANA_RPC_URL` (and, for the
//! simulations, dumped program binaries) are absent.

mod common;

use common::SuiteConfig;
use solana_pubkey::Pubkey;
use titan_integration_template::your_venue::Deriverse;

// Installs the allocation guard that powers the construction test's
// `assert_no_alloc` checks. The Makefile runs that test under `release-debug`
// so the guard is active; speed tests run under true `--release`.
#[cfg(debug_assertions)]
#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn pool() -> Pubkey {
    Pubkey::from_str_const("3vzgUvXqpqdKH55R4xVcAbThoXt5xusVLojquA3s1YHh")
}

fn programs() -> Vec<Pubkey> {
    vec![Pubkey::from_str_const(
        "DRVSpZ2YUYYKgZP8XtLhAGtT1zYSCKzeHfb4DgRnrgqD",
    )]
}

fn config() -> SuiteConfig {
    SuiteConfig {
        pool: pool(),
        programs: programs(),
    }
}

#[tokio::test]
async fn construction() {
    common::construction::<Deriverse>(&config()).await;
}

#[tokio::test]
async fn zero_input_spot_price() {
    common::zero_input_spot_price::<Deriverse>(&config()).await;
}

#[tokio::test]
async fn bound_simulation() {
    common::bound_simulation::<Deriverse>(&config()).await;
}

#[tokio::test]
async fn random_samples() {
    common::random_samples::<Deriverse>(&config()).await;
}

#[tokio::test]
async fn monotone() {
    common::monotone::<Deriverse>(&config()).await;
}

#[tokio::test]
async fn quoting_speed() {
    common::quoting_speed::<Deriverse>(&config()).await;
}

#[tokio::test]
async fn price_monotone() {
    common::price_monotone::<Deriverse>(&config()).await;
}

#[tokio::test]
async fn mean_value_theorem() {
    common::mean_value_theorem::<Deriverse>(&config()).await;
}
