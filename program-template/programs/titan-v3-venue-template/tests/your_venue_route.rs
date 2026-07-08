//! Your venue's swap-route test — the same end-to-end suite the example passes,
//! run against `YourVenue`. Red once you've implemented YourVenue and pointed the
//! config below at a real pool + program (with SOLANA_RPC_URL set and the route
//! program built); SKIPs cleanly until then.

mod common;

use common::{run_swap_route, RouteConfig};
use solana_pubkey::Pubkey;
use titan_integration_template::your_venue::Deriverse;

fn pool() -> Pubkey {
    Pubkey::from_str_const("8Wk2L1yDovBJifCN1o86X7g7pDcqLau39m6tEsJ9Sheh")
}

fn venue_programs() -> Vec<Pubkey> {
    vec![Pubkey::from_str_const(
        "DRVSpZ2YUYYKgZP8XtLhAGtT1zYSCKzeHfb4DgRnrgqD",
    )]
}

#[tokio::test]
async fn swap_route_both_directions() {
    run_swap_route::<Deriverse>(RouteConfig {
        pool: pool(),
        venue_programs: venue_programs(),
    })
    .await;
}
