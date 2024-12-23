use anyhow::Result;
use std::{collections::HashMap, vec};

use bitcoin::{
    absolute, consensus::encode::serialize_hex, opcodes::all::OP_RETURN, script::Builder,
    taproot::TaprootSpendInfo, transaction, Address, Amount, OutPoint, Sequence, Transaction, TxIn,
    TxOut, Txid,
};
use bitcoincore_rpc::{Client, RpcApi};
use itertools::Itertools;
use tracing::info;

use crate::{
    config::{NetworkConfig, FEE_AMOUNT, TX_VERSION},
    ctv_scripts::{create_pool_address, create_withdraw_ctv_hash, spend_ctv},
    AMOUNT_PER_USER, POOL_USERS,
};

pub fn create_entry_pool_withdraw_hashes(
    addresses: &[Address],
    second_pool_addresses: &HashMap<Vec<usize>, TaprootSpendInfo>,
    anchor_addr: &Address,
    config: &NetworkConfig,
    pool_exit_ammount: Amount,
) -> Vec<[u8; 32]> {
    let mut entry_pool_withdraw_hashes = Vec::new();

    for (i, address) in addresses.iter().enumerate() {
        let users: Vec<_> = (0..POOL_USERS).filter(|&x| x != i).collect();

        let key = users.clone();

        let triple_spend_info = &second_pool_addresses[&key];

        let addr = Address::p2tr_tweaked(triple_spend_info.output_key(), config.network);
        let ctv_hash = create_withdraw_ctv_hash(&addr, address, anchor_addr, pool_exit_ammount);
        entry_pool_withdraw_hashes.push(ctv_hash);
    }

    entry_pool_withdraw_hashes
}

pub fn create_exit_pool(
    addresses: &[Address],
    anchor_addr: &Address,
) -> Result<HashMap<Vec<usize>, TaprootSpendInfo>> {
    let exit_pool: Result<HashMap<Vec<usize>, TaprootSpendInfo>> = (0..POOL_USERS)
        .combinations(2)
        .map(|mut combo| {
            combo.sort();
            let i = combo[0];
            let j = combo[1];

            let ctv_hash = create_withdraw_ctv_hash(
                &addresses[i],
                &addresses[j],
                anchor_addr,
                AMOUNT_PER_USER,
            );

            let spend_info = create_pool_address(vec![ctv_hash])?;

            Ok((combo, spend_info))
        })
        .collect();

    exit_pool
}

pub fn create_pool(
    target_pool: &HashMap<Vec<usize>, TaprootSpendInfo>,
    pool_size: usize,
    addresses: &[Address],
    anchor_addr: &Address,
    config: &NetworkConfig,
) -> HashMap<Vec<usize>, TaprootSpendInfo> {
    let mut new_pool: HashMap<Vec<usize>, TaprootSpendInfo> = HashMap::new();

    let num_users = addresses.len();
    info!("Creating addresses for {} user pool \n", pool_size);

    //iterate over all possible spending combinations of users in the pool
    for users in (0..num_users).combinations(pool_size) {
        let mut ctv_hashes = Vec::new();

        for &user in &users {
            let remaining_users: Vec<_> = users.iter().copied().filter(|&u| u != user).collect();
            let spend_info = &target_pool[&remaining_users];

            let withdrawal_address = Address::p2tr_tweaked(spend_info.output_key(), config.network);
            let ctv_hash = create_withdraw_ctv_hash(
                &withdrawal_address,
                &addresses[user],
                anchor_addr,
                (AMOUNT_PER_USER) * remaining_users.len().try_into().unwrap(),
            );

            ctv_hashes.push(ctv_hash);
        }

        let spend_info = create_pool_address(ctv_hashes).unwrap();
        new_pool.insert(users, spend_info);
    }

    new_pool
}

