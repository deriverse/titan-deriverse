#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;

    use solana_account::Account;
    use solana_pubkey::Pubkey;

    pub type AccountMap = HashMap<Pubkey, Box<Account>, ahash::RandomState>;

    #[cfg(not(feature = "rpc-test"))]
    pub mod integration_tests {
        use anyhow::Result;

        use bytemuck::{Pod, Zeroable, bytes_of};
        use drv_models::{
            constants::{DF, nulls::NULL_ORDER, trading_limitations::MAX_PRICE},
            state::{
                candles::CandlesAccountHeader,
                community_account_header::CommunityAccountHeader,
                instrument::InstrAccountHeader,
                spots::spot_account_header::SpotTradeAccountHeaderNonGen,
                token::TokenState,
                types::{Order, PxOrders},
            },
        };
        use jupiter_amm_interface::{
            Amm, AmmContext, ClockRef, KeyedAccount, QuoteParams, SwapMode,
        };
        use serde_json::to_value;
        use solana_account::Account;
        use solana_pubkey::Pubkey;

        use crate::{
            Deriverse, InstructionBuilderParams, ParamsWrapper,
            helper::get_dec_factor,
            lines_linked_list::Lines,
            orders_linked_list::Orders,
            tests::tests::{
                AccountMap,
                integration_tests::config::{TOKEN_A, TOKEN_A1, TOKEN_B, TOKEN_B1},
            },
        };

        pub mod config {
            use solana_pubkey::Pubkey;

            pub struct Token {
                pub mint: Pubkey,
                pub token_id: u32,
                pub decs_count: u32,
            }

            pub const TOKEN_A: Token = Token {
                mint: Pubkey::from_str_const("ATokenMint111111111111111111111111111111111"),
                token_id: 2,
                decs_count: 6,
            };

            pub const TOKEN_B: Token = Token {
                mint: Pubkey::from_str_const("BTokenMint111111111111111111111111111111111"),
                token_id: 3,
                decs_count: 9,
            };

            pub const TOKEN_A1: Token = Token {
                mint: Pubkey::from_str_const("A1TokenMint11111111111111111111111111111111"),
                token_id: 4,
                decs_count: 9,
            };

            pub const TOKEN_B1: Token = Token {
                mint: Pubkey::from_str_const("B1TokenMint11111111111111111111111111111111"),
                token_id: 5,
                decs_count: 6,
            };
        }

        fn default_account_with_object<T: Pod>(object: &T) -> Account {
            Account {
                lamports: 0,
                data: bytemuck::bytes_of(object).to_vec(),
                owner: solana_system_interface::program::id(),
                executable: false,
                rent_epoch: 0,
            }
        }

        fn default_account_with_data(data: Vec<u8>) -> Account {
            Account {
                lamports: 0,
                data,
                owner: solana_system_interface::program::id(),
                executable: false,
                rent_epoch: 0,
            }
        }

        fn build_key_account(params: InstructionBuilderParams) -> Result<KeyedAccount> {
            let header = InstrAccountHeader {
                asset_mint: TOKEN_A.mint,
                crncy_mint: TOKEN_B.mint,
                asset_token_id: TOKEN_A.token_id,
                crncy_token_id: TOKEN_B.token_id,
                ..Zeroable::zeroed()
            };

            let params = to_value(ParamsWrapper {
                instruction_builder_params: params,
            })?;

            Ok(KeyedAccount {
                key: Pubkey::new_unique(),
                account: default_account_with_object(&header),
                params: Some(params),
            })
        }

        fn build_key_account1(params: InstructionBuilderParams) -> Result<KeyedAccount> {
            let header = InstrAccountHeader {
                asset_mint: TOKEN_A1.mint,
                crncy_mint: TOKEN_B1.mint,
                asset_token_id: TOKEN_A1.token_id,
                crncy_token_id: TOKEN_B1.token_id,
                ..Zeroable::zeroed()
            };

            let params = to_value(ParamsWrapper {
                instruction_builder_params: params,
            })?;

            Ok(KeyedAccount {
                key: Pubkey::new_unique(),
                account: default_account_with_object(&header),
                params: Some(params),
            })
        }

        impl Deriverse {
            pub fn init_community_header(
                &mut self,
                fee_rate: u32,
                account_metas: &mut AccountMap,
            ) -> Result<()> {
                let header = CommunityAccountHeader {
                    spot_fee_rate: fee_rate,
                    ..Zeroable::zeroed()
                };

                account_metas.insert(
                    self.accounts_ctx.community_acc,
                    Box::new(default_account_with_object(&header)),
                );

                Ok(())
            }

            pub fn init_order_book(
                &mut self,
                account_metas: &mut AccountMap,
                lines: Lines,
                ask_orderes: Orders,
                bid_orders: Orders,
                bid_begin_line: usize,
                ask_begin_line: usize,
            ) -> Result<()> {
                self.instr_header.bid_lines_begin = bid_begin_line as u32;
                self.instr_header.ask_lines_begin = ask_begin_line as u32;

                self.instr_header.bid_lines_count = lines.len() as u32;
                self.instr_header.ask_lines_count = lines.len() as u32;

                self.instr_header.best_ask = lines
                    .get(ask_begin_line)
                    .map(|line| line.price)
                    .unwrap_or(MAX_PRICE);
                self.instr_header.best_bid = lines
                    .get(bid_begin_line)
                    .map(|line| line.price)
                    .unwrap_or(0);

                let mut lines_acc = bytes_of(&SpotTradeAccountHeaderNonGen {
                    ..Zeroable::zeroed()
                })
                .to_vec();

                lines
                    .iter()
                    .for_each(|line| lines_acc.extend_from_slice(bytes_of(line)));

                account_metas.insert(
                    self.accounts_ctx.lines,
                    Box::new(default_account_with_data(lines_acc)),
                );

                let mut ask_orders_acc = bytes_of(&SpotTradeAccountHeaderNonGen {
                    ..Zeroable::zeroed()
                })
                .to_vec();

                ask_orderes
                    .iter()
                    .for_each(|order| ask_orders_acc.extend_from_slice(bytes_of(order)));

                account_metas.insert(
                    self.accounts_ctx.ask_orders,
                    Box::new(default_account_with_data(ask_orders_acc)),
                );

                let mut bid_orders_acc = bytes_of(&SpotTradeAccountHeaderNonGen {
                    ..Zeroable::zeroed()
                })
                .to_vec();

                bid_orders
                    .iter()
                    .for_each(|order| bid_orders_acc.extend_from_slice(bytes_of(order)));

                account_metas.insert(
                    self.accounts_ctx.bid_orders,
                    Box::new(default_account_with_data(bid_orders_acc)),
                );

                Ok(())
            }

            pub fn init_amm(&mut self, a_tokens: i64, b_tokens: i64) {
                let Deriverse { instr_header, .. } = self;

                instr_header.asset_mint = TOKEN_A.mint;
                instr_header.asset_tokens = a_tokens;

                instr_header.crncy_mint = TOKEN_B.mint;
                instr_header.crncy_tokens = b_tokens;

                instr_header.dec_factor =
                    get_dec_factor((9 + TOKEN_A.decs_count - TOKEN_B.decs_count) as u8);
            }

            pub fn init_amm1(&mut self, a_tokens: i64, b_tokens: i64) {
                let Deriverse { instr_header, .. } = self;

                instr_header.asset_mint = TOKEN_A1.mint;
                instr_header.asset_tokens = a_tokens;

                instr_header.crncy_mint = TOKEN_B1.mint;
                instr_header.crncy_tokens = b_tokens;

                instr_header.dec_factor =
                    get_dec_factor((9 + TOKEN_A1.decs_count - TOKEN_B1.decs_count) as u8);
            }
        }

        #[test]
        fn get_accounts_to_update() {
            let deriverse = Deriverse::from_keyed_account(
                &build_key_account(InstructionBuilderParams { ata_init: false }).unwrap(),
                &AmmContext {
                    clock_ref: ClockRef::default(),
                },
            )
            .unwrap();

            println!("Ctx: {:?}", deriverse.accounts_ctx);

            println!(
                "Accounts to update: {:?}",
                deriverse.get_accounts_to_update()
            );
        }

        #[test]
        fn update_derviverse() {
            let mut accounts_map = AccountMap::with_hasher(ahash::RandomState::new());

            let mut deriverse = Deriverse::from_keyed_account(
                &build_key_account(InstructionBuilderParams { ata_init: false }).unwrap(),
                &AmmContext {
                    clock_ref: ClockRef::default(),
                },
            )
            .unwrap();

            let lines = vec![
                // bid (line 0)
                PxOrders {
                    price: (10.4 * DF) as i64,
                    qty: 100_000,
                    next: 3,
                    prev: 1,
                    sref: 0,
                    begin: 0,
                    ..Zeroable::zeroed()
                },
                // bid (line 1)
                PxOrders {
                    price: (10.1 * DF) as i64,
                    qty: 100_000,
                    next: 0,
                    prev: NULL_ORDER,
                    sref: 1,
                    begin: 3,
                    ..Zeroable::zeroed()
                },
                // ask (line 2)
                PxOrders {
                    price: (9.9 * DF) as i64,
                    qty: 100_000,
                    next: 4,
                    prev: NULL_ORDER,
                    sref: 0,
                    begin: 0,
                    ..Zeroable::zeroed()
                },
                // bid (line 3)
                PxOrders {
                    price: (10.0 * DF) as i64,
                    qty: 100_000,
                    next: NULL_ORDER,
                    prev: 3,
                    sref: 0,
                    begin: 6,
                    ..Zeroable::zeroed()
                },
                // ask (line 4)
                PxOrders {
                    price: (10.1 * DF) as i64,
                    qty: 100_000,
                    next: 6,
                    prev: NULL_ORDER,
                    sref: 0,
                    begin: 3,
                    ..Zeroable::zeroed()
                },
                PxOrders {
                    next: NULL_ORDER,
                    prev: NULL_ORDER,
                    ..Zeroable::zeroed()
                },
                // ask (line 6)
                PxOrders {
                    price: (10.1 * DF) as i64,
                    qty: 100_000,
                    next: NULL_ORDER,
                    prev: 4,
                    sref: 0,
                    begin: 7,
                    ..Zeroable::zeroed()
                },
            ];

            let dec_factor =
                get_dec_factor((9 + TOKEN_A.decs_count - TOKEN_B.decs_count) as u8) as f64;
            let sum_for = |qty: i64, line: u32| {
                ((qty as f64 * lines[line as usize].price as f64) / dec_factor) as i64
            };

            let bid_orders: Orders = vec![
                Order {
                    qty: 30_000,
                    sum: sum_for(30_000, 0),
                    order_id: 0,
                    line: 0,
                    prev: NULL_ORDER,
                    next: 1,
                    sref: 0,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 40_000,
                    sum: sum_for(40_000, 0),
                    order_id: 1,
                    line: 0,
                    prev: 0,
                    next: 2,
                    sref: 1,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 30_000,
                    sum: sum_for(30_000, 0),
                    order_id: 2,
                    line: 0,
                    prev: 1,
                    next: NULL_ORDER,
                    sref: 2,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 25_000,
                    sum: sum_for(25_000, 1),
                    order_id: 3,
                    line: 1,
                    prev: NULL_ORDER,
                    next: 4,
                    sref: 3,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 50_000,
                    sum: sum_for(50_000, 1),
                    order_id: 4,
                    line: 1,
                    prev: 3,
                    next: 5,
                    sref: 4,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 25_000,
                    sum: sum_for(25_000, 1),
                    order_id: 5,
                    line: 1,
                    prev: 4,
                    next: NULL_ORDER,
                    sref: 5,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 20_000,
                    sum: sum_for(20_000, 3),
                    order_id: 6,
                    line: 3,
                    prev: NULL_ORDER,
                    next: 7,
                    sref: 6,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 30_000,
                    sum: sum_for(30_000, 3),
                    order_id: 7,
                    line: 3,
                    prev: 6,
                    next: 8,
                    sref: 7,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 30_000,
                    sum: sum_for(30_000, 3),
                    order_id: 8,
                    line: 3,
                    prev: 7,
                    next: 9,
                    sref: 8,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 20_000,
                    sum: sum_for(20_000, 3),
                    order_id: 9,
                    line: 3,
                    prev: 8,
                    next: NULL_ORDER,
                    sref: 9,
                    ..Zeroable::zeroed()
                },
                Order {
                    order_id: 10,
                    next: NULL_ORDER,
                    prev: NULL_ORDER,
                    ..Zeroable::zeroed()
                },
                Order {
                    order_id: 11,
                    next: NULL_ORDER,
                    prev: NULL_ORDER,
                    ..Zeroable::zeroed()
                },
            ];

            let ask_orders: Orders = vec![
                Order {
                    qty: 40_000,
                    sum: sum_for(40_000, 2),
                    order_id: 0,
                    line: 2,
                    prev: NULL_ORDER,
                    next: 1,
                    sref: 0,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 30_000,
                    sum: sum_for(30_000, 2),
                    order_id: 1,
                    line: 2,
                    prev: 0,
                    next: 2,
                    sref: 1,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 30_000,
                    sum: sum_for(30_000, 2),
                    order_id: 2,
                    line: 2,
                    prev: 1,
                    next: NULL_ORDER,
                    sref: 2,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 25_000,
                    sum: sum_for(25_000, 4),
                    order_id: 3,
                    line: 4,
                    prev: NULL_ORDER,
                    next: 4,
                    sref: 3,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 25_000,
                    sum: sum_for(25_000, 4),
                    order_id: 4,
                    line: 4,
                    prev: 3,
                    next: 5,
                    sref: 4,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 25_000,
                    sum: sum_for(25_000, 4),
                    order_id: 5,
                    line: 4,
                    prev: 4,
                    next: 6,
                    sref: 5,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 25_000,
                    sum: sum_for(25_000, 4),
                    order_id: 6,
                    line: 4,
                    prev: 5,
                    next: NULL_ORDER,
                    sref: 6,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 35_000,
                    sum: sum_for(35_000, 6),
                    order_id: 7,
                    line: 6,
                    prev: NULL_ORDER,
                    next: 8,
                    sref: 7,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 35_000,
                    sum: sum_for(35_000, 6),
                    order_id: 8,
                    line: 6,
                    prev: 7,
                    next: 9,
                    sref: 8,
                    ..Zeroable::zeroed()
                },
                Order {
                    qty: 30_000,
                    sum: sum_for(30_000, 6),
                    order_id: 9,
                    line: 6,
                    prev: 8,
                    next: NULL_ORDER,
                    sref: 9,
                    ..Zeroable::zeroed()
                },
                Order {
                    order_id: 10,
                    next: NULL_ORDER,
                    prev: NULL_ORDER,
                    ..Zeroable::zeroed()
                },
                Order {
                    order_id: 11,
                    next: NULL_ORDER,
                    prev: NULL_ORDER,
                    ..Zeroable::zeroed()
                },
            ];

            deriverse
                .init_community_header(10, &mut accounts_map)
                .unwrap();
            deriverse.init_amm(
                110 * get_dec_factor(TOKEN_A.decs_count as u8),
                11 * get_dec_factor(TOKEN_A.decs_count as u8),
            );
            deriverse
                .init_order_book(
                    &mut accounts_map,
                    lines.clone(),
                    ask_orders,
                    bid_orders,
                    1,
                    2,
                )
                .unwrap();

            accounts_map.insert(
                deriverse.accounts_ctx.a_token_state_acc,
                Box::new(default_account_with_data(
                    bytes_of(&TokenState::zeroed()).to_vec(),
                )),
            );
            accounts_map.insert(
                deriverse.accounts_ctx.b_token_state_acc,
                Box::new(default_account_with_data(
                    bytes_of(&TokenState::zeroed()).to_vec(),
                )),
            );
            accounts_map.insert(
                deriverse.accounts_ctx.instr_header,
                Box::new(default_account_with_object(deriverse.instr_header.as_ref())),
            );
            accounts_map.insert(
                deriverse.accounts_ctx.a_mint,
                Box::new(default_account_with_data(
                    bytes_of(&TokenState::zeroed()).to_vec(),
                )),
            );
            accounts_map.insert(
                deriverse.accounts_ctx.b_mint,
                Box::new(default_account_with_data(
                    bytes_of(&TokenState::zeroed()).to_vec(),
                )),
            );

            if let Some(candles) = deriverse.accounts_ctx.candles {
                let header = CandlesAccountHeader::<0>::zeroed();
                let header = bytes_of(&header);
                accounts_map.insert(
                    candles.0,
                    Box::new(default_account_with_data(header.to_vec())),
                );
                accounts_map.insert(
                    candles.1,
                    Box::new(default_account_with_data(header.to_vec())),
                );
                accounts_map.insert(
                    candles.2,
                    Box::new(default_account_with_data(header.to_vec())),
                );
            }

            let mut new_deriverse = Deriverse::from_keyed_account(
                &build_key_account(InstructionBuilderParams { ata_init: false }).unwrap(),
                &AmmContext {
                    clock_ref: ClockRef::default(),
                },
            )
            .unwrap();

            new_deriverse.update(&accounts_map).unwrap();

            // lines in correct order
            let bid_lines = vec![lines[1], lines[0], lines[3]];

            assert_eq!(
                bid_lines.len(),
                new_deriverse.order_book.iter_bids().count()
            );

            new_deriverse
                .order_book
                .iter_bids()
                .zip(bid_lines)
                .for_each(|((_, line), expected_line)| assert_eq!(line, expected_line));

            assert!(new_deriverse.amm.a_tokens != 0);
            assert!(new_deriverse.amm.b_tokens != 0);

            assert!(new_deriverse.order_book.lines.len() != 0);
        }

        pub mod test_quote_order_book_only {

            use std::os::unix::fs::FileTypeExt;

            use drv_models::constants::SWAP_FEE_RATE;
            use jupiter_amm_interface::FeeMode;

            use super::*;

            fn init_deriverse(instruction_builder_params: InstructionBuilderParams) -> Deriverse {
                let mut accounts_map = AccountMap::with_hasher(ahash::RandomState::new());

                let mut deriverse = Deriverse::from_keyed_account(
                    &build_key_account(instruction_builder_params.clone()).unwrap(),
                    &AmmContext {
                        clock_ref: ClockRef::default(),
                    },
                )
                .unwrap();

                let lines = vec![
                    // bid (line 0)
                    PxOrders {
                        price: (10.1 * DF) as i64,
                        qty: 100_000,
                        next: 3,
                        prev: 1,
                        sref: 0,
                        begin: 0,
                        ..Zeroable::zeroed()
                    },
                    // bid (line 1)
                    PxOrders {
                        price: (10.4 * DF) as i64,
                        qty: 100_000,
                        next: 0,
                        prev: NULL_ORDER,
                        sref: 1,
                        begin: 3,
                        ..Zeroable::zeroed()
                    },
                    // ask (line 2)
                    PxOrders {
                        price: (9.9 * DF) as i64,
                        qty: 100_000,
                        next: 4,
                        prev: NULL_ORDER,
                        sref: 0,
                        begin: 0,
                        ..Zeroable::zeroed()
                    },
                    // bid (line 3)
                    PxOrders {
                        price: (10.0 * DF) as i64,
                        qty: 100_000,
                        next: NULL_ORDER,
                        prev: 3,
                        sref: 0,
                        begin: 6,
                        ..Zeroable::zeroed()
                    },
                    // ask (line 4)
                    PxOrders {
                        price: (10.1 * DF) as i64,
                        qty: 100_000,
                        next: 6,
                        prev: NULL_ORDER,
                        sref: 0,
                        begin: 3,
                        ..Zeroable::zeroed()
                    },
                    // empty (line 5)
                    PxOrders {
                        next: NULL_ORDER,
                        prev: NULL_ORDER,
                        ..Zeroable::zeroed()
                    },
                    // ask (line 6)
                    PxOrders {
                        price: (10.1 * DF) as i64,
                        qty: 100_000,
                        next: NULL_ORDER,
                        prev: 4,
                        sref: 0,
                        begin: 7,
                        ..Zeroable::zeroed()
                    },
                ];

                let dec_factor =
                    get_dec_factor((9 + TOKEN_A.decs_count - TOKEN_B.decs_count) as u8) as f64;
                let sum_for = |qty: i64, line: u32| {
                    ((qty as f64 * lines[line as usize].price as f64) / dec_factor) as i64
                };

                let bid_orders: Orders = vec![
                    Order {
                        qty: 30_000,
                        sum: sum_for(30_000, 0),
                        order_id: 0,
                        line: 0,
                        prev: NULL_ORDER,
                        next: 1,
                        sref: 0,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 40_000,
                        sum: sum_for(40_000, 0),
                        order_id: 1,
                        line: 0,
                        prev: 0,
                        next: 2,
                        sref: 1,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 30_000,
                        sum: sum_for(30_000, 0),
                        order_id: 2,
                        line: 0,
                        prev: 1,
                        next: NULL_ORDER,
                        sref: 2,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 25_000,
                        sum: sum_for(25_000, 1),
                        order_id: 3,
                        line: 1,
                        prev: NULL_ORDER,
                        next: 4,
                        sref: 3,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 50_000,
                        sum: sum_for(50_000, 1),
                        order_id: 4,
                        line: 1,
                        prev: 3,
                        next: 5,
                        sref: 4,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 25_000,
                        sum: sum_for(25_000, 1),
                        order_id: 5,
                        line: 1,
                        prev: 4,
                        next: NULL_ORDER,
                        sref: 5,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 20_000,
                        sum: sum_for(20_000, 3),
                        order_id: 6,
                        line: 3,
                        prev: NULL_ORDER,
                        next: 7,
                        sref: 6,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 30_000,
                        sum: sum_for(30_000, 3),
                        order_id: 7,
                        line: 3,
                        prev: 6,
                        next: 8,
                        sref: 7,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 30_000,
                        sum: sum_for(30_000, 3),
                        order_id: 8,
                        line: 3,
                        prev: 7,
                        next: 9,
                        sref: 8,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 20_000,
                        sum: sum_for(20_000, 3),
                        order_id: 9,
                        line: 3,
                        prev: 8,
                        next: NULL_ORDER,
                        sref: 9,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        order_id: 10,
                        next: NULL_ORDER,
                        prev: NULL_ORDER,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        order_id: 11,
                        next: NULL_ORDER,
                        prev: NULL_ORDER,
                        ..Zeroable::zeroed()
                    },
                ];

                let ask_orders: Orders = vec![
                    Order {
                        qty: 40_000,
                        sum: sum_for(40_000, 2),
                        order_id: 0,
                        line: 2,
                        prev: NULL_ORDER,
                        next: 1,
                        sref: 0,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 30_000,
                        sum: sum_for(30_000, 2),
                        order_id: 1,
                        line: 2,
                        prev: 0,
                        next: 2,
                        sref: 1,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 30_000,
                        sum: sum_for(30_000, 2),
                        order_id: 2,
                        line: 2,
                        prev: 1,
                        next: NULL_ORDER,
                        sref: 2,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 25_000,
                        sum: sum_for(25_000, 4),
                        order_id: 3,
                        line: 4,
                        prev: NULL_ORDER,
                        next: 4,
                        sref: 3,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 25_000,
                        sum: sum_for(25_000, 4),
                        order_id: 4,
                        line: 4,
                        prev: 3,
                        next: 5,
                        sref: 4,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 25_000,
                        sum: sum_for(25_000, 4),
                        order_id: 5,
                        line: 4,
                        prev: 4,
                        next: 6,
                        sref: 5,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 25_000,
                        sum: sum_for(25_000, 4),
                        order_id: 6,
                        line: 4,
                        prev: 5,
                        next: NULL_ORDER,
                        sref: 6,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 35_000,
                        sum: sum_for(35_000, 6),
                        order_id: 7,
                        line: 6,
                        prev: NULL_ORDER,
                        next: 8,
                        sref: 7,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 35_000,
                        sum: sum_for(35_000, 6),
                        order_id: 8,
                        line: 6,
                        prev: 7,
                        next: 9,
                        sref: 8,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 30_000,
                        sum: sum_for(30_000, 6),
                        order_id: 9,
                        line: 6,
                        prev: 8,
                        next: NULL_ORDER,
                        sref: 9,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        order_id: 10,
                        next: NULL_ORDER,
                        prev: NULL_ORDER,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        order_id: 11,
                        next: NULL_ORDER,
                        prev: NULL_ORDER,
                        ..Zeroable::zeroed()
                    },
                ];

                let orders_sum = bid_orders[0].sum + bid_orders[1].sum + bid_orders[2].sum;
                println!("Order sum {}", orders_sum);

                let lines_sum = ((100_000 as f64 * (10.1 * DF) as f64) / dec_factor) as i64;
                println!("Line sum: {}", lines_sum);

                assert_eq!(orders_sum, lines_sum);

                deriverse
                    .init_community_header(0, &mut accounts_map)
                    .unwrap();
                deriverse.init_amm(0, 0);
                deriverse
                    .init_order_book(
                        &mut accounts_map,
                        lines.clone(),
                        ask_orders,
                        bid_orders,
                        1,
                        2,
                    )
                    .unwrap();

                accounts_map.insert(
                    deriverse.accounts_ctx.a_token_state_acc,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState::zeroed()).to_vec(),
                    )),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_token_state_acc,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState {
                            address: TOKEN_B.mint,
                            ..Zeroable::zeroed()
                        })
                        .to_vec(),
                    )),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.a_mint,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState::zeroed()).to_vec(),
                    )),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_mint,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState::zeroed()).to_vec(),
                    )),
                );

                deriverse.instr_header.last_px = (10.0 * DF) as i64;

                accounts_map.insert(
                    deriverse.accounts_ctx.instr_header,
                    Box::new(default_account_with_object(deriverse.instr_header.as_ref())),
                );

                if let Some(candles) = deriverse.accounts_ctx.candles {
                    let header = CandlesAccountHeader::<0>::zeroed();
                    let header = bytes_of(&header);
                    accounts_map.insert(
                        candles.0,
                        Box::new(default_account_with_data(header.to_vec())),
                    );
                    accounts_map.insert(
                        candles.1,
                        Box::new(default_account_with_data(header.to_vec())),
                    );
                    accounts_map.insert(
                        candles.2,
                        Box::new(default_account_with_data(header.to_vec())),
                    );
                }

                let mut new_deriverse = Deriverse::from_keyed_account(
                    &build_key_account(instruction_builder_params).unwrap(),
                    &AmmContext {
                        clock_ref: ClockRef::default(),
                    },
                )
                .unwrap();

                new_deriverse.update(&accounts_map).unwrap();

                new_deriverse
            }

            #[test]
            fn partial_fill_sell_with_swap_fees() {
                let deriverse = init_deriverse(InstructionBuilderParams { ata_init: false });
                let result = deriverse
                    .quote(&QuoteParams {
                        fee_mode: FeeMode::Normal,
                        amount: 140_000,
                        input_mint: TOKEN_A.mint,
                        output_mint: TOKEN_B.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                let mut expected = (140_000 as f64
                    / get_dec_factor(TOKEN_A.decs_count as u8) as f64
                    * (10.4 * 100_000.0 / 140_000.0 + 10.1 * 40_000.0 / 140_000.0)
                    * get_dec_factor(TOKEN_B.decs_count as u8) as f64)
                    as u64;

                expected -= (expected as f64 * SWAP_FEE_RATE) as u64;

                let diff = (result.out_amount as i64 - expected as i64).abs();

                println!("Expected: {}", expected);
                println!("Result:   {}", result.out_amount);

                assert!(
                    (diff as f64) < expected as f64 * 0.001,
                    "Calculations are not presize enough"
                );
            }

            #[test]
            fn full_fill_sell() {
                let deriverse = init_deriverse(InstructionBuilderParams { ata_init: false });

                let result = deriverse
                    .quote(&QuoteParams {
                        fee_mode: FeeMode::Normal,
                        amount: 200_000,
                        input_mint: TOKEN_A.mint,
                        output_mint: TOKEN_B.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                let expected = ((200_000 as f64 / get_dec_factor(TOKEN_A.decs_count as u8) as f64
                    * (10.4 * 100_000.0 / 200_000.0 + 10.1 * 100_000.0 / 200_000.0)
                    * get_dec_factor(TOKEN_B.decs_count as u8) as f64)
                    * (1.0 - SWAP_FEE_RATE)) as u64;

                let diff = result.out_amount - expected;

                assert!(
                    (diff as f64) < expected as f64 * 0.001,
                    "Calculations are not presize enough"
                );
            }

            #[test]
            fn partial_fill_buy() {
                let deriverse = init_deriverse(InstructionBuilderParams { ata_init: false });

                let result = deriverse
                    .quote(&QuoteParams {
                        fee_mode: FeeMode::Normal,
                        amount: 1_400_000_000,
                        input_mint: TOKEN_B.mint,
                        output_mint: TOKEN_A.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                let expected = (result.in_amount as f64
                // due to complex calculations middle price between first and second asks lines is used
                / 9.96
                / (get_dec_factor((TOKEN_B.decs_count - TOKEN_A.decs_count) as u8) as f64))
                    as u64;
                let diff = (result.out_amount as i64 - expected as i64).abs() as u64;

                println!("Result: {}", result.out_amount);
                println!("Expected: {}", expected);

                assert!(
                    (diff as f64) < expected as f64 * 0.0001,
                    "Calculations are not presize enough"
                );
            }
        }

        pub mod test_quote_amm_only {
            use drv_models::constants::SWAP_FEE_RATE;
            use jupiter_amm_interface::FeeMode;

            use super::*;

            fn init_deriverse() -> Deriverse {
                let mut accounts_map = AccountMap::with_hasher(ahash::RandomState::new());

                let mut deriverse = Deriverse::from_keyed_account(
                    &build_key_account(InstructionBuilderParams { ata_init: false }).unwrap(),
                    &AmmContext {
                        clock_ref: ClockRef::default(),
                    },
                )
                .unwrap();

                let lines = vec![];

                deriverse
                    .init_community_header(0, &mut accounts_map)
                    .unwrap();
                deriverse.init_amm(
                    1_000_000 * get_dec_factor(TOKEN_A.decs_count as u8),
                    10_000_000 * get_dec_factor(TOKEN_B.decs_count as u8),
                );
                deriverse
                    .init_order_book(&mut accounts_map, lines.clone(), vec![], vec![], 0, 0)
                    .unwrap();

                accounts_map.insert(
                    deriverse.accounts_ctx.a_token_state_acc,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState::zeroed()).to_vec(),
                    )),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_token_state_acc,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState {
                            address: TOKEN_B.mint,
                            ..Zeroable::zeroed()
                        })
                        .to_vec(),
                    )),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.a_mint,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState::zeroed()).to_vec(),
                    )),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_mint,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState::zeroed()).to_vec(),
                    )),
                );

                deriverse.instr_header.last_px = (10.0 * DF) as i64;

                accounts_map.insert(
                    deriverse.accounts_ctx.instr_header,
                    Box::new(default_account_with_object(deriverse.instr_header.as_ref())),
                );

                if let Some(candles) = deriverse.accounts_ctx.candles {
                    let header = CandlesAccountHeader::<0>::zeroed();
                    let header = bytes_of(&header);
                    accounts_map.insert(
                        candles.0,
                        Box::new(default_account_with_data(header.to_vec())),
                    );
                    accounts_map.insert(
                        candles.1,
                        Box::new(default_account_with_data(header.to_vec())),
                    );
                    accounts_map.insert(
                        candles.2,
                        Box::new(default_account_with_data(header.to_vec())),
                    );
                }

                let mut new_deriverse = Deriverse::from_keyed_account(
                    &build_key_account(InstructionBuilderParams { ata_init: false }).unwrap(),
                    &AmmContext {
                        clock_ref: ClockRef::default(),
                    },
                )
                .unwrap();

                new_deriverse.update(&accounts_map).unwrap();

                new_deriverse
            }

            #[test]
            fn sell() {
                let deriverse = init_deriverse();

                let result = deriverse
                    .quote(&QuoteParams {
                        fee_mode: FeeMode::Normal,
                        amount: 140_000,
                        input_mint: TOKEN_A.mint,
                        output_mint: TOKEN_B.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                let expected = (result.in_amount as f64
                    * 10.0
                    * (get_dec_factor((TOKEN_B.decs_count - TOKEN_A.decs_count) as u8) as f64)
                    * (1.0 - SWAP_FEE_RATE)) as u64;
                println!("Expected: {}", expected);
                let diff = (result.out_amount as i64 - expected as i64).abs() as u64;

                assert!(
                    (diff as f64) < expected as f64 * 0.001,
                    "Calculations are not presize enough"
                );
            }

            #[test]
            fn buy() {
                let mut deriverse = init_deriverse();

                deriverse.instr_header.asset_tokens =
                    1_000_000 * get_dec_factor(TOKEN_A.decs_count as u8);

                deriverse.instr_header.crncy_tokens =
                    10_000_000 * get_dec_factor(TOKEN_B.decs_count as u8);

                let result = deriverse
                    .quote(&QuoteParams {
                        fee_mode: FeeMode::Normal,
                        amount: 1_400_000_000,
                        input_mint: TOKEN_B.mint,
                        output_mint: TOKEN_A.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                println!("In Amount: {}", result.in_amount);
                println!("Out Amount: {}", result.out_amount);

                let expected = ((result.in_amount as f64
                    / 10.0
                    / (get_dec_factor((TOKEN_B.decs_count - TOKEN_A.decs_count) as u8) as f64))
                    * (1.0 - SWAP_FEE_RATE)) as u64;
                println!("Expected: {}", expected);
                let diff = (result.out_amount as i64 - expected as i64).abs();

                assert!(
                    (diff as f64) < (expected as f64 * 0.000_001),
                    "Calculations are not presize enough: diff ({}) > {}",
                    diff,
                    expected as f64 * 0.000_001
                );
            }
        }

        pub mod test_order_book_and_amm {

            use crate::tests::tests::integration_tests::config::{TOKEN_A1, TOKEN_B1};
            use jupiter_amm_interface::FeeMode;

            use super::*;

            fn init_deriverse() -> Deriverse {
                let mut accounts_map = AccountMap::with_hasher(ahash::RandomState::new());

                let mut deriverse = Deriverse::from_keyed_account(
                    &build_key_account1(InstructionBuilderParams { ata_init: false }).unwrap(),
                    &AmmContext {
                        clock_ref: ClockRef::default(),
                    },
                )
                .unwrap();

                let lines = vec![
                    // ask (line 0)
                    PxOrders {
                        price: (85.52 * DF) as i64,
                        qty: 25000000,
                        next: 1,
                        prev: NULL_ORDER,
                        sref: 0,
                        begin: 0,
                        ..Zeroable::zeroed()
                    },
                    // ask (line 1)
                    PxOrders {
                        price: (85.53 * DF) as i64,
                        qty: 30000000,
                        next: 2,
                        prev: 0,
                        sref: 1,
                        begin: 1,
                        ..Zeroable::zeroed()
                    },
                    // ask (line 2)
                    PxOrders {
                        price: (85.55 * DF) as i64,
                        qty: 35000000,
                        next: 3,
                        prev: 1,
                        sref: 2,
                        begin: 2,
                        ..Zeroable::zeroed()
                    },
                    // ask (line 3)
                    PxOrders {
                        price: (85.57 * DF) as i64,
                        qty: 40000000,
                        next: 4,
                        prev: 2,
                        sref: 3,
                        begin: 3,
                        ..Zeroable::zeroed()
                    },
                    // ask (line 4)
                    PxOrders {
                        price: (85.58 * DF) as i64,
                        qty: 45000000,
                        next: 5,
                        prev: 3,
                        sref: 4,
                        begin: 4,
                        ..Zeroable::zeroed()
                    },
                    // ask (line 5)
                    PxOrders {
                        price: (85.6 * DF) as i64,
                        qty: 50000000,
                        next: NULL_ORDER,
                        prev: 4,
                        sref: 5,
                        begin: 5,
                        ..Zeroable::zeroed()
                    },
                    // bid (line 0)
                    PxOrders {
                        price: (85.34 * DF) as i64,
                        qty: 25000000,
                        next: 7,
                        prev: NULL_ORDER,
                        sref: 6,
                        begin: 0,
                        ..Zeroable::zeroed()
                    },
                    // bid (line 1)
                    PxOrders {
                        price: (85.33 * DF) as i64,
                        qty: 30000000,
                        next: 8,
                        prev: 6,
                        sref: 7,
                        begin: 1,
                        ..Zeroable::zeroed()
                    },
                    // bid (line 2)
                    PxOrders {
                        price: (85.31 * DF) as i64,
                        qty: 35000000,
                        next: 9,
                        prev: 7,
                        sref: 8,
                        begin: 2,
                        ..Zeroable::zeroed()
                    },
                    // bid (line 3)
                    PxOrders {
                        price: (85.29 * DF) as i64,
                        qty: 40000000,
                        next: 10,
                        prev: 8,
                        sref: 9,
                        begin: 3,
                        ..Zeroable::zeroed()
                    },
                    // bid (line 4)
                    PxOrders {
                        price: (85.27 * DF) as i64,
                        qty: 45000000,
                        next: 11,
                        prev: 9,
                        sref: 10,
                        begin: 4,
                        ..Zeroable::zeroed()
                    },
                    // bid (line 5)
                    PxOrders {
                        price: (85.26 * DF) as i64,
                        qty: 50000000,
                        next: NULL_ORDER,
                        prev: 10,
                        sref: 11,
                        begin: 5,
                        ..Zeroable::zeroed()
                    },
                ];

                let dec_factor =
                    get_dec_factor((9 + TOKEN_A1.decs_count - TOKEN_B1.decs_count) as u8) as f64;
                let sum_for = |qty: i64, line: u32| {
                    ((qty as f64 * lines[line as usize].price as f64) / dec_factor) as i64
                };

                let bid_orders: Orders = vec![
                    Order {
                        qty: 25000000,
                        sum: sum_for(25000000, 6),
                        order_id: 0,
                        line: 6,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 0,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 30_000_000,
                        sum: sum_for(30_000_000, 7),
                        order_id: 1,
                        line: 7,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 1,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 35_000_000,
                        sum: sum_for(35_000_000, 8),
                        order_id: 2,
                        line: 8,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 2,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 40_000_000,
                        sum: sum_for(40_000_000, 9),
                        order_id: 3,
                        line: 9,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 3,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 45_000_000,
                        sum: sum_for(45_000_000, 10),
                        order_id: 4,
                        line: 10,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 4,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 50_000_000,
                        sum: sum_for(50_000_000, 11),
                        order_id: 5,
                        line: 11,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 5,
                        ..Zeroable::zeroed()
                    },
                ];

                let ask_orders: Orders = vec![
                    Order {
                        qty: 25000000,
                        sum: sum_for(25000000, 0),
                        order_id: 0,
                        line: 0,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 0,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 30_000_000,
                        sum: sum_for(30_000_000, 1),
                        order_id: 1,
                        line: 1,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 1,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 35_000_000,
                        sum: sum_for(35_000_000, 2),
                        order_id: 2,
                        line: 2,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 2,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 40_000_000,
                        sum: sum_for(40_000_000, 3),
                        order_id: 3,
                        line: 3,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 3,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 45_000_000,
                        sum: sum_for(45_000_000, 4),
                        order_id: 4,
                        line: 4,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 4,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 50_000_000,
                        sum: sum_for(50_000_000, 5),
                        order_id: 5,
                        line: 5,
                        prev: NULL_ORDER,
                        next: NULL_ORDER,
                        sref: 5,
                        ..Zeroable::zeroed()
                    },
                ];

                deriverse
                    .init_community_header(20, &mut accounts_map)
                    .unwrap();
                deriverse.init_amm1(9542270844, 816055002);

                deriverse
                    .init_order_book(
                        &mut accounts_map,
                        lines.clone(),
                        ask_orders,
                        bid_orders,
                        6,
                        0,
                    )
                    .unwrap();

                accounts_map.insert(
                    deriverse.accounts_ctx.a_token_state_acc,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState::zeroed()).to_vec(),
                    )),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_token_state_acc,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState {
                            address: TOKEN_B1.mint,
                            ..Zeroable::zeroed()
                        })
                        .to_vec(),
                    )),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.a_mint,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState::zeroed()).to_vec(),
                    )),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_mint,
                    Box::new(default_account_with_data(
                        bytes_of(&TokenState::zeroed()).to_vec(),
                    )),
                );

                deriverse.instr_header.asset_tokens = 9542270844;
                deriverse.instr_header.crncy_tokens = 816055002;
                deriverse.instr_header.day_volatility = 0.04023745522889307;
                deriverse.instr_header.last_px = 85519999939;
                deriverse.amm.a_tokens = 9542270844;
                deriverse.amm.b_tokens = 816055002;

                accounts_map.insert(
                    deriverse.accounts_ctx.instr_header,
                    Box::new(default_account_with_object(deriverse.instr_header.as_ref())),
                );

                if let Some(candles) = deriverse.accounts_ctx.candles {
                    let header = CandlesAccountHeader::<0>::zeroed();
                    let header = bytes_of(&header);
                    accounts_map.insert(
                        candles.0,
                        Box::new(default_account_with_data(header.to_vec())),
                    );
                    accounts_map.insert(
                        candles.1,
                        Box::new(default_account_with_data(header.to_vec())),
                    );
                    accounts_map.insert(
                        candles.2,
                        Box::new(default_account_with_data(header.to_vec())),
                    );
                }

                let mut new_deriverse = Deriverse::from_keyed_account(
                    &build_key_account1(InstructionBuilderParams { ata_init: false }).unwrap(),
                    &AmmContext {
                        clock_ref: ClockRef::default(),
                    },
                )
                .unwrap();

                new_deriverse.update(&accounts_map).unwrap();

                new_deriverse
            }

            #[test]
            fn sell() {
                let deriverse = init_deriverse();

                let result = deriverse
                    .quote(&QuoteParams {
                        amount: 100000000,
                        input_mint: TOKEN_A1.mint,
                        output_mint: TOKEN_B1.mint,
                        swap_mode: SwapMode::ExactIn,
                        fee_mode: FeeMode::Normal,
                    })
                    .unwrap();

                println!("In Amount: {}", result.in_amount);
                println!("Out Amount: {}", result.out_amount);

                let expected_in_amount = 100000000;
                let expected_out_amount = 8528443;
                println!("Expected in_amount: {}", expected_in_amount);
                println!("Expected out_amount: {}", expected_out_amount);

                assert!(
                    result.in_amount == expected_in_amount,
                    "Diff in in_amount: {} vs {}",
                    result.in_amount,
                    expected_in_amount
                );
                assert!(
                    result.out_amount == expected_out_amount,
                    "Diff in out_amount:: {} vs {}",
                    result.out_amount,
                    expected_out_amount
                );
            }

            #[test]
            fn buy() {
                let deriverse = init_deriverse();
                let result = deriverse
                    .quote(&QuoteParams {
                        amount: 100000000,
                        input_mint: TOKEN_B1.mint,
                        output_mint: TOKEN_A1.mint,
                        swap_mode: SwapMode::ExactIn,
                        fee_mode: FeeMode::Normal,
                    })
                    .unwrap();

                println!("In Amount: {}", result.in_amount);
                println!("Out Amount: {}", result.out_amount);

                let expected_in_amount = 68795502;
                let expected_out_amount = 770731609;
                println!("Expected in_amount: {}", expected_in_amount);
                println!("Expected out_amount: {}", expected_out_amount);

                assert!(
                    result.in_amount == expected_in_amount,
                    "Diff in in_amount: {} vs {}",
                    result.in_amount,
                    expected_in_amount
                );
                assert!(
                    result.out_amount == expected_out_amount,
                    "Diff in out_amount:: {} vs {}",
                    result.out_amount,
                    expected_out_amount
                );
            }
        }
    }

    #[cfg(feature = "rpc-test")]
    pub mod rpc_tests {

        use ahash::{HashMap, HashMapExt};
        use bytemuck::bytes_of;
        use drv_models::state::{
            client_primary_account_header::ClientPrimaryAccountHeader,
            token::TokenState,
            types::account_type::{INSTR, SPOT_1M_CANDLES, SPOT_15M_CANDLES, SPOT_DAY_CANDLES},
        };
        use jupiter_amm_interface::{
            Amm, AmmContext, ClockRef, FeeMode, KeyedAccount, SwapAndAccountMetas, SwapParams,
        };
        use once_cell::sync::Lazy;
        use serde_json::to_value;
        use solana_client::{
            rpc_client::RpcClient, rpc_config::CommitmentConfig,
            rpc_response::transaction::Transaction,
        };

        use solana_instruction::Instruction;
        use solana_keypair::{EncodableKey, Keypair, Signer};
        use solana_pubkey::Pubkey;
        use spl_associated_token_account::get_associated_token_address_with_program_id;

        use crate::{
            Deriverse, InstructionBuilderParams, ParamsWrapper,
            custom_sdk::{
                deposit::{DepositBuildContext, DepositContext},
                extend_candles::ExtendCandlesBuilder,
                new_spot_order::{NewSpotOrderBuildContext, NewSpotOrderContext},
                traits::{Context, InstructionBuilder},
            },
            from_swap,
            helper::{Helper, get_dec_factor},
            program_id,
            tests::tests::rpc_tests::config::{TOKEN_A, TOKEN_B},
        };

        static RPC: Lazy<RpcClient> = Lazy::new(|| {
            let url = "https://api.devnet.solana.com";

            RpcClient::new_with_commitment(url, CommitmentConfig::confirmed())
        });

        static CLIENT_A: Lazy<Keypair> =
            Lazy::new(|| Keypair::read_from_file("../keys/client-a.json").unwrap());
        static CLIENT_B: Lazy<Keypair> =
            Lazy::new(|| Keypair::read_from_file("../keys/client-b.json").unwrap());
        static CLIENT_C: Lazy<Keypair> =
            Lazy::new(|| Keypair::read_from_file("../keys/client-c.json").unwrap());

        pub mod config {
            use solana_pubkey::Pubkey;

            pub const TOKEN_A: Pubkey =
                Pubkey::from_str_const("CEHfCDDZZcnVUxcvs1fh4ZztcaVqrakb3jfMQK4CPfNs");
            pub const TOKEN_B: Pubkey =
                Pubkey::from_str_const("SDg94MDr1WjJLfQjigef3Vo7ifceLtjbbCa6MxF6RCT");
        }

        impl InstructionBuilder for RpcClient {
            fn new_builder<U: Context>(
                &self,
                ctx: <U as Context>::Build,
            ) -> Result<Box<U>, solana_client::client_error::ClientError> {
                U::build(self, ctx)
            }
        }

        fn build_key_account(ata_init: bool, realloc_allowed: bool) -> KeyedAccount {
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

            let params = to_value(ParamsWrapper {
                instruction_builder_params: InstructionBuilderParams { ata_init },
            })
            .unwrap();

            KeyedAccount {
                key: keyd_addr,
                account: keyd_acc,
                params: Some(params),
            }
        }

        #[test]
        fn test_rpc() {
            let current_slot = RPC.get_slot().unwrap();

            assert!(current_slot > 0);
        }

        #[test]
        fn instruction_builder() {
            let ix = RPC
                .new_builder::<DepositContext>(DepositBuildContext {
                    signer: CLIENT_A.pubkey(),
                    token_mint: TOKEN_B,
                    amount: 100,
                    deposit_all: false,
                })
                .unwrap()
                .create_instruction();

            let mut tx = Transaction::new_with_payer(&[ix], Some(&CLIENT_A.pubkey()));
            tx.sign(
                &[CLIENT_A.insecure_clone()],
                RPC.get_latest_blockhash().unwrap(),
            );

            println!(
                "Signature: {}",
                RPC.send_and_confirm_transaction(&tx).unwrap()
            );

            let client_primary = {
                let addr = CLIENT_A.pubkey().new_client_primary_acc();
                let acc = RPC.get_account(&addr).unwrap();
                unsafe { *(acc.data.as_ptr() as *const ClientPrimaryAccountHeader) }
            };

            println!("Client primary: {}", client_primary.id);
        }

        pub fn init_deriverse() {
            let builder = RPC
                .new_builder::<NewSpotOrderContext>(NewSpotOrderBuildContext {
                    signer: CLIENT_A.pubkey(),
                    token_a_mint: TOKEN_A,
                    token_b_mint: TOKEN_B,
                    price: 10.1,
                    amount: 12.0,
                })
                .unwrap();

            let ix = builder.create_instruction();

            let mut tx = Transaction::new_with_payer(&[ix], Some(&CLIENT_A.pubkey()));
            tx.sign(
                &[CLIENT_A.insecure_clone()],
                RPC.get_latest_blockhash().unwrap(),
            );

            println!(
                "Signature: {}",
                RPC.send_and_confirm_transaction(&tx).unwrap()
            );
        }

        #[test]
        fn test_build_key_account() {
            let keyd_account = build_key_account(false, true);

            let mut deriverse = Deriverse::from_keyed_account(
                &keyd_account,
                &AmmContext {
                    clock_ref: ClockRef::default(),
                },
            )
            .unwrap();

            let accounts_to_update = deriverse.get_accounts_to_update();

            let accounts_map = RPC
                .get_multiple_accounts(&accounts_to_update)
                .unwrap()
                .iter()
                .enumerate()
                .fold(HashMap::new(), |mut m, (index, account)| {
                    if let Some(account) = account {
                        m.insert(accounts_to_update[index], Box::new(account.clone()));
                    }
                    m
                });

            deriverse.update(&accounts_map).unwrap();

            println!("Ask Orders: {:?}", deriverse.order_book.ask_orders);
            println!("Bid Orders: {:?}", deriverse.order_book.bid_orders);
        }

        fn extend_candles(asset_token_id: u32, crncy_token_id: u32) {
            let extend_candles_ix_1m = ExtendCandlesBuilder::extend_candles(
                CLIENT_A.pubkey(),
                SPOT_1M_CANDLES,
                asset_token_id,
                crncy_token_id,
            );

            let extend_candles_ix_15m = ExtendCandlesBuilder::extend_candles(
                CLIENT_A.pubkey(),
                SPOT_15M_CANDLES,
                asset_token_id,
                crncy_token_id,
            );
            let extend_candles_ix_day = ExtendCandlesBuilder::extend_candles(
                CLIENT_A.pubkey(),
                SPOT_DAY_CANDLES,
                asset_token_id,
                crncy_token_id,
            );

            let mut tx = Transaction::new_with_payer(
                &[
                    extend_candles_ix_1m,
                    extend_candles_ix_15m,
                    extend_candles_ix_day,
                ],
                Some(&CLIENT_A.pubkey()),
            );

            tx.sign(
                &[CLIENT_A.insecure_clone()],
                RPC.get_latest_blockhash().unwrap(),
            );

            RPC.send_and_confirm_transaction(&tx).unwrap();
        }

        #[test]
        fn test_deriverse() {
            let keyd_account = build_key_account(false, false);

            let mut deriverse = Deriverse::from_keyed_account(
                &keyd_account,
                &AmmContext {
                    clock_ref: ClockRef::default(),
                },
            )
            .unwrap();

            let accounts_to_update = deriverse.get_accounts_to_update();

            let accounts_map = RPC
                .get_multiple_accounts(&accounts_to_update)
                .unwrap()
                .iter()
                .enumerate()
                .fold(HashMap::new(), |mut m, (index, account)| {
                    if let Some(account) = account {
                        m.insert(accounts_to_update[index], Box::new(account.clone()));
                    }
                    m
                });

            deriverse.update(&accounts_map).unwrap();

            let in_amount = get_dec_factor(deriverse.b_token_state.mask.decimals()) as u64 - 2;

            let quote_result = deriverse
                .quote(&jupiter_amm_interface::QuoteParams {
                    fee_mode: FeeMode::Normal,
                    amount: in_amount,
                    input_mint: TOKEN_A,
                    output_mint: TOKEN_B,
                    swap_mode: jupiter_amm_interface::SwapMode::ExactIn,
                })
                .unwrap();

            println!("Result: {:?}", quote_result);

            println!("Program id: {}", deriverse.a_program_id);
            println!("Program id: {}", deriverse.b_program_id);

            let a_ata = get_associated_token_address_with_program_id(
                &CLIENT_B.pubkey(),
                &TOKEN_A,
                &deriverse.a_program_id,
            );

            let b_ata = get_associated_token_address_with_program_id(
                &CLIENT_B.pubkey(),
                &TOKEN_B,
                &deriverse.a_program_id,
            );

            let a_balance_before = {
                let account = RPC.get_account(&a_ata);

                account
                    .map(|acc| u64::from_le_bytes(acc.data[64..72].try_into().unwrap()))
                    .unwrap_or(0)
            };

            let b_balance_before = {
                let account = RPC.get_account(&b_ata);

                account
                    .map(|acc| u64::from_le_bytes(acc.data[64..72].try_into().unwrap()))
                    .unwrap_or(0)
            };

            println!("A before: {}", a_balance_before);
            println!("B before: {}", b_balance_before);

            if !deriverse.is_active() {
                panic!("Deriverse is not active for trading")
            }

            let SwapAndAccountMetas {
                swap,
                account_metas,
            } = deriverse
                .get_swap_and_account_metas(&SwapParams {
                    user: Pubkey::new_unique(),
                    payer: Pubkey::new_unique(),
                    in_amount,
                    source_mint: TOKEN_A,
                    destination_mint: TOKEN_B,
                    source_token_account: a_ata,
                    destination_token_account: b_ata,
                    token_transfer_authority: CLIENT_B.pubkey(),
                    swap_mode: jupiter_amm_interface::SwapMode::ExactIn,
                    out_amount: 0,
                    quote_mint_to_referrer: None,
                    jupiter_program_id: &Pubkey::new_unique(),
                    missing_dynamic_accounts_as_default: false,
                })
                .unwrap();

            let instruction_data = from_swap(swap, in_amount);

            let ix = Instruction::new_with_bytes(
                program_id::id(),
                bytes_of(&instruction_data),
                account_metas,
            );

            let mut tx = Transaction::new_with_payer(&[ix], Some(&CLIENT_B.pubkey()));
            tx.sign(
                &[CLIENT_B.insecure_clone()],
                RPC.get_latest_blockhash().unwrap(),
            );

            println!(
                "Signature: {}",
                RPC.send_and_confirm_transaction(&tx).unwrap()
            );

            let a_balance_after = {
                let account = RPC.get_account(&a_ata).unwrap();

                u64::from_le_bytes(account.data[64..72].try_into().unwrap())
            };

            let b_balance_after = {
                let account = RPC.get_account(&b_ata).unwrap();

                u64::from_le_bytes(account.data[64..72].try_into().unwrap())
            };

            assert!(a_balance_after < a_balance_before, "Incorrect order side");
            assert!(b_balance_after > b_balance_before, "Incorrect order side");

            assert!(
                (quote_result.in_amount as i64
                    - (a_balance_after as i64 - a_balance_before as i64).abs())
                    < (quote_result.in_amount as f64 * 0.012) as i64,
                "Calculations of quote where not precise enough"
            );

            assert!(
                (quote_result.out_amount as i64
                    - (b_balance_after as i64 - b_balance_before as i64).abs())
                    < (quote_result.out_amount as f64 * 0.012) as i64,
                "Calculations of quote where not precise enough"
            );

            println!("A before: {}", a_balance_after);
            println!("B before: {}", b_balance_after);
            println!(
                "A exchanged: {}",
                a_balance_after as i64 - a_balance_before as i64
            );
            println!(
                "B exchanged: {}",
                b_balance_after as i64 - b_balance_before as i64
            );

            assert_eq!(
                a_balance_before as i64 - a_balance_after as i64,
                in_amount as i64
            )
        }

        #[test]
        fn test_ata_creation() {
            let keyd_account = build_key_account(true, false);

            let mut deriverse = Deriverse::from_keyed_account(
                &keyd_account,
                &AmmContext {
                    clock_ref: ClockRef::default(),
                },
            )
            .unwrap();

            let accounts_to_update = deriverse.get_accounts_to_update();

            let accounts_map = RPC
                .get_multiple_accounts(&accounts_to_update)
                .unwrap()
                .iter()
                .enumerate()
                .fold(HashMap::new(), |mut m, (index, account)| {
                    if let Some(account) = account {
                        m.insert(accounts_to_update[index], Box::new(account.clone()));
                    }
                    m
                });

            deriverse.update(&accounts_map).unwrap();

            let in_amount = get_dec_factor(deriverse.b_token_state.mask.decimals()) as u64 - 2;

            let quote_result = deriverse
                .quote(&jupiter_amm_interface::QuoteParams {
                    fee_mode: FeeMode::Normal,
                    amount: in_amount,
                    input_mint: TOKEN_A,
                    output_mint: TOKEN_B,
                    swap_mode: jupiter_amm_interface::SwapMode::ExactIn,
                })
                .unwrap();

            println!("Result: {:?}", quote_result);

            println!("Program id: {}", deriverse.a_program_id);
            println!("Program id: {}", deriverse.b_program_id);

            let a_ata = get_associated_token_address_with_program_id(
                &CLIENT_C.pubkey(),
                &TOKEN_A,
                &deriverse.a_program_id,
            );

            let b_ata = get_associated_token_address_with_program_id(
                &CLIENT_C.pubkey(),
                &TOKEN_B,
                &deriverse.a_program_id,
            );

            let a_balance_before = {
                let account = RPC.get_account(&a_ata);

                account
                    .map(|acc| u64::from_le_bytes(acc.data[64..72].try_into().unwrap()))
                    .unwrap_or(0)
            };

            let b_balance_before = {
                let account = RPC.get_account(&b_ata);

                account
                    .map(|acc| u64::from_le_bytes(acc.data[64..72].try_into().unwrap()))
                    .unwrap_or(0)
            };

            println!("A before: {}", a_balance_before);
            println!("B before: {}", b_balance_before);

            if !deriverse.is_active() {
                panic!("Deriverse is not active for trading")
            }

            let SwapAndAccountMetas {
                swap,
                account_metas,
            } = deriverse
                .get_swap_and_account_metas(&SwapParams {
                    user: Pubkey::new_unique(),
                    payer: Pubkey::new_unique(),
                    in_amount,
                    source_mint: TOKEN_A,
                    destination_mint: TOKEN_B,
                    source_token_account: a_ata,
                    destination_token_account: b_ata,
                    token_transfer_authority: CLIENT_C.pubkey(),
                    swap_mode: jupiter_amm_interface::SwapMode::ExactIn,
                    out_amount: 0,
                    quote_mint_to_referrer: None,
                    jupiter_program_id: &Pubkey::new_unique(),
                    missing_dynamic_accounts_as_default: false,
                })
                .unwrap();

            let instruction_data = from_swap(swap, in_amount);

            let ix = Instruction::new_with_bytes(
                program_id::id(),
                bytes_of(&instruction_data),
                account_metas,
            );

            let mut tx = Transaction::new_with_payer(&[ix], Some(&CLIENT_C.pubkey()));
            tx.sign(
                &[CLIENT_C.insecure_clone()],
                RPC.get_latest_blockhash().unwrap(),
            );

            println!(
                "Signature: {}",
                RPC.send_and_confirm_transaction(&tx).unwrap()
            );

            let a_balance_after = {
                let account = RPC.get_account(&a_ata).unwrap();

                println!("Owner: {}", account.owner);

                u64::from_le_bytes(account.data[64..72].try_into().unwrap())
            };

            let b_balance_after = {
                let account = RPC.get_account(&b_ata).unwrap();

                u64::from_le_bytes(account.data[64..72].try_into().unwrap())
            };

            assert!(a_balance_after < a_balance_before, "Incorrect order side");
            assert!(b_balance_after > b_balance_before, "Incorrect order side");

            assert!(
                (quote_result.in_amount as i64
                    - (a_balance_after as i64 - a_balance_before as i64).abs())
                    < (quote_result.in_amount as f64 * 0.012) as i64,
                "Calculations of quote where not precise enough"
            );

            assert!(
                (quote_result.out_amount as i64
                    - (b_balance_after as i64 - b_balance_before as i64).abs())
                    < (quote_result.out_amount as f64 * 0.012) as i64,
                "Calculations of quote where not precise enough"
            );

            println!("A before: {}", a_balance_after);
            println!("B before: {}", b_balance_after);
            println!(
                "A exchanged: {}",
                a_balance_after as i64 - a_balance_before as i64
            );
            println!(
                "B exchanged: {}",
                b_balance_after as i64 - b_balance_before as i64
            );
            // close ata

            let b_ata_client_b = get_associated_token_address_with_program_id(
                &CLIENT_B.pubkey(),
                &TOKEN_B,
                &deriverse.a_program_id,
            );

            let close_acc = spl_token_interface::instruction::close_account(
                &spl_token_interface::id(),
                &b_ata,
                &b_ata_client_b,
                &CLIENT_C.pubkey(),
                &[&CLIENT_C.pubkey()],
            )
            .unwrap();

            let transfer_tokens = spl_token_interface::instruction::transfer_checked(
                &spl_token_interface::id(),
                &b_ata,
                &TOKEN_B,
                &b_ata_client_b,
                &CLIENT_C.pubkey(),
                &[&CLIENT_C.pubkey()],
                b_balance_after,
                deriverse.b_token_state.mask.decimals(),
            )
            .unwrap();

            let mut tx = Transaction::new_with_payer(
                &[transfer_tokens, close_acc],
                Some(&CLIENT_C.pubkey()),
            );

            tx.sign(
                &[CLIENT_C.insecure_clone()],
                RPC.get_latest_blockhash().unwrap(),
            );

            let result = RPC.send_and_confirm_transaction(&tx).unwrap();
            println!("Result {}", result);
        }
    }

    #[cfg(feature = "mainnet-test")]
    pub mod mainnet_tests {
        use ahash::{HashMap, HashMapExt};
        use drv_models::state::{token::TokenState, types::account_type::INSTR};
        use jupiter_amm_interface::{Amm, FeeMode, SwapMode};
        use jupiter_amm_interface::{AmmContext, ClockRef, KeyedAccount};
        use once_cell::sync::Lazy;
        use serde_json::to_value;
        use solana_client::{rpc_client::RpcClient, rpc_config::CommitmentConfig};
        use solana_pubkey::Pubkey;

        use crate::{
            Deriverse, InstructionBuilderParams, ParamsWrapper,
            helper::Helper,
            tests::tests::mainnet_tests::config::{TOKEN_A, TOKEN_B},
        };

        static RPC: Lazy<RpcClient> = Lazy::new(|| {
            let url = "https://rpc-mainnet.deriverse.io";

            RpcClient::new_with_commitment(url, CommitmentConfig::confirmed())
        });

        pub mod config {
            use solana_pubkey::Pubkey;

            pub const TOKEN_A: Pubkey =
                Pubkey::from_str_const("So11111111111111111111111111111111111111112");
            pub const TOKEN_B: Pubkey =
                Pubkey::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
        }

        #[test]
        fn test_rpc() {
            let current_slot = RPC.get_slot().unwrap();

            assert!(current_slot > 0);
        }

        fn build_key_account(ata_init: bool, realloc_allowed: bool) -> KeyedAccount {
            let b_token_state = {
                let addr = TOKEN_B.new_token_acc();
                let acc = RPC.get_account(&addr).unwrap();
                unsafe { *(acc.data.as_ptr() as *const TokenState) }
            };

            println!("B token state: {}", b_token_state.address);

            let a_token_state = {
                let addr = TOKEN_A.new_token_acc();
                let acc = RPC.get_account(&addr).unwrap();
                unsafe { *(acc.data.as_ptr() as *const TokenState) }
            };

            println!("A token state: {}", a_token_state.address);

            let keyd_addr = Pubkey::new_spot_acc(INSTR, a_token_state.id, b_token_state.id);
            let keyd_acc = RPC.get_account(&keyd_addr).unwrap();

            let params = to_value(ParamsWrapper {
                instruction_builder_params: InstructionBuilderParams { ata_init },
            })
            .unwrap();

            KeyedAccount {
                key: keyd_addr,
                account: keyd_acc,
                params: Some(params),
            }
        }

        #[test]
        fn test_build_key_account() {
            let keyd_account = build_key_account(false, true);

            println!("Keyd account: {}", keyd_account.key);

            let mut deriverse = Deriverse::from_keyed_account(
                &keyd_account,
                &AmmContext {
                    clock_ref: ClockRef::default(),
                },
            )
            .unwrap();

            let accounts_to_update = deriverse.get_accounts_to_update();

            let accounts_map = RPC
                .get_multiple_accounts(&accounts_to_update)
                .unwrap()
                .iter()
                .enumerate()
                .fold(HashMap::new(), |mut m, (index, account)| {
                    if let Some(account) = account {
                        m.insert(accounts_to_update[index], Box::new(account.clone()));
                    }
                    m
                });

            deriverse.update(&accounts_map).unwrap();

            println!("Ask Orders: {:?}", deriverse.order_book.ask_orders);
            println!("Bid Orders: {:?}", deriverse.order_book.bid_orders);
        }

        #[test]
        fn test_sol_usdc_swap() {
            let keyd_account = build_key_account(false, true);

            println!("Keyd account: {}", keyd_account.key);

            let mut deriverse = Deriverse::from_keyed_account(
                &keyd_account,
                &AmmContext {
                    clock_ref: ClockRef::default(),
                },
            )
            .unwrap();

            println!("Day volatility: {}", deriverse.instr_header.day_volatility);

            let accounts_to_update = deriverse.get_accounts_to_update();

            let accounts_map = RPC
                .get_multiple_accounts(&accounts_to_update)
                .unwrap()
                .iter()
                .enumerate()
                .fold(HashMap::new(), |mut m, (index, account)| {
                    if let Some(account) = account {
                        m.insert(accounts_to_update[index], Box::new(account.clone()));
                    }
                    m
                });

            deriverse.update(&accounts_map).unwrap();

            println!("Deriverse: {:?}", deriverse.amm);

            println!("Dierverse ask line: {:?}", deriverse.order_book.ask_orders);

            let in_amount = 67824544;

            let quote_result = deriverse
                .quote(&jupiter_amm_interface::QuoteParams {
                    amount: in_amount,
                    input_mint: TOKEN_B,
                    output_mint: TOKEN_A,
                    swap_mode: SwapMode::ExactIn,
                    fee_mode: FeeMode::Normal,
                })
                .unwrap();

            println!("Result {:?}", quote_result);
        }
    }
}
