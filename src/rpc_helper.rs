use std::collections::HashMap;

use anyhow::Result;
use bitcoin::{Address, Txid};
use bitcoincore_rpc::{
    json::{self, GetTransactionResultDetail},
    jsonrpc::serde_json,
    Client, RpcApi,
};
use serde_json::json;
use tracing::info;

use crate::{
    config::{NetworkConfig, DEFAULT_FEE_RATE, DUST_AMOUNT, INIT_WALLET_AMOUNT_FEE},
    AMOUNT_PER_USER, POOL_USERS,
};

pub fn send_funding_transaction(rpc: &Client, config: &NetworkConfig) -> bitcoin::Txid {
    let addresses: Vec<Address> = (0..POOL_USERS)
        .map(|_| {
            rpc.get_new_address(None, None)
                .unwrap()
                .require_network(config.network)
                .unwrap()
        })
        .collect();

    let mut amounts = serde_json::Map::new();
    let total_btc = AMOUNT_PER_USER.to_btc() + INIT_WALLET_AMOUNT_FEE.to_btc();
    let total_btc_str = format!("{:.8}", total_btc);

    for address in addresses {
        amounts.insert(address.to_string(), json!(total_btc_str));
    }

    let minconf = 1;
    let comment = "Fund init user wallets";

    let txid: String = rpc
        .call(
            "sendmany",
            &["".into(), json!(amounts), minconf.into(), comment.into()],
        )
        .expect("Failed to execute sendmany command");

    info!("Fund init user wallets TXID: {} \n", txid);
    txid.parse().expect("Failed to parse txid")
}

pub fn simulate_psbt_signing(
    rpc: &Client,
    init_wallets_txid: Txid,
    pool_0_addr: &Address,
) -> Result<Txid> {
    let matching_vouts = get_vouts_from_init_tx(rpc, &init_wallets_txid);
    let num_users = matching_vouts.len() as u64;

    // Calculate fee

    let num_outputs = num_users + 1;
    let input_size = 68; // SegWit input size
    let output_size = 34; // SegWit output size
    let fixed_overhead = 10; // Version, locktime, and input/output count

    let estimated_tx_size = (num_users * input_size) + (num_outputs * output_size) + fixed_overhead;

    let fee_rate = rpc
        .estimate_smart_fee(1, None)
        .ok()
        .and_then(|estimate| estimate.fee_rate.map(|rate| rate.to_sat()))
        .unwrap_or(DEFAULT_FEE_RATE);
    let total_fee = fee_rate * estimated_tx_size / 1000;
    let fee_per_user = total_fee / num_users;

    info!(
        "Fee estimation: {} sat/vB, Est. size: {} vB, Total fee: {} sats, Per user: {} sats \n",
        fee_rate, estimated_tx_size, total_fee, fee_per_user
    );

    let mut psbt_output = HashMap::new();
    psbt_output.insert(
        pool_0_addr.to_string(),
        (AMOUNT_PER_USER) * POOL_USERS.try_into().unwrap(),
    );

    let initial_psbt = rpc.create_psbt(&[], &psbt_output, None, None).unwrap();
    let mut current_psbt = initial_psbt.clone();

    for output in &matching_vouts {
        let input = json::CreateRawTransactionInput {
            txid: init_wallets_txid,
            vout: output.vout,
            sequence: None,
        };

        let change_address = rpc.get_raw_change_address(None).unwrap();

        if output.amount.to_unsigned().unwrap()
            < AMOUNT_PER_USER + bitcoin::Amount::from_sat(fee_per_user) + DUST_AMOUNT
        {
            panic!(
                "insufficient funds, amount in output{}, fee:{}, increase amount in INIT_WALLET_AMOUNT const",
                output.amount.to_sat(), (bitcoin::Amount::from_sat(fee_per_user) + AMOUNT_PER_USER).to_sat()
            );
        }

        let change_amount = (output.amount
            - AMOUNT_PER_USER.to_signed().unwrap()
            - bitcoin::Amount::from_sat(fee_per_user).to_signed().unwrap())
        .to_unsigned()
        .unwrap();

        let mut change = HashMap::new();
        change.insert(change_address.assume_checked().to_string(), change_amount);

        let input_psbt = rpc.create_psbt(&[input], &change, None, None).unwrap();
        current_psbt = rpc.join_psbt(&[current_psbt, input_psbt]).unwrap();

        let processed_psbt = rpc
            .wallet_process_psbt(&current_psbt, Some(true), None, None)
            .unwrap();

        current_psbt = processed_psbt.psbt;

        info!(
            "User {} added and signed their input to PSBT, fee contribution: {} sats \n",
            output.vout, fee_per_user
        );
    }

    let finalize_psbt = rpc.finalize_psbt(&current_psbt, None).unwrap();
    let pool_funding_txid = rpc.send_raw_transaction(&finalize_psbt.hex.clone().unwrap())?;
    Ok(pool_funding_txid)
}

pub fn get_vouts_from_init_tx(rpc: &Client, txid: &Txid) -> Vec<GetTransactionResultDetail> {
    let tx = rpc.get_transaction(txid, None).unwrap();
    let tx_details = tx.details;

    let matched_vouts: Vec<GetTransactionResultDetail> = tx_details
        .iter()
        .filter(|vout| {
            vout.amount
                == (AMOUNT_PER_USER + INIT_WALLET_AMOUNT_FEE)
                    .to_signed()
                    .unwrap()
        })
        .cloned()
        .collect();

    matched_vouts
}
