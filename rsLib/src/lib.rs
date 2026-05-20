use std::str::FromStr;
use std::time::Duration;

use bdk::bitcoin::{Address, Network};
use bdk::miniscript::Segwitv0;
use bdk::{
    blockchain::{electrum::ElectrumBlockchain, Blockchain},
    database::MemoryDatabase,
    electrum_client::Client,
    keys::{
        bip39::{Language, Mnemonic, WordCount},
        DerivableKey, ExtendedKey, GeneratableKey, GeneratedKey,
    },
    template::Bip84,
    wallet::AddressIndex,
    FeeRate, KeychainKind, SignOptions, SyncOptions, Wallet,
};

use jni::objects::{JClass, JString};
use jni::sys::{jdouble, jfloat, jlong, jstring};
use jni::JNIEnv;

// ── Error ──────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("Mnemónico inválido")]
    InvalidMnemonic,
    #[error("Error de sincronización: {0}")]
    SyncError(String),
    #[error("Fondos insuficientes")]
    InsufficientFunds,
    #[error("Error construyendo/firmando transacción: {0}")]
    TxBuildError(String),
    #[error("Error obteniendo precio: {0}")]
    PriceError(String),
}

// ── Constants ──────────────────────────────────────────────────────

const NETWORK: Network = Network::Testnet; // Only Test, Changed in case of use Mainnet
const ELECTRUM_URL: &str = "ssl://electrum.blockstream.info:60002";

// ── Wallet helpers (efímeras en memoria) ───────────────────────────

fn build_wallet(mnemonic: &str) -> Result<Wallet<MemoryDatabase>, WalletError> {
    let mnemonic = Mnemonic::parse_in(Language::English, mnemonic)
        .map_err(|_| WalletError::InvalidMnemonic)?;

    let xkey: ExtendedKey<Segwitv0> = mnemonic
        .into_extended_key()
        .map_err(|_| WalletError::InvalidMnemonic)?;

    let xprv = xkey
        .into_xprv(NETWORK)
        .ok_or(WalletError::InvalidMnemonic)?;

    let wallet = Wallet::new(
        Bip84(xprv.clone(), KeychainKind::External),
        Some(Bip84(xprv, KeychainKind::Internal)),
        NETWORK,
        MemoryDatabase::default(),
    )
    .map_err(|e| WalletError::TxBuildError(e.to_string()))?;

    Ok(wallet)
}

fn sync_wallet(wallet: &Wallet<MemoryDatabase>) -> Result<(), WalletError> {
    let client = Client::new(ELECTRUM_URL).map_err(|e| WalletError::SyncError(e.to_string()))?;
    let blockchain = ElectrumBlockchain::from(client);
    wallet
        .sync(&blockchain, SyncOptions::default())
        .map_err(|e| WalletError::SyncError(e.to_string()))?;
    Ok(())
}

// ── Public API ─────────────────────────────────────────────────────

pub fn create_wallet() -> String {
    let generated: GeneratedKey<Mnemonic, Segwitv0> =
        Mnemonic::generate((WordCount::Words12, Language::English))
            .expect("failed to generate mnemonic");
    generated.to_string()
}

pub fn get_address(mnemonic: &str) -> Result<String, WalletError> {
    let wallet = build_wallet(mnemonic)?;
    let addr = wallet
        .get_address(AddressIndex::New)
        .map_err(|e| WalletError::TxBuildError(e.to_string()))?;
    Ok(addr.to_string())
}

pub fn get_balance(mnemonic: &str) -> Result<u64, WalletError> {
    let wallet = build_wallet(mnemonic)?;
    sync_wallet(&wallet)?;
    let balance = wallet
        .get_balance()
        .map_err(|e| WalletError::SyncError(e.to_string()))?;
    Ok(balance.get_total())
}

pub fn validate_mnemonic(mnemonic: &str) -> Result<(), String> {
    Mnemonic::parse_in(Language::English, mnemonic)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn send_bitcoin(
    mnemonic: &str,
    to_address: &str,
    amount_satoshis: u64,
    fee_rate_sat_per_vbyte: f32,
) -> Result<String, WalletError> {
    let wallet = build_wallet(mnemonic)?;

    let client = Client::new(ELECTRUM_URL).map_err(|e| WalletError::SyncError(e.to_string()))?;
    let blockchain = ElectrumBlockchain::from(client);

    wallet
        .sync(&blockchain, SyncOptions::default())
        .map_err(|e| WalletError::SyncError(e.to_string()))?;

    let addr = Address::from_str(to_address)
        .map_err(|e| WalletError::TxBuildError(e.to_string()))?
        .require_network(NETWORK)
        .map_err(|e| WalletError::TxBuildError(e.to_string()))?;

    let script = addr.script_pubkey();
    let fee_rate = FeeRate::from_sat_per_vb(fee_rate_sat_per_vbyte);

    let mut tx_builder = wallet.build_tx();
    tx_builder
        .add_recipient(script, amount_satoshis)
        .fee_rate(fee_rate);

    let (mut psbt, _details) = tx_builder
        .finish()
        .map_err(|e| WalletError::TxBuildError(e.to_string()))?;

    wallet
        .sign(&mut psbt, SignOptions::default())
        .map_err(|e| WalletError::TxBuildError(e.to_string()))?;

    let tx = psbt.extract_tx();
    let txid = tx.txid();

    blockchain
        .broadcast(&tx)
        .map_err(|e| WalletError::TxBuildError(e.to_string()))?;

    Ok(txid.to_string())
}

// ── Precios y conversión ──────────────────────────────────────────

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(15))
        .build()
}