pub fn create_all_pools(
    addresses: &[Address],
    anchor_addr: &Address,
    config: &NetworkConfig,
    pools: &mut Vec<HashMap<Vec<usize>, TaprootSpendInfo>>,
) {
    for pool_num in (1..=POOL_USERS).rev() {
        let users_in_pool = POOL_USERS - pool_num;

        if users_in_pool < 3 {
            continue;
        }

        let previous_pool = pools.last().unwrap();

        let new_pool = create_pool(previous_pool, users_in_pool, addresses, anchor_addr, config);

        pools.push(new_pool);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn send_from_pool(
    pools: &[HashMap<Vec<usize>, TaprootSpendInfo>],
    config: &NetworkConfig,
    pool_num: usize,
    pool_combo: Vec<usize>,
    withdraw_address: Address,
    anchor_addr: &Address,
    pool_exit_ammount: Amount,
    previous_txid: Txid,
    previous_pool_combo: Vec<usize>,
    vout: u32,
) -> String {
    info!(
        "init withdrawal from pool {}, combo: {:?} \n",
        pool_num, pool_combo
    );
    let withdraw_hash = create_withdraw_ctv_hash(
        &Address::p2tr_tweaked(pools[pool_num][&pool_combo].output_key(), config.network),
        &withdraw_address,
        anchor_addr,
        pool_exit_ammount,
    );

    let tx_out = [
        TxOut {
            value: pool_exit_ammount,
            script_pubkey: Address::p2tr_tweaked(
                pools[pool_num][&pool_combo].output_key(),
                config.network,
            )
            .script_pubkey(),
        },
        TxOut {
            value: AMOUNT_PER_USER - FEE_AMOUNT,
            script_pubkey: withdraw_address.script_pubkey(),
        },
        #[cfg(feature = "regtest")]
        TxOut {
            value: FEE_AMOUNT,
            script_pubkey: anchor_addr.script_pubkey(),
        },
    ];

    let inputs = vec![TxIn {
        previous_output: OutPoint {
            txid: previous_txid,
            vout,
        },
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        ..Default::default()
    }];

    let unsigned_tx = Transaction {
        version: transaction::Version(TX_VERSION),
        lock_time: absolute::LockTime::ZERO,
        input: inputs,
        output: tx_out.to_vec(),
    };

    let parent_tx = spend_ctv(
        unsigned_tx,
        pools[pool_num + 1][&previous_pool_combo].clone(),
        withdraw_hash,
    );

    let parent_serialized_tx = serialize_hex(&parent_tx);
    info!(
        "withdrawal from pool {}, parent tx: {} \n",
        pool_num, parent_serialized_tx
    );
    parent_serialized_tx
}

#[allow(clippy::too_many_arguments)]
pub fn process_pool_spend(
    pools: &[HashMap<Vec<usize>, TaprootSpendInfo>],
    config: &NetworkConfig,
    rpc: &Client,
    spender_index: usize,
    addresses: &[Address],
    previous_txid: Txid,
    anchor_addr: &Address,
    mining_address: &Address,
) -> Result<Txid> {
    let pool_amount = (AMOUNT_PER_USER) * (POOL_USERS - spender_index).try_into()?;

    let previous_tx: Transaction = rpc.get_raw_transaction(&previous_txid, None).unwrap();

    let vout = previous_tx
        .output
        .iter()
        .position(|vout| vout.value == pool_amount)
        .unwrap() as u32;

    //create final exit tx for last two users
    if spender_index == POOL_USERS - 2 {
        let last_index = addresses.len() - 1;
        let second_last_index = last_index - 1;

        let last_pool_withdraw_hash = create_withdraw_ctv_hash(
            &addresses[second_last_index],
            &addresses[last_index],
            anchor_addr,
            AMOUNT_PER_USER,
        );

        let last_pool_tx_out = [
            TxOut {
                //the user who waits to leave last gets some extra sats!
                value: AMOUNT_PER_USER,
                script_pubkey: addresses[second_last_index].script_pubkey(),
            },
            TxOut {
                value: AMOUNT_PER_USER - FEE_AMOUNT,
                script_pubkey: addresses[last_index].script_pubkey(),
            },
            #[cfg(feature = "regtest")]
            TxOut {
                value: FEE_AMOUNT,
                script_pubkey: anchor_addr.script_pubkey(),
            },
        ];

        let inputs = vec![TxIn {
            previous_output: OutPoint {
                txid: previous_txid,
                vout,
            },
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            ..Default::default()
        }];

        let unsigned_tx = Transaction {
            version: transaction::Version(TX_VERSION),
            lock_time: absolute::LockTime::ZERO,
            input: inputs,
            output: last_pool_tx_out.to_vec(),
        };

        let final_tx = spend_ctv(
            unsigned_tx,
            pools[0][&vec![second_last_index, last_index]].clone(),
            last_pool_withdraw_hash,
        );

        let serialized_tx = serialize_hex(&final_tx);
        info!("Final exit tx: {} \n", serialized_tx);
        let txid = rpc.send_raw_transaction(serialized_tx)?;
        info!("Final exit txid: {} \n", txid);

        #[cfg(feature = "regtest")]
        let _ = rpc.generate_to_address(1, mining_address);

        #[cfg(feature = "regtest")]
        cpfp_tx(rpc, txid);

        #[cfg(feature = "regtest")]
        let _ = rpc.generate_to_address(1, mining_address);

        return Ok(txid);
    }

    let pool_exit_amount = (AMOUNT_PER_USER) * (POOL_USERS - spender_index - 1).try_into()?;

    let recipient_pool: Vec<usize> = ((spender_index + 1)..POOL_USERS).collect();

    let previous_pool: Vec<usize> = if spender_index == 0 {
        vec![(0)]
    } else {
        (spender_index..POOL_USERS).collect()
    };

    let pool_num = pools.len() - 2 - spender_index;

    info!("previous pool: {:?}", previous_pool);

    let withdraw_parent_serialized_tx = send_from_pool(
        pools,
        config,
        pool_num,
        recipient_pool,
        addresses[spender_index].clone(),
        anchor_addr,
        pool_exit_amount,
        previous_txid,
        previous_pool,
        vout,
    );

    let withdraw_parent_txid = rpc.send_raw_transaction(withdraw_parent_serialized_tx)?;
    info!("{} parent txid: {} \n", spender_index, withdraw_parent_txid);

    //use p2a for fee management on regtest (i was having trouble with v3 transactions propagating on signet)
    #[cfg(feature = "regtest")]
    let _ = rpc.generate_to_address(1, mining_address);

    #[cfg(feature = "regtest")]
    cpfp_tx(rpc, withdraw_parent_txid);

    #[cfg(feature = "regtest")]
    let _ = rpc.generate_to_address(1, mining_address);

    Ok(withdraw_parent_txid)
}

pub fn cpfp_tx(rpc: &Client, parent_txid: Txid) {
    info!("Spending child transaction...");

    let change_address = rpc.get_raw_change_address(None).unwrap();

    let input_size = 68; // SegWit input size
    let output_size = 34; // SegWit output size
    let fixed_overhead = 10; // Version, locktime, and input/output count

    let estimated_tx_size = (input_size) + (output_size) + fixed_overhead;

    let fee_rate = rpc
        .estimate_smart_fee(1, None)
        .unwrap()
        .fee_rate
        .unwrap()
        .to_sat();
    let total_fee = fee_rate * estimated_tx_size / 1000;

    let unspent = rpc.list_unspent(Some(1), None, None, None, None).unwrap();

    let matching_utxo = unspent
        .into_iter()
        .find(|utxo| utxo.amount >= Amount::from_sat(total_fee))
        .unwrap();

    let op_return_script = Builder::new()
        .push_opcode(OP_RETURN)
        .push_slice(b"\xe2\x9a\x93 \xF0\x9F\xA5\xAA \xe2\x9a\x93")
        .into_script();

    let child_spend = Transaction {
        version: transaction::Version(TX_VERSION),
        lock_time: absolute::LockTime::ZERO,
        input: vec![
            TxIn {
                previous_output: OutPoint {
                    txid: parent_txid,
                    vout: 1,
                },
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                ..Default::default()
            },
            TxIn {
                previous_output: OutPoint {
                    txid: matching_utxo.txid,
                    vout: matching_utxo.vout,
                },
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                ..Default::default()
            },
        ],
        output: vec![
            TxOut {
                value: Amount::from_sat(0),
                script_pubkey: op_return_script,
            },
            TxOut {
                value: matching_utxo.amount - Amount::from_sat(total_fee),
                script_pubkey: change_address.assume_checked().script_pubkey(),
            },
        ],
    };

    let child_serialized_tx = serialize_hex(&child_spend);

    info!("\nchild tx: {}", child_serialized_tx);

    let signed_child_tx = rpc
        .sign_raw_transaction_with_wallet(child_serialized_tx, None, None)
        .unwrap();

    let child_txid = rpc.send_raw_transaction(&signed_child_tx.hex).unwrap();

    info!("\nchild txid: {}", child_txid);
}
