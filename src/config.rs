use bitcoin::{Amount, Network};
use bitcoincore_rpc::{Auth, Client, Error, RpcApi};
use std::path::PathBuf;
use tracing::{error, info};

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

    pub fn bitcoin_rpc(&self) -> Result<Client, Error> {
        let bitcoin_rpc_user = Self::get_env_var("BITCOIN_RPC_USER", "NA");
        let bitcoin_rpc_pass = Self::get_env_var("BITCOIN_RPC_PASS", "NA");
        let bitcoin_rpc_cookie_path = Self::get_env_var("BITCOIN_RPC_COOKIE_PATH", "NA");

        let bitcoin_rpc_url =
            format!("http://localhost:{}/wallet/{}", self.port, self.wallet_name,);

        info!("wallet name in use: {} \n", self.wallet_name);

        //Check if user/pass or cookie is found in enviroment variables.
        //If both are found, UserPass will be tried first.
        let test_auth = if bitcoin_rpc_user != "NA" && bitcoin_rpc_pass != "NA" {
            Auth::UserPass(bitcoin_rpc_user.to_string(), bitcoin_rpc_pass.to_string())
        } else if bitcoin_rpc_cookie_path != "NA" {
            Auth::CookieFile(PathBuf::from(bitcoin_rpc_cookie_path.clone()))
        } else {
            error!("No User/Pass or Cookie found!");
            return Err(Error::InvalidCookieFile);
        };

        let test_bitcoin_rpc_userpass = Client::new(&bitcoin_rpc_url, test_auth)?;

        let bitcoin_rpc = match test_bitcoin_rpc_userpass.get_best_block_hash() {
            Ok(_) => {
                //UserPass authentication succeeded
                test_bitcoin_rpc_userpass
            }
            Err(e) => {
                if &bitcoin_rpc_cookie_path == "NA" {
                    error!("UserPass failed and no cookie file was found!");
                    return Err(Error::ReturnedError(e.to_string()));
                }

                info!("UserPass would not authenticate, trying CookieFile now");

                match Client::new(
                    &bitcoin_rpc_url,
                    Auth::CookieFile(PathBuf::from(&bitcoin_rpc_cookie_path)),
                ) {
                    Ok(test_bitcoin_rpc_cookiefile) => {
                        match test_bitcoin_rpc_cookiefile.get_best_block_hash() {
                            Ok(_) => {
                                info!("Cookie File authentication succeeded!");
                                test_bitcoin_rpc_cookiefile
                            }

                            Err(e) => {
                                //Both Userpass and cookie have failed
                                error!("Cookie File authenication failed!: {}", e);
                                return Err(Error::InvalidCookieFile {});
                            }
                        }
                    }
                    Err(e) => {
                        error!("{}", e);
                        return Err(Error::InvalidCookieFile {});
                    }
                }
            }
        };

        #[cfg(feature = "regtest")]
        let regtest_wallet = bitcoin_rpc.create_wallet(&self.wallet_name, None, None, None, None);
        #[cfg(feature = "regtest")]
        if regtest_wallet.is_ok() {
            info!("regtest wallet created \n")
        }

        let _ = bitcoin_rpc.load_wallet(&self.wallet_name);

        Ok(bitcoin_rpc)
    }
}
