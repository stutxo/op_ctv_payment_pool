use bitcoin::{
    consensus::Encodable,
    hashes::{sha256, Hash},
    key::{Keypair, Secp256k1},
    opcodes::all::OP_NOP4,
    script::Builder,
    taproot::{LeafVersion, TaprootBuilder, TaprootSpendInfo},
    Address, Amount, Opcode, ScriptBuf, Sequence, Transaction, TxOut, XOnlyPublicKey,
};

use anyhow::Result;

use crate::{
    config::{FEE_AMOUNT, TX_VERSION},
    AMOUNT_PER_USER,
};

const OP_SECURETHEBAG: Opcode = OP_NOP4;

pub fn ctv_script(ctv_hash: [u8; 32]) -> ScriptBuf {
    Builder::new()
        .push_slice(ctv_hash)
        .push_opcode(OP_SECURETHEBAG)
        .into_script()
}

pub fn calc_ctv_hash(outputs: &[TxOut], timeout: Option<u32>) -> [u8; 32] {
    let mut buffer = Vec::new();
    buffer.extend(TX_VERSION.to_le_bytes()); // version
    buffer.extend(0_i32.to_le_bytes()); // locktime
    buffer.extend(1_u32.to_le_bytes()); // inputs len

    let seq = if let Some(timeout_value) = timeout {
        sha256::Hash::hash(&Sequence(timeout_value).0.to_le_bytes())
    } else {
        sha256::Hash::hash(&Sequence::ENABLE_RBF_NO_LOCKTIME.0.to_le_bytes())
    };
    buffer.extend(seq.to_byte_array()); // sequences

    let outputs_len = outputs.len() as u32;
    buffer.extend(outputs_len.to_le_bytes()); // outputs len

    let mut output_bytes: Vec<u8> = Vec::new();
    for o in outputs {
        o.consensus_encode(&mut output_bytes).unwrap();
    }
    buffer.extend(sha256::Hash::hash(&output_bytes).to_byte_array()); // outputs hash

    buffer.extend(0_u32.to_le_bytes()); // inputs index

    let hash = sha256::Hash::hash(&buffer);
    hash.to_byte_array()
}

pub fn create_pool_address(ctv_hashes: Vec<[u8; 32]>) -> Result<TaprootSpendInfo> {
    let secp = Secp256k1::new();

    let key_pair = Keypair::new(&secp, &mut rand::thread_rng());
    //TO DO: replace this with a MuSig key for happy spend :)
    // Random unspendable XOnlyPublicKey provided for internal key. Will replace this with combination of all pool users pubkeys (MuSig)
    //in a real implentation we would most likely use nostr to communicate the funding PSBT, so you could also user their npubs to create the MuSig key
    let (unspendable_pubkey, _parity) = XOnlyPublicKey::from_keypair(&key_pair);

    let num_scripts = ctv_hashes.len();
    let depths = calculate_depths(num_scripts);

    let mut builder = TaprootBuilder::new();

    for (depth, hash) in depths.iter().zip(ctv_hashes.iter()) {
        let script = ctv_script(*hash);
        builder = builder.add_leaf((*depth).try_into()?, script)?;
    }

    let taproot_spend_info = builder.finalize(&secp, unspendable_pubkey).unwrap();

    Ok(taproot_spend_info)
}

fn calculate_depths(num_scripts: usize) -> Vec<usize> {
    if num_scripts == 0 {
        return vec![];
    }

    let height = (num_scripts as f64).log2().ceil() as usize;
    let mut depths = vec![height; num_scripts];

    let next_power = 2usize.pow(height as u32);
    let excess = next_power - num_scripts;

    for i in 0..excess {
        if let Some(index) = num_scripts.checked_sub(1).and_then(|x| x.checked_sub(i)) {
            depths[index] = height - 1;
        }
    }

    depths
}

pub fn create_withdraw_ctv_hash(
    pool_addr: &Address,
    withdraw_addr: &Address,
    anchor_addr: &Address,
    pool_exit_amount: Amount,
) -> [u8; 32] {
    let ctv_tx_out = [
        TxOut {
            value: pool_exit_amount,
            script_pubkey: pool_addr.script_pubkey(),
        },
        TxOut {
            value: AMOUNT_PER_USER - FEE_AMOUNT,
            script_pubkey: withdraw_addr.script_pubkey(),
        },
        #[cfg(feature = "regtest")]
        TxOut {
            value: FEE_AMOUNT,
            script_pubkey: anchor_addr.script_pubkey(),
        },
    ];

    calc_ctv_hash(&ctv_tx_out, None)
}

pub fn spend_ctv(
    mut unsigned_tx: Transaction,
    taproot_spend_info: TaprootSpendInfo,
    ctv_hash: [u8; 32],
) -> Transaction {
    let ctv_script = ctv_script(ctv_hash);

    //TO DO - add a signature here for the spends, for now it works ok as an example,
    //or maybe we dont even need them, it just means anyone with the descriptor can spend these...

    for input in unsigned_tx.input.iter_mut() {
        let script_ver = (ctv_script.clone(), LeafVersion::TapScript);
        let ctrl_block = taproot_spend_info.control_block(&script_ver).unwrap();

        input.witness.push(script_ver.0.into_bytes());
        input.witness.push(ctrl_block.serialize());
    }
    unsigned_tx
}
