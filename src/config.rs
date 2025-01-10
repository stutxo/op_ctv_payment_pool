use std::env;

use bitcoin::{Amount, Network};

use bitcoincore_rpc::{Auth, Client, RpcApi};
use tracing::info;
use std::path::PathBuf;

// https://bitcoinops.org/en/bitcoin-core-28-wallet-integration-guide/
// mainnet: bc1pfeessrawgf
// regtest: bcrt1pfeesnyr2tx
// testnet: tb1pfees9rn5nz

//this could be 240 for P2A but we set for 1000 for now so it works on signet with hard coded fee
pub const FEE_AMOUNT: Amount = Amount::from_sat(1000);
pub const DUST_AMOUNT: Amount = Amount::from_sat(546);
pub const DEFAULT_FEE_RATE: u64 = 5000;

//send a bit more so we can cover the fees for the pool funding transaction
pub const INIT_WALLET_AMOUNT_FEE: Amount = Amount::from_sat(2000);

//must be 3 or more. You can do maybe up to 20, but it will take a very long time to compute all taproot addresses
pub const POOL_USERS: usize = 10;

//has to be more than FEE_AMOUNT + DUST_AMOUNT
pub const AMOUNT_PER_USER: Amount = Amount::from_sat(11000);

#[cfg(feature = "signet")]
pub const TX_VERSION: i32 = 2;

#[cfg(feature = "regtest")]
pub const TX_VERSION: i32 = 3;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub network: Network,
    pub port: &'static str,
    pub fee_anchor_addr: &'static str,
    pub wallet_name: String,
}

impl NetworkConfig {
    #[allow(clippy::needless_return)]
    pub fn new() -> Self {
        #[cfg(feature = "regtest")]
        {
            return Self {
                network: Network::Regtest,
                port: "18443",
                fee_anchor_addr: "bcrt1pfeesnyr2tx",
                wallet_name: "simple_ctv".to_string(),
            };
        }
        #[cfg(feature = "signet")]
        {
            let wallet_name = env::var("SIGNET_WALLET").expect("SIGNET_WALLET env var not set");
            info!("wallet name: {} \n", wallet_name);
            return Self {
                network: Network::Signet,
                port: "38332",
                fee_anchor_addr: "tb1pfees9rn5nz",
                wallet_name,
            };
        }
        //wen mainnet
    }

    pub fn bitcoin_rpc(&self) -> Client {
        let bitcoin_rpc_cookie_path =
            env::var("BITCOIN_RPC_COOKIE_PATH").expect("BITCOIN_RPC_COOKIE_PATH env var not set");
        //let bitcoin_rpc_user =
            //env::var("BITCOIN_RPC_USER").expect("BITCOIN_RPC_USER env var not set");
        //let bitcoin_rpc_pass =
            //env::var("BITCOIN_RPC_PASS").expect("BITCOIN_RPC_PASS env var not set");

        let bitcoin_rpc_url =
            format!("http://localhost:{}/wallet/{}", self.port, self.wallet_name,);

        info!("wallet name in use: {} \n", self.wallet_name);

        let cookie_path = PathBuf::from(bitcoin_rpc_cookie_path);

        let bitcoin_rpc = Client::new(
            &bitcoin_rpc_url,
            Auth::CookieFile(cookie_path),
        )
        .unwrap();

        #[cfg(feature = "regtest")]
        let regtest_wallet = bitcoin_rpc.create_wallet(&self.wallet_name, None, None, None, None);
        #[cfg(feature = "regtest")]
        if regtest_wallet.is_ok() {
            info!("regtest wallet created \n")
        }

        let _ = bitcoin_rpc.load_wallet(&self.wallet_name);

        bitcoin_rpc
    }
}
