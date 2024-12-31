use anyhow::Result;
use bitcoin::Address;
use bitcoincore_rpc::RpcApi;
use config::{NetworkConfig, AMOUNT_PER_USER, POOL_USERS};
use ctv_scripts::create_pool_address;
use pools::{
    create_all_pools, create_entry_pool_withdraw_hashes, create_exit_pool, process_pool_spend,
};
use rpc_helper::{send_funding_transaction, simulate_psbt_signing};
use std::{collections::HashMap, str::FromStr};
use tracing::info;

mod config;
mod ctv_scripts;
mod pools;
mod rpc_helper;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    if POOL_USERS < 3 {
        panic!("Pool must have at least 3 users");
    }

    let config = NetworkConfig::new();
    let rpc = config.bitcoin_rpc();

    let mining_address = rpc
        .get_new_address(None, None)?
        .require_network(config.network)?;

    #[cfg(feature = "regtest")]
    if rpc.get_balance(None, None)? < (AMOUNT_PER_USER) * POOL_USERS.try_into()? {
        let _ = rpc.generate_to_address(101, &mining_address);
    }

    let anchor_addr = Address::from_str(config.fee_anchor_addr)?.require_network(config.network)?;

    info!("Creating pool with {} users \n", POOL_USERS);

    let withdraw_addresses: Vec<Address> = (0..POOL_USERS)
        .map(|_| {
            rpc.get_new_address(None, None)
                .unwrap()
                .require_network(config.network)
                .unwrap()
        })
        .collect();

    let init_wallets_txid = send_funding_transaction(&rpc, &config);

    #[cfg(feature = "regtest")]
    let _ = rpc.generate_to_address(1, &mining_address);

    ////////////////////////////////////////////////////////////////////////////
    /////////////////////////////CREATE LAST POOL //////////////////////////////
    ////////////////////////////////////////////////////////////////////////////
    let mut pools = Vec::new();
    //The last pool will always be the same, regardless of how many users are in the pool (it will allow 2 users to withdraw)
    let exit_pool = create_exit_pool(&withdraw_addresses, &anchor_addr)?;
    pools.push(exit_pool);

    /////////////////////////////////////////////////////////////////////////////
    /////////////////////////////CREATE ALL OTHER POOLS//////////////////////////
    ////////////////////////////////////////////////////////////////////////////

    create_all_pools(&withdraw_addresses, &anchor_addr, &config, &mut pools);

    let total_taproot_spend_info: usize = pools.iter().map(|pool| pool.len()).sum();

    info!(
        "total taproot addresses across all pools: {} for {} users \n",
        total_taproot_spend_info, POOL_USERS
    );

    ////////////////////////////////////////////////////////////////////////////
    //////////////////////CREATE FIRST POOL/////////////////////////////////////
    ////////////////////////////////////////////////////////////////////////////

    let pool_0 = create_entry_pool_withdraw_hashes(
        &withdraw_addresses,
        pools.last().unwrap(),
        &anchor_addr,
        &config,
        (AMOUNT_PER_USER) * (POOL_USERS - 1).try_into()?,
    );
    let pool_0_spend_info = create_pool_address(pool_0.clone())?;
    let mut pool_0_map = HashMap::new();
    pool_0_map.insert(vec![0], pool_0_spend_info.clone());
    pools.push(pool_0_map);

    //////////////////////////////////////////////////////////////////////////////////
    /////////////////////////////FUND POOL WITH PSBT//////////////////////////////////
    /////////////////////////////////////////////////////////////////////////////////

    //the first pools address
    let pool_0_addr = Address::p2tr_tweaked(pool_0_spend_info.output_key(), config.network);

    //here we will simulate the pool psbt funding transaction
    let pool_funding_txid = simulate_psbt_signing(&rpc, init_wallets_txid, &pool_0_addr)?;

    info!("PSBT Pool funding txid: {} \n", pool_funding_txid);

    #[cfg(feature = "regtest")]
    let _ = rpc.generate_to_address(1, &mining_address);

    ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    ////we are going to test spending, but for the PoC we will just spend in the order of addresses so for example, for a 10 user pool it will be///
    /////////////////////Alice -> Bob -> Carol -> Danny -> Eve -> Frank -> George -> Helen -> Igor && Jao///////////////////////////////////////////
    ////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

    let mut current_txid = pool_funding_txid;
    for i in 0..=(POOL_USERS - 2) {
        current_txid = process_pool_spend(
            &pools,
            &config,
            &rpc,
            i,
            &withdraw_addresses,
            current_txid,
            &anchor_addr,
            &mining_address,
        )?;
    }

    Ok(())
}
