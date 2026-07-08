pub const DRVS_SEED: &[u8; 5] = b"ndxnt";

pub mod account_type {

    use serde::{Deserialize, Serialize};

    pub const CLIENT_COMMUNITY: u32 = 35;
    pub const CLIENT_DRV: u32 = 32;
    pub const CLIENT_PRIMARY: u32 = 31;
    pub const COMMUNITY: u32 = 34;

    pub const HOLDER: u32 = 1;
    pub const ROOT: u32 = 2;
    pub const INSTR: u32 = 7;

    pub const SPOT_ASK_ORDERS: u32 = 17;
    pub const SPOT_ASKS_TREE: u32 = 15;
    pub const SPOT_BID_ORDERS: u32 = 16;
    pub const SPOT_BIDS_TREE: u32 = 14;

    pub const SPOT_CLIENT_INFOS: u32 = 12;
    pub const SPOT_LINES: u32 = 18;
    pub const SPOT_MAPS: u32 = 10;
    pub const TOKEN: u32 = 4;
    pub const PERP_ASK_ORDERS: u32 = 36;
    pub const PERP_ASKS_TREE: u32 = 37;
    pub const PERP_BID_ORDERS: u32 = 38;
    pub const PERP_BIDS_TREE: u32 = 39;
    pub const PERP_CLIENT_INFOS: u32 = 41;
    pub const PERP_CLIENT_INFOS2: u32 = 42;
    pub const PERP_CLIENT_INFOS3: u32 = 43;
    pub const PERP_CLIENT_INFOS4: u32 = 44;
    pub const PERP_CLIENT_INFOS5: u32 = 45;
    pub const PERP_LINES: u32 = 46;
    pub const PERP_MAPS: u32 = 47;
    pub const PERP_LONG_PX_TREE: u32 = 48;
    pub const PERP_SHORT_PX_TREE: u32 = 49;
    pub const PERP_REBALANCE_TIME_TREE: u32 = 50;
    pub const PRIVATE_CLIENTS: u32 = 51;
    pub const VM_CLIENT: u32 = 52;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[repr(u32)]
    pub enum AccountType {
        Holder = 1,
        Root = 2,
        Token = 4,
        Instr = 7,
        SpotMaps = 10,
        SpotClientAccounts = 11,
        SpotClientInfos = 12,
        SpotClientInfos2 = 13,
        SpotBidsTree = 14,
        SpotAsksTree = 15,
        SpotBidOrders = 16,
        SpotAskOrders = 17,
        SpotLines = 18,
        Spot1MCandles = 19,
        Spot15MCandles = 20,
        SpotDayCandles = 21,
        ClientPrimary = 31,
        Community = 34,
        ClientCommunity = 35,
        PerpAskOrders = 36,
        PerpAsksTree = 37,
        PerpBidOrders = 38,
        PerpBidsTree = 39,
        PerpClientAccounts = 40,
        PerpClientInfos = 41,
        PerpClientInfos2 = 42,
        PerpClientInfos3 = 43,
        PerpClientInfos4 = 44,
        PerpClientInfos5 = 45,
        PerpLines = 46,
        PerpMaps = 47,
        PerpLongPxTree = 48,
        PerpShortPxTree = 49,
        PerpRebalanceTimeTree = 50,
        PrivateClients = 51,
        VmClient = 52,
        KaminoClient = 53,
        ProgramTokenAccount,
        DrvsAuthority,
    }

    impl std::fmt::Display for AccountType {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}({})", self, (*self) as u32)
        }
    }

    #[test]
    fn some_test() {
        let account_type = AccountType::Community;
        assert_eq!(format!("{}", account_type), "Community(34)".to_string());
    }
}

pub mod nulls {
    pub const NULL_NODE: u32 = 0xFFFFFFFF;
    pub const NULL_ORDER: u32 = 0xFFFF;
    pub const NULL_THREAD: u32 = 0xFFFF;
    pub const NULL_INDEX: usize = 0xFFFF;
    pub const NULL_CLIENT: u32 = 0xFFFFFF;
    pub const NULL_INSTR: u32 = 0xFFFFFFF;
    pub const NULL_TOKEN: u32 = 0xFFFFFFF;
}

pub const FEE_RATE_STEP: f64 = 0.0005;
