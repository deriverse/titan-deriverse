#[cfg(test)]
pub mod tests {

    #[cfg(not(feature = "rpc-test"))]
    pub mod integration_tests {
        use anyhow::Result;

        use bytemuck::{bytes_of, Pod, Zeroable};
        use drv_models::{
            constants::{nulls::NULL_ORDER, trading_limitations::MAX_PRICE, DF},
            state::{
                community_account_header::CommunityAccountHeader,
                instrument::InstrAccountHeader,
                spots::spot_account_header::SpotTradeAccountHeaderNonGen,
                token::TokenState,
                types::{Order, PxOrders},
            },
        };
        use jupiter_amm_interface::{
            AccountMap, Amm, AmmContext, ClockRef, KeyedAccount, QuoteParams, SwapMode,
        };
        use serde_json::to_value;
        use solana_sdk::{account::Account, pubkey::Pubkey};

        use crate::{
            helper::get_dec_factor,
            lines_linked_list::Lines,
            orders_linked_list::Orders,
            tests::tests::integration_tests::config::{TOKEN_A, TOKEN_B},
            Deriverse, InstructionBuilderParams, ParamsWrapper, SwapReferralParams,
        };

        pub mod config {
            use solana_sdk::pubkey::Pubkey;

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
                swap_ref_params: None,
                instruction_builder_params: params,
            })?;