pub fn get_btc_price(currency: &str) -> f64 {
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies={}",
        currency.to_lowercase()
    );
    match http_agent().get(&url).call() {
        Ok(response) => {
            let json: serde_json::Value = response.into_json().unwrap_or_default();
            json["bitcoin"][currency.to_lowercase()]
                .as_f64()
                .unwrap_or(0.0)
        }
        Err(_) => 0.0,
    }
}

pub fn convert_currency(amount: f64, from_currency: &str, to_currency: &str) -> f64 {
    let from = from_currency.to_lowercase();
    let to = to_currency.to_lowercase();

    if from == to {
        return amount;
    }

    if from == "btc" || to == "btc" {
        let fiat = if from == "btc" { &to } else { &from };
        let price = get_btc_price(fiat);
        if price == 0.0 {
            return 0.0;
        }
        if from == "btc" {
            return amount * price;
        } else {
            return amount / price;
        }
    }

    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies={},{}",
        from, to
    );
    let json: serde_json::Value = match http_agent().get(&url).call() {
        Ok(response) => response.into_json().unwrap_or_default(),
        Err(_) => return 0.0,
    };
    let price_from = json["bitcoin"][&from].as_f64().unwrap_or(0.0);
    let price_to = json["bitcoin"][&to].as_f64().unwrap_or(0.0);
    if price_from == 0.0 || price_to == 0.0 {
        return 0.0;
    }
    let btc_amount = amount / price_from;
    btc_amount * price_to
}

// ── JNI Bridge ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn Java_mx_angelDL_wallet_core_BtcWalletCore_validateMnemonic(
    mut env: JNIEnv,
    _class: JClass,
    j_mnemonic: JString,
) -> jstring {
    let mnemonic = match jni_string(&mut env, &j_mnemonic) {
        Ok(s) => s,
        Err(e) => return jni_output(&mut env, &format!("Error:{}", e)),
    };
    match validate_mnemonic(&mnemonic) {
        Ok(()) => jni_output(&mut env, ""),
        Err(e) => jni_output(&mut env, &format!("Error:{}", e)),
    }
}

fn jni_string(env: &mut JNIEnv, input: &JString) -> Result<String, String> {
    env.get_string(input)
        .map(|s| s.into())
        .map_err(|e| format!("JNI: no se pudo leer el string: {}", e))
}

fn jni_output(env: &mut JNIEnv, s: &str) -> jstring {
    env.new_string(s)
        .map(|js| js.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn Java_mx_angelDL_wallet_core_BtcWalletCore_createWallet(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let phrase = create_wallet();
    jni_output(&mut env, &phrase)
}

#[no_mangle]
pub extern "C" fn Java_mx_angelDL_wallet_core_BtcWalletCore_getAddress(
    mut env: JNIEnv,
    _class: JClass,
    j_mnemonic: JString,
) -> jstring {
    let mnemonic = match jni_string(&mut env, &j_mnemonic) {
        Ok(s) => s,
        Err(e) => return jni_output(&mut env, &format!("Error:{}", e)),
    };
    match get_address(&mnemonic) {
        Ok(addr) => jni_output(&mut env, &addr),
        Err(e) => jni_output(&mut env, &format!("Error:{}", e)),
    }
}

#[no_mangle]
pub extern "C" fn Java_mx_angelDL_wallet_core_BtcWalletCore_getBalance(
    mut env: JNIEnv,
    _class: JClass,
    j_mnemonic: JString,
) -> jlong {
    let mnemonic = match jni_string(&mut env, &j_mnemonic) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    get_balance(&mnemonic).unwrap_or(0) as jlong
}

#[no_mangle]
pub extern "C" fn Java_mx_angelDL_wallet_core_BtcWalletCore_sendBitcoin(
    mut env: JNIEnv,
    _class: JClass,
    j_mnemonic: JString,
    j_to_address: JString,
    j_amount_sat: jlong,
    j_fee_rate: jfloat,
) -> jstring {
    let mnemonic = match jni_string(&mut env, &j_mnemonic) {
        Ok(s) => s,
        Err(e) => return jni_output(&mut env, &format!("Error:{}", e)),
    };
    let to = match jni_string(&mut env, &j_to_address) {
        Ok(s) => s,
        Err(e) => return jni_output(&mut env, &format!("Error:{}", e)),
    };
    match send_bitcoin(&mnemonic, &to, j_amount_sat as u64, j_fee_rate) {
        Ok(tx_hex) => jni_output(&mut env, &tx_hex),
        Err(e) => jni_output(&mut env, &format!("Error:{}", e)),
    }
}

#[no_mangle]
pub extern "C" fn Java_mx_angelDL_wallet_core_BtcWalletCore_getBtcPrice(
    mut env: JNIEnv,
    _class: JClass,
    j_currency: JString,
) -> jdouble {
    let currency = match jni_string(&mut env, &j_currency) {
        Ok(s) => s,
        Err(_) => return -1.0,
    };
    get_btc_price(&currency) as jdouble
}

#[no_mangle]
pub extern "C" fn Java_mx_angelDL_wallet_core_BtcWalletCore_convertCurrency(
    mut env: JNIEnv,
    _class: JClass,
    j_amount: jdouble,
    j_from: JString,
    j_to: JString,
) -> jdouble {
    let amount = j_amount as f64;
    let from = match jni_string(&mut env, &j_from) {
        Ok(s) => s,
        Err(_) => return -1.0,
    };
    let to = match jni_string(&mut env, &j_to) {
        Ok(s) => s,
        Err(_) => return -1.0,
    };
    convert_currency(amount, &from, &to) as jdouble
}
