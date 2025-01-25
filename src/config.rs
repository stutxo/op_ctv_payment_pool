use bitcoin::{Amount, Network};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use std::path::PathBuf;
use tracing::info;

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

    pub fn get_env_var(var_name: &str, default_value: &str) -> String {
        std::env::var(var_name).unwrap_or_else(|_| default_value.to_string())
    }

    pub fn bitcoin_rpc(&self) -> Client {
        let bitcoin_rpc_user = Self::get_env_var("BITCOIN_RPC_USER", "NA");
        let bitcoin_rpc_pass = Self::get_env_var("BITCOIN_RPC_PASS", "NA");
        let bitcoin_rpc_cookie_path = Self::get_env_var("BITCOIN_RPC_COOKIE_PATH", "NA");

        let bitcoin_rpc_url =
            format!("http://localhost:{}/wallet/{}", self.port, self.wallet_name,);

        info!("wallet name in use: {} \n", self.wallet_name);

        let bitcoin_rpc_user_clone = bitcoin_rpc_user.clone();
        let bitcoin_rpc_pass_clone = bitcoin_rpc_pass.clone();

        let auth;
        let bitcoin_rpc;
        let test_auth;

        //Check if user/pass or cookie is found in enviroment variables. 
        //If both are found, UserPass will be used first.
        if bitcoin_rpc_user != "NA" || bitcoin_rpc_pass != "NA" {
            test_auth = Auth::UserPass(bitcoin_rpc_user, bitcoin_rpc_pass);
        } else if bitcoin_rpc_cookie_path == "NA" {
            panic!("No User/Pass or Cookie found!");
        } else {
            test_auth = Auth::CookieFile(PathBuf::from(&bitcoin_rpc_cookie_path));
        }

        let test_bitcoin_rpc = Client::new(&bitcoin_rpc_url, test_auth).unwrap();

        //Test UserPass, If it fails, then fall back to CookieFile.
        match test_bitcoin_rpc.get_best_block_hash() {
            Ok(value) => {
                //UserPass test succeeded
                auth = Auth::UserPass(bitcoin_rpc_user_clone, bitcoin_rpc_pass_clone);
                value
            }
            Err(_) => {
                let auth_with_cookie = Auth::CookieFile(PathBuf::from(&bitcoin_rpc_cookie_path));
                info!("UserPass would not authenticate, trying CookieFile now");
                if bitcoin_rpc_cookie_path == "NA" { 
                    info!("No CookieFile found!");
                    panic!()
                } 

                match Client::new(&bitcoin_rpc_url, auth_with_cookie) {
                    Ok(alternative_bitcoin_rpc) => {
                        match alternative_bitcoin_rpc.get_best_block_hash() {
                            Ok(alternative_value) => {
                                info!("Cookie File is good!");
                                auth = Auth::CookieFile(PathBuf::from(&bitcoin_rpc_cookie_path));
                                alternative_value
                            }
                            Err(e) => {
                                panic!("Cookie File would not authenticate: {}", e)
                            }
                        }
                    }
                    Err(e) => {
                        panic!("Error with User Pass or CookieFile: {}", e)
                    }
                }
            }
        };

        bitcoin_rpc = Client::new(&bitcoin_rpc_url, auth).unwrap();

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