            Ok(KeyedAccount {
                key: Pubkey::new_unique(),
                account: default_account_with_object(&header),
                params: Some(params),
            })
        }

        fn build_key_account_with_params(
            instruction_builder_params: InstructionBuilderParams,
            swap_referral_params: SwapReferralParams,
        ) -> Result<KeyedAccount> {
            let header = InstrAccountHeader {
                asset_mint: TOKEN_A.mint,
                crncy_mint: TOKEN_B.mint,
                asset_token_id: TOKEN_A.token_id,
                crncy_token_id: TOKEN_B.token_id,
                ..Zeroable::zeroed()
            };

            let params = to_value(ParamsWrapper {
                swap_ref_params: Some(swap_referral_params),
                instruction_builder_params: instruction_builder_params,
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
                    default_account_with_object(&header),
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
                    default_account_with_data(lines_acc),
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
                    default_account_with_data(ask_orders_acc),
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
                    default_account_with_data(bid_orders_acc),
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
        fn get_accounts_to_update_with_params() {
            let deriverse = Deriverse::from_keyed_account(
                &build_key_account_with_params(
                    InstructionBuilderParams { ata_init: false },
                    SwapReferralParams {
                        fee_rate_factor: 0.0001,
                        client_mint_token_acc: Pubkey::new_unique(),
                    },
                )
                .unwrap(),
                &AmmContext {
                    clock_ref: ClockRef::default(),
                },
            )
            .unwrap();

            assert_eq!(
                deriverse
                    .swap_referral_params
                    .clone()
                    .unwrap()
                    .fee_rate_factor,
                0.0001
            );

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
                default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
            );
            accounts_map.insert(
                deriverse.accounts_ctx.b_token_state_acc,
                default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
            );
            accounts_map.insert(
                deriverse.accounts_ctx.instr_header,
                default_account_with_object(deriverse.instr_header.as_ref()),
            );
            accounts_map.insert(
                deriverse.accounts_ctx.a_mint,
                default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
            );
            accounts_map.insert(
                deriverse.accounts_ctx.b_mint,
                default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
            );

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

            use super::*;

            fn init_deriverse(
                instruction_builder_params: InstructionBuilderParams,
                additional_params: Option<SwapReferralParams>,
            ) -> Deriverse {
                let mut accounts_map = AccountMap::with_hasher(ahash::RandomState::new());

                let mut deriverse = Deriverse::from_keyed_account(
                    &if let Some(params) = additional_params {
                        build_key_account_with_params(instruction_builder_params.clone(), params)
                            .unwrap()
                    } else {
                        build_key_account(instruction_builder_params.clone()).unwrap()
                    },
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
                    default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_token_state_acc,
                    default_account_with_data(
                        bytes_of(&TokenState {
                            address: TOKEN_B.mint,
                            ..Zeroable::zeroed()
                        })
                        .to_vec(),
                    ),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.a_mint,
                    default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_mint,
                    default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
                );

                deriverse.instr_header.last_px = (10.0 * DF) as i64;

                accounts_map.insert(
                    deriverse.accounts_ctx.instr_header,
                    default_account_with_object(deriverse.instr_header.as_ref()),
                );

                let mut new_deriverse = Deriverse::from_keyed_account(
                    &build_key_account(instruction_builder_params).unwrap(),
                    &AmmContext {
                        clock_ref: ClockRef::default(),
                    },
                )
                .unwrap();

                new_deriverse.swap_referral_params = deriverse.swap_referral_params;

                new_deriverse.update(&accounts_map).unwrap();

                new_deriverse
            }

            #[test]
            fn partial_fill_sell_with_swap_fees() {
                let swap_ref_params = SwapReferralParams {
                    fee_rate_factor: 0.01,
                    client_mint_token_acc: Pubkey::new_unique(),
                };

                let deriverse = init_deriverse(
                    InstructionBuilderParams { ata_init: false },
                    Some(swap_ref_params.clone()),
                );

                let result = deriverse
                    .quote(&QuoteParams {
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

                expected -= (expected as f64 * swap_ref_params.fee_rate_factor) as u64;

                let diff = (result.out_amount as i64 - expected as i64).abs();

                println!("Expected: {}", expected);
                println!("Result:   {}", result.out_amount);

                assert!(
                    (diff as f64) < expected as f64 * 0.001,
                    "Calculations are not presize enough"
                );
            }

            #[test]
            fn partial_fill_sell() {
                let deriverse = init_deriverse(InstructionBuilderParams { ata_init: false }, None);

                let result = deriverse
                    .quote(&QuoteParams {
                        amount: 140_000,
                        input_mint: TOKEN_A.mint,
                        output_mint: TOKEN_B.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                let expected = (140_000 as f64 / get_dec_factor(TOKEN_A.decs_count as u8) as f64
                    * (10.4 * 100_000.0 / 140_000.0 + 10.1 * 40_000.0 / 140_000.0)
                    * get_dec_factor(TOKEN_B.decs_count as u8) as f64)
                    as u64;

                let diff = result.out_amount - expected;

                assert!(
                    (diff as f64) < expected as f64 * 0.001,
                    "Calculations are not presize enough"
                );
            }

            #[test]
            fn full_fill_sell() {
                let deriverse = init_deriverse(InstructionBuilderParams { ata_init: false }, None);

                let result = deriverse
                    .quote(&QuoteParams {
                        amount: 200_000,
                        input_mint: TOKEN_A.mint,
                        output_mint: TOKEN_B.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                let expected = (200_000 as f64 / get_dec_factor(TOKEN_A.decs_count as u8) as f64
                    * (10.4 * 100_000.0 / 200_000.0 + 10.1 * 100_000.0 / 200_000.0)
                    * get_dec_factor(TOKEN_B.decs_count as u8) as f64)
                    as u64;

                let diff = result.out_amount - expected;

                assert!(
                    (diff as f64) < expected as f64 * 0.001,
                    "Calculations are not presize enough"
                );
            }

            #[test]
            fn partial_fill_buy() {
                let deriverse = init_deriverse(InstructionBuilderParams { ata_init: false }, None);

                let result = deriverse
                    .quote(&QuoteParams {
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

                assert!(
                    (diff as f64) < expected as f64 * 0.001,
                    "Calculations are not presize enough"
                );
            }
        }

        pub mod test_quote_amm_only {
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
                    default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_token_state_acc,
                    default_account_with_data(
                        bytes_of(&TokenState {
                            address: TOKEN_B.mint,
                            ..Zeroable::zeroed()
                        })
                        .to_vec(),
                    ),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.a_mint,
                    default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_mint,
                    default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
                );

                deriverse.instr_header.last_px = (10.0 * DF) as i64;

                accounts_map.insert(
                    deriverse.accounts_ctx.instr_header,
                    default_account_with_object(deriverse.instr_header.as_ref()),
                );

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
                        amount: 140_000,
                        input_mint: TOKEN_A.mint,
                        output_mint: TOKEN_B.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                let expected = (result.in_amount as f64
                    * 10.0
                    * (get_dec_factor((TOKEN_B.decs_count - TOKEN_A.decs_count) as u8) as f64))
                    as u64;
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
                        amount: 1_400_000_000,
                        input_mint: TOKEN_B.mint,
                        output_mint: TOKEN_A.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                println!("In Amount: {}", result.in_amount);
                println!("Out Amount: {}", result.out_amount);

                let expected = (result.in_amount as f64
                    / 10.0
                    / (get_dec_factor((TOKEN_B.decs_count - TOKEN_A.decs_count) as u8) as f64))
                    as u64;
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
                        price: (10.5 * DF) as i64,
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
                        price: (10.6 * DF) as i64,
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
                    // --- Line 2 orders (begin=0, qty: 40k+30k+30k = 100k) ---
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
                    // --- Line 4 orders (begin=3, qty: 25k+25k+25k+25k = 100k) ---
                    Order {
                        qty: 50_000,
                        sum: sum_for(50_000, 4),
                        order_id: 3,
                        line: 4,
                        prev: NULL_ORDER,
                        next: 4,
                        sref: 3,
                        ..Zeroable::zeroed()
                    },
                    Order {
                        qty: 50_000,
                        sum: sum_for(50_000, 4),
                        order_id: 4,
                        line: 4,
                        prev: 3,
                        next: NULL_ORDER,
                        sref: 4,
                        ..Zeroable::zeroed()
                    },
                    // --- Empty orders ---
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
                    .init_community_header(0, &mut accounts_map)
                    .unwrap();
                deriverse.init_amm(
                    1_000_000 * get_dec_factor(TOKEN_A.decs_count as u8),
                    10_000_000 * get_dec_factor(TOKEN_B.decs_count as u8),
                );
                deriverse
                    .init_order_book(
                        &mut accounts_map,
                        lines.clone(),
                        ask_orders,
                        bid_orders,
                        0,
                        2,
                    )
                    .unwrap();

                accounts_map.insert(
                    deriverse.accounts_ctx.a_token_state_acc,
                    default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_token_state_acc,
                    default_account_with_data(
                        bytes_of(&TokenState {
                            address: TOKEN_B.mint,
                            ..Zeroable::zeroed()
                        })
                        .to_vec(),
                    ),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.a_mint,
                    default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
                );
                accounts_map.insert(
                    deriverse.accounts_ctx.b_mint,
                    default_account_with_data(bytes_of(&TokenState::zeroed()).to_vec()),
                );

                deriverse.instr_header.last_px = (10.0 * DF) as i64;

                accounts_map.insert(
                    deriverse.accounts_ctx.instr_header,
                    default_account_with_object(deriverse.instr_header.as_ref()),
                );

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
                        amount: 140_000,
                        input_mint: TOKEN_A.mint,
                        output_mint: TOKEN_B.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                let expected = (result.in_amount as f64
                    * 10.08
                    * (get_dec_factor((TOKEN_B.decs_count - TOKEN_A.decs_count) as u8) as f64))
                    as u64;

                println!("Result: {:?}", result);
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
                    10_500_000 * get_dec_factor(TOKEN_B.decs_count as u8);

                deriverse.amm.a_tokens = 1_000_000 * get_dec_factor(TOKEN_A.decs_count as u8);
                deriverse.amm.b_tokens = 10_500_000 * get_dec_factor(TOKEN_B.decs_count as u8);
                deriverse.amm.k = deriverse.amm.a_tokens as i128 * deriverse.amm.b_tokens as i128;

                //990_000_000
                let result = deriverse
                    .quote(&QuoteParams {
                        amount: 1_400_000_000,
                        input_mint: TOKEN_B.mint,
                        output_mint: TOKEN_A.mint,
                        swap_mode: SwapMode::ExactIn,
                    })
                    .unwrap();

                println!("In Amount: {}", result.in_amount);
                println!("Out Amount: {}", result.out_amount);

                let expected = ((1_400_000_000 as f64 / 10.5)
                    / (get_dec_factor((TOKEN_B.decs_count - TOKEN_A.decs_count) as u8) as f64))
                    as u64;
                println!("Expected: {}", expected);
                let diff = (result.out_amount as i64 - expected as i64).abs();

                assert!(
                    (diff as f64) < (expected as f64 * 0.001),
                    "Calculations are not presize enough: diff ({}) > {}",
                    diff,
                    expected as f64 * 0.000_001
                );
            }
        }
    }

    pub mod rpc_tests {

        use ahash::{HashMap, HashMapExt};
        use bytemuck::bytes_of;
        use drv_models::state::{
            client_primary_account_header::ClientPrimaryAccountHeader,
            token::TokenState,
            types::account_type::{INSTR, SPOT_15M_CANDLES, SPOT_1M_CANDLES, SPOT_DAY_CANDLES},
        };
        use jupiter_amm_interface::{
            Amm, AmmContext, ClockRef, KeyedAccount, SwapAndAccountMetas, SwapParams,
        };
        use once_cell::sync::Lazy;
        use serde_json::to_value;
        use solana_client::{rpc_client::RpcClient, rpc_config::CommitmentConfig};
        use solana_sdk::{
            instruction::Instruction,
            pubkey::Pubkey,
            signature::Keypair,
            signer::{EncodableKey, Signer},
            transaction::Transaction,
        };
        use spl_associated_token_account::get_associated_token_address_with_program_id;

        use crate::{
            custom_sdk::{
                deposit::{DepositBuildContext, DepositContext},
                extend_candles::ExtendCandlesBuilder,
                new_spot_order::{NewSpotOrderBuildContext, NewSpotOrderContext},
                traits::{Context, InstructionBuilder},
            },
            from_swap,
            helper::{get_dec_factor, Helper},
            program_id,
            tests::tests::rpc_tests::config::{TOKEN_A, TOKEN_B},
            Deriverse, InstructionBuilderParams, ParamsWrapper, SwapReferralParams,
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
            use solana_sdk::pubkey::Pubkey;

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

        fn build_key_account(
            ata_init: bool,
            realloc_allowed: bool,
            swap_ref_params: Option<SwapReferralParams>,
        ) -> KeyedAccount {
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
                swap_ref_params: swap_ref_params,
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
            let keyd_account = build_key_account(false, true, None);

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
                        m.insert(accounts_to_update[index], account.clone());
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
            let keyd_account = build_key_account(false, false, None);

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
                        m.insert(accounts_to_update[index], account.clone());
                    }
                    m
                });

            deriverse.update(&accounts_map).unwrap();

            let in_amount = get_dec_factor((deriverse.b_token_state.mask & 0xFF) as u8) as u64 - 2;

            let quote_result = deriverse
                .quote(&jupiter_amm_interface::QuoteParams {
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
                    in_amount,
                    source_mint: TOKEN_A,
                    destination_mint: TOKEN_B,
                    source_token_account: a_ata,
                    destination_token_account: b_ata,
                    token_transfer_authority: CLIENT_B.pubkey(),
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
        }

        #[test]
        fn test_ata_creation() {
            let keyd_account = build_key_account(true, false, None);

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
                        m.insert(accounts_to_update[index], account.clone());
                    }
                    m
                });

            deriverse.update(&accounts_map).unwrap();

            let in_amount = get_dec_factor((deriverse.b_token_state.mask & 0xFF) as u8) as u64 - 2;

            let quote_result = deriverse
                .quote(&jupiter_amm_interface::QuoteParams {
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
                    in_amount,
                    source_mint: TOKEN_A,
                    destination_mint: TOKEN_B,
                    source_token_account: a_ata,
                    destination_token_account: b_ata,
                    token_transfer_authority: CLIENT_C.pubkey(),
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
                (deriverse.b_token_state.mask & 0xFF) as u8,
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

        #[test]
        fn test_swap_ref_params() {
            let ref_taker_ata = get_associated_token_address_with_program_id(
                &CLIENT_A.pubkey(),
                &TOKEN_B,
                &spl_token_interface::id(),
            );

            let keyd_account = build_key_account(
                false,
                false,
                Some(SwapReferralParams {
                    fee_rate_factor: 1.0,
                    client_mint_token_acc: ref_taker_ata,
                }),
            );

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
                        m.insert(accounts_to_update[index], account.clone());
                    }
                    m
                });

            deriverse.update(&accounts_map).unwrap();

            let in_amount = get_dec_factor((deriverse.b_token_state.mask & 0xFF) as u8) as u64 - 2;

            let quote_result = deriverse
                .quote(&jupiter_amm_interface::QuoteParams {
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

            let ref_taker_balance_before = {
                let account = RPC.get_account(&ref_taker_ata);

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
                    in_amount,
                    source_mint: TOKEN_A,
                    destination_mint: TOKEN_B,
                    source_token_account: a_ata,
                    destination_token_account: b_ata,
                    token_transfer_authority: CLIENT_B.pubkey(),
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

            let ref_taker_balance_after = {
                let account = RPC.get_account(&ref_taker_ata).unwrap();

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

            assert!(
                ref_taker_balance_after > ref_taker_balance_before,
                "Ref taker didnt receive any payments"
            );

            println!(
                "Ref payment {}",
                ref_taker_balance_after - ref_taker_balance_before
            )
        }
    }
}
