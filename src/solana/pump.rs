use blockhash_cache::BLOCKHASH_CACHE;

use crate::solana::{
    constants::{
        ASSOCIATED_TOKEN_PROGRAM, EVENT_AUTHORITY, PUMP_BUY_METHOD, PUMP_SELL_METHOD,
        PUMP_FEE_ADDRESS, PUMP_FUN_PROGRAM, PUMP_GLOBAL_ADDRESS, RENT_PROGRAM,
        SYSTEM_PROGRAM_ID, TOKEN_PROGRAM, TOKEN_2022_PROGRAM, PUMP_FUN_FEE_PROGRAM,
        PUMP_BUY_V2_DISCRIMINATOR, PUMP_SELL_V2_DISCRIMINATOR,
        PUMP_V2_NORMAL_FEE_RECIPIENTS, PUMP_V2_RESERVED_FEE_RECIPIENTS,
        PUMP_V2_BUYBACK_FEE_RECIPIENTS,
        INITIAL_VIRTUAL_SOL_RESERVES, INITIAL_VIRTUAL_TOKEN_RESERVES,
    },
    transaction::{get_jito_tip_pubkey, send_tx},
    util::apply_fee,
    util::{make_compute_budget_ixs, pubkey_to_string, string_to_pubkey},
};
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use rand::seq::SliceRandom;
use rand::thread_rng;
use solana_account_decoder::UiAccountEncoding;
use solana_client::rpc_config::{
    RpcAccountInfoConfig, RpcSendTransactionConfig, RpcTransactionConfig,
};
use solana_sdk::hash::Hash;
use solana_sdk::system_instruction::transfer;
use solana_sdk::transaction::{Transaction, VersionedTransaction};

use std::str::FromStr;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use borsh::{BorshDeserialize, BorshSerialize};

use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature};
use solana_sdk::signer::Signer;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage, UiParsedMessage,
    UiTransactionEncoding,
};
use spl_associated_token_account::get_associated_token_address_with_program_id;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;

// ============================================================================
// V1 Instruction Data (original 12-account layout)
// ============================================================================

#[derive(BorshSerialize)]
pub struct PumpFunSwapInstructionData {
    pub method_id: [u8; 8],
    pub token_amount: u64,
    pub lamports: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct BondingCurveLayout {
    pub blob1: u64,
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub blob4: u64,
    pub complete: bool,
}

#[derive(Debug, BorshSerialize, BorshDeserialize, Clone)]
pub struct BondingCurveLayoutV2 {
    pub discriminator: u64,
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
    pub creator: Pubkey,
    pub is_mayhem_mode: bool,
    pub is_cashback_coin: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PumpTokenData {
    pub address: String,
    pub balance: u64,
    pub image_uri: String,
    pub market_cap: f64,
    pub mint: String,
    pub name: String,
    pub symbol: String,
    pub value: f64,
}

impl BondingCurveLayout {
    pub const LEN: usize = 8 + 8 + 8 + 8 + 8 + 8 + 1;

    pub fn parse(data: &[u8]) -> Result<Self, std::io::Error> {
        Self::try_from_slice(data)
    }
}

impl BondingCurveLayoutV2 {
    /// Decode the 83-byte V2 bonding curve layout
    pub fn decode_v2(data: &[u8]) -> Result<Self> {
        const MIN_LEN: usize = 83;
        if data.len() < MIN_LEN {
            return Err(anyhow!(
                "Bonding curve account too short: expected at least {} bytes, got {}",
                MIN_LEN,
                data.len()
            ));
        }

        let read_u64 = |start: usize| -> u64 {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&data[start..start + 8]);
            u64::from_le_bytes(bytes)
        };

        let creator = Pubkey::try_from(&data[49..81])
            .map_err(|e| anyhow!("Failed to parse creator pubkey: {}", e))?;

        Ok(BondingCurveLayoutV2 {
            discriminator: read_u64(0),
            virtual_token_reserves: read_u64(8),
            virtual_sol_reserves: read_u64(16),
            real_token_reserves: read_u64(24),
            real_sol_reserves: read_u64(32),
            token_total_supply: read_u64(40),
            complete: data[48] != 0,
            creator,
            is_mayhem_mode: data[81] != 0,
            is_cashback_coin: data[82] != 0,
        })
    }
}

// ============================================================================
// Shared Swap types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwapDirection {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwapInType {
    Qty,
    Pct,
}

#[derive(Debug, Clone)]
pub struct SwapConfig {
    pub swap_direction: SwapDirection,
    pub in_type: SwapInType,
    pub amount_in: f64,
    pub slippage: u64,
}

// ============================================================================
// V1 Methods (original 12-account layout - preserved unchanged)
// ============================================================================

pub async fn get_slot_created(rpc_client: &RpcClient, mint: &Pubkey) -> Result<u64> {
    let token_transactions = rpc_client.get_signatures_for_address(mint).await?;

    if token_transactions.is_empty() {
        warn!("No transactions found for mint: {}", mint);
        return Ok(0);
    }

    let first_tx_sig = token_transactions.last().unwrap();
    let first_tx = rpc_client
        .get_transaction_with_config(
            &Signature::from_str(&first_tx_sig.signature).expect("signature"),
            RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                commitment: None,
                max_supported_transaction_version: Some(0),
            },
        )
        .await?;

    Ok(first_tx.slot)
}

pub fn mint_to_pump_accounts(mint: &Pubkey) -> PumpAccounts {
    let (bonding_curve, _) = Pubkey::find_program_address(
        &[b"bonding-curve", mint.as_ref()],
        &Pubkey::from_str(PUMP_FUN_PROGRAM).unwrap(),
    );

    let associated_bonding_curve =
        spl_associated_token_account::get_associated_token_address(&bonding_curve, mint);

    PumpAccounts {
        mint: *mint,
        bonding_curve,
        associated_bonding_curve,
        dev: Pubkey::default(),
        metadata: Pubkey::default(),
    }
}

pub async fn get_bonding_curve(
    rpc_client: &RpcClient,
    bonding_curve_pubkey: Pubkey,
) -> Result<BondingCurveLayout> {
    const MAX_RETRIES: u32 = 5;
    const INITIAL_DELAY_MS: u64 = 200;
    let mut retries = 0;
    let mut delay = Duration::from_millis(INITIAL_DELAY_MS);

    loop {
        match rpc_client
            .get_account_with_config(
                &bonding_curve_pubkey,
                RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    commitment: Some(CommitmentConfig::processed()),
                    data_slice: None,
                    min_context_slot: None,
                },
            )
            .await
        {
            Ok(res) => {
                if let Some(account) = res.value {
                    let data_length = account.data.len();
                    let data: [u8; 49] = account
                        .data
                        .try_into()
                        .map_err(|_| anyhow!("Invalid data length: {}", data_length))?;

                    debug!("Raw bytes: {:?}", data);

                    let layout = BondingCurveLayout {
                        blob1: u64::from_le_bytes(data[0..8].try_into()?),
                        virtual_token_reserves: u64::from_le_bytes(data[8..16].try_into()?),
                        virtual_sol_reserves: u64::from_le_bytes(data[16..24].try_into()?),
                        real_token_reserves: u64::from_le_bytes(data[24..32].try_into()?),
                        real_sol_reserves: u64::from_le_bytes(data[32..40].try_into()?),
                        blob4: u64::from_le_bytes(data[40..48].try_into()?),
                        complete: data[48] != 0,
                    };

                    debug!("Parsed BondingCurveLayout: {:?}", layout);
                    return Ok(layout);
                } else {
                    if retries >= MAX_RETRIES {
                        error!("Max retries reached. Account not found.");
                        return Err(anyhow!("Account not found after max retries"));
                    }
                    warn!(
                        "Attempt {} failed: Account not found. Retrying in {:?}...",
                        retries + 1,
                        delay
                    );
                    sleep(delay).await;
                    retries += 1;
                    delay = Duration::from_millis(INITIAL_DELAY_MS * 2u64.pow(retries));
                    continue;
                }
            }
            Err(e) => {
                if retries >= MAX_RETRIES {
                    error!("Max retries reached. Last error: {}", e);
                    return Err(anyhow!("Max retries reached. Last error: {}", e));
                }
                warn!(
                    "Attempt {} failed: {}. Retrying in {:?}...",
                    retries + 1,
                    e,
                    delay
                );
                sleep(delay).await;
                retries += 1;
                delay = Duration::from_millis(INITIAL_DELAY_MS * 2u64.pow(retries));
            }
        }
    }
}

/// Fetch V2 bonding curve data (83-byte layout with creator, mayhem, cashback)
pub async fn get_bonding_curve_v2(
    rpc_client: &RpcClient,
    bonding_curve_pubkey: Pubkey,
) -> Result<BondingCurveLayoutV2> {
    const MAX_RETRIES: u32 = 5;
    const INITIAL_DELAY_MS: u64 = 200;
    let mut retries = 0;
    let mut delay = Duration::from_millis(INITIAL_DELAY_MS);

    loop {
        match rpc_client
            .get_account_with_config(
                &bonding_curve_pubkey,
                RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    commitment: Some(CommitmentConfig::processed()),
                    data_slice: None,
                    min_context_slot: None,
                },
            )
            .await
        {
            Ok(res) => {
                if let Some(account) = res.value {
                    let curve = BondingCurveLayoutV2::decode_v2(&account.data)?;
                    return Ok(curve);
                } else {
                    if retries >= MAX_RETRIES {
                        return Err(anyhow!("Account not found after max retries"));
                    }
                    sleep(delay).await;
                    retries += 1;
                    delay = Duration::from_millis(INITIAL_DELAY_MS * 2u64.pow(retries));
                    continue;
                }
            }
            Err(e) => {
                if retries >= MAX_RETRIES {
                    return Err(anyhow!("Max retries reached. Last error: {}", e));
                }
                sleep(delay).await;
                retries += 1;
                delay = Duration::from_millis(INITIAL_DELAY_MS * 2u64.pow(retries));
            }
        }
    }
}

pub fn get_pump_token_amount(
    virtual_sol_reserves: u64,
    virtual_token_reserves: u64,
    real_token_reserves: Option<u64>,
    lamports: u64,
) -> Result<u64> {
    let virtual_sol_reserves = virtual_sol_reserves as u128;
    let virtual_token_reserves = virtual_token_reserves as u128;
    let amount_in = lamports as u128;

    let reserves_product = virtual_sol_reserves
        .checked_mul(virtual_token_reserves)
        .ok_or("Overflow in reserves product calculation")
        .map_err(|e| anyhow!(e))?;

    let new_virtual_sol_reserve = virtual_sol_reserves
        .checked_add(amount_in)
        .ok_or("Overflow in new virtual SOL reserve calculation")
        .map_err(|e| anyhow!(e))?;

    let new_virtual_token_reserve = reserves_product
        .checked_div(new_virtual_sol_reserve)
        .ok_or("Division by zero or overflow in new virtual token reserve calculation")
        .map_err(|e| anyhow!(e))?
        .checked_add(1)
        .ok_or("Overflow in new virtual token reserve calculation")
        .map_err(|e| anyhow!(e))?;

    let amount_out = virtual_token_reserves
        .checked_sub(new_virtual_token_reserve)
        .ok_or("Underflow in amount out calculation")
        .map_err(|e| anyhow!(e))?;

    let final_amount_out = if let Some(real_token_reserves) = real_token_reserves {
        std::cmp::min(amount_out, real_token_reserves as u128)
    } else {
        amount_out
    };

    Ok(final_amount_out as u64)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PumpBuyRequest {
    #[serde(
        serialize_with = "pubkey_to_string",
        deserialize_with = "string_to_pubkey"
    )]
    pub mint: Pubkey,
    #[serde(
        serialize_with = "pubkey_to_string",
        deserialize_with = "string_to_pubkey"
    )]
    pub bonding_curve: Pubkey,
    #[serde(
        serialize_with = "pubkey_to_string",
        deserialize_with = "string_to_pubkey"
    )]
    pub associated_bonding_curve: Pubkey,
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub slot: Option<u64>,
}

pub async fn buy_pump_token(
    wallet: &Keypair,
    latest_blockhash: Hash,
    pump_accounts: PumpAccounts,
    token_amount: u64,
    lamports: u64,
    tip: u64,
) -> Result<()> {
    let owner = wallet.pubkey();

    info!("{} buying {} {}", owner, token_amount, pump_accounts.mint);

    let mut ixs = _make_buy_ixs(
        owner,
        pump_accounts.mint,
        pump_accounts.bonding_curve,
        pump_accounts.associated_bonding_curve,
        token_amount,
        apply_fee(lamports),
    )?;

    ixs.push(transfer(&owner, &get_jito_tip_pubkey(), tip));

    let tx = Transaction::new_signed_with_payer(&ixs, Some(&owner), &[wallet], latest_blockhash);

    send_tx(&tx).await?;

    Ok(())
}

pub fn _make_buy_ixs(
    owner: Pubkey,
    mint: Pubkey,
    bonding_curve: Pubkey,
    associated_bonding_curve: Pubkey,
    token_amount: u64,
    lamports: u64,
) -> Result<Vec<Instruction>> {
    let mut ixs = vec![];
    let ata = spl_associated_token_account::get_associated_token_address(&owner, &mint);

    ixs.push(
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &owner,
            &owner,
            &mint,
            &spl_token::id(),
        ),
    );
    ixs.push(make_pump_swap_ix(
        owner,
        mint,
        bonding_curve,
        associated_bonding_curve,
        token_amount,
        lamports,
        ata,
    )?);

    Ok(ixs)
}

async fn _send_tx_standard(
    ixs: Vec<Instruction>,
    wallet: &Keypair,
    rpc_client: &RpcClient,
    owner: Pubkey,
) -> Result<()> {
    let transaction = VersionedTransaction::from(Transaction::new_signed_with_payer(
        &ixs,
        Some(&owner),
        &[wallet],
        BLOCKHASH_CACHE.get_blockhash().await?,
    ));
    let res = rpc_client
        .send_transaction_with_config(
            &transaction,
            RpcSendTransactionConfig {
                skip_preflight: true,
                min_context_slot: None,
                preflight_commitment: Some(CommitmentLevel::Processed),
                max_retries: None,
                encoding: None,
            },
        )
        .await;

    match res {
        Ok(sig) => {
            info!("Transaction sent: {}", sig);
        }
        Err(e) => {
            return Err(e.into());
        }
    }

    Ok(())
}

#[timed::timed(duration(printer = "info!"))]
pub async fn sell_pump_token(
    wallet: &Keypair,
    latest_blockhash: Hash,
    pump_accounts: PumpAccounts,
    token_amount: u64,
) -> Result<()> {
    let owner = wallet.pubkey();

    let ata =
        spl_associated_token_account::get_associated_token_address(&owner, &pump_accounts.mint);

    let mut ixs = vec![];
    let mut compute_budget_ixs = make_compute_budget_ixs(69_000, 69_000);
    let sell_ix = make_pump_sell_ix(owner, pump_accounts, token_amount, ata)?;
    ixs.append(&mut compute_budget_ixs);
    ixs.push(sell_ix);
    ixs.push(transfer(&owner, &get_jito_tip_pubkey(), 30_000));

    let tx = Transaction::new_signed_with_payer(&ixs, Some(&owner), &[wallet], latest_blockhash);

    send_tx(&tx).await?;

    Ok(())
}

/// Interact With Pump.Fun - 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
/// #1 - Global
/// #2 - Fee Recipient: Pump.fun Fee Account (Writable)
/// #3 - Mint
/// #4 - Bonding Curve (Writable)
/// #5 - Associated Bonding Curve (Writable)
/// #6 - Associated Token Account (ATA) (Writable)
/// #7 - User (Writable Signer Fee-Payer)
/// #8 - System Program
/// #9 - Associated Token Program
/// #10 - Token Program
/// #11 - Event Authority
/// #12 - Program: Pump.fun Program
pub fn make_pump_sell_ix(
    owner: Pubkey,
    pump_accounts: PumpAccounts,
    token_amount: u64,
    ata: Pubkey,
) -> Result<Instruction> {
    let accounts: [AccountMeta; 12] = [
        AccountMeta::new_readonly(Pubkey::from_str(PUMP_GLOBAL_ADDRESS)?, false),
        AccountMeta::new(Pubkey::from_str(PUMP_FEE_ADDRESS)?, false),
        AccountMeta::new_readonly(pump_accounts.mint, false),
        AccountMeta::new(pump_accounts.bonding_curve, false),
        AccountMeta::new(pump_accounts.associated_bonding_curve, false),
        AccountMeta::new(ata, false),
        AccountMeta::new(owner, true),
        AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(EVENT_AUTHORITY)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(PUMP_FUN_PROGRAM)?, false),
    ];

    let data = PumpFunSwapInstructionData {
        method_id: PUMP_SELL_METHOD,
        token_amount,
        lamports: 0,
    };

    Ok(Instruction::new_with_borsh(
        Pubkey::from_str(PUMP_FUN_PROGRAM)?,
        &data,
        accounts.to_vec(),
    ))
}

/// Interact With Pump.Fun 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
/// Input Accounts
/// #1 - Global: 4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf
/// #2 - Fee Recipient: Pump.fun Fee Account (Writable)
/// #3 - Mint
/// #4 - Bonding Curve (Writable)
/// #5 - Associated Bonding Curve (Writable)
/// #6 - Associated User Account (Writable) (ATA)
/// #7 - User - owner, sender (Writable, Signer, Fee Payer)
/// #8 - System Program (11111111111111111111111111111111)
/// #9 - Token Program (TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA)
/// #10 - Rent (SysvarRent111111111111111111111111111111111)
/// #11 - Event Authority: Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1
/// #12 - Program: Pump.fun Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
pub fn make_pump_swap_ix(
    owner: Pubkey,
    mint: Pubkey,
    bonding_curve: Pubkey,
    associated_bonding_curve: Pubkey,
    token_amount: u64,
    lamports: u64,
    ata: Pubkey,
) -> Result<Instruction> {
    let accounts: [AccountMeta; 12] = [
        AccountMeta::new_readonly(Pubkey::from_str(PUMP_GLOBAL_ADDRESS)?, false),
        AccountMeta::new(Pubkey::from_str(PUMP_FEE_ADDRESS)?, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(bonding_curve, false),
        AccountMeta::new(associated_bonding_curve, false),
        AccountMeta::new(ata, false),
        AccountMeta::new(owner, true),
        AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(RENT_PROGRAM)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(EVENT_AUTHORITY)?, false),
        AccountMeta::new_readonly(Pubkey::from_str(PUMP_FUN_PROGRAM)?, false),
    ];

    let data = PumpFunSwapInstructionData {
        method_id: PUMP_BUY_METHOD,
        token_amount,
        lamports,
    };

    Ok(Instruction::new_with_borsh(
        Pubkey::from_str(PUMP_FUN_PROGRAM)?,
        &data,
        accounts.to_vec(),
    ))
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct PumpAccounts {
    #[serde(
        serialize_with = "pubkey_to_string",
        deserialize_with = "string_to_pubkey"
    )]
    pub mint: Pubkey,
    #[serde(
        serialize_with = "pubkey_to_string",
        deserialize_with = "string_to_pubkey"
    )]
    pub bonding_curve: Pubkey,
    #[serde(
        serialize_with = "pubkey_to_string",
        deserialize_with = "string_to_pubkey"
    )]
    pub associated_bonding_curve: Pubkey,
    #[serde(
        serialize_with = "pubkey_to_string",
        deserialize_with = "string_to_pubkey"
    )]
    pub dev: Pubkey,
    #[serde(
        serialize_with = "pubkey_to_string",
        deserialize_with = "string_to_pubkey"
    )]
    pub metadata: Pubkey,
}

pub fn parse_pump_accounts(tx: EncodedConfirmedTransactionWithStatusMeta) -> Result<PumpAccounts> {
    if let EncodedTransaction::Json(tx) = &tx.transaction.transaction {
        if let UiMessage::Parsed(UiParsedMessage {
            account_keys,
            instructions: _,
            recent_blockhash: _,
            address_table_lookups: _,
        }) = &tx.message
        {
            debug!("Account keys: {:?}", account_keys);
            if account_keys.len() >= 5 {
                let dev = account_keys[0].pubkey.parse()?;
                let mint = account_keys[1].pubkey.parse()?;
                let bonding_curve = account_keys[3].pubkey.parse()?;
                let associated_bonding_curve = account_keys[4].pubkey.parse()?;
                let metadata = account_keys[5].pubkey.parse()?;

                Ok(PumpAccounts {
                    mint,
                    bonding_curve,
                    associated_bonding_curve,
                    dev,
                    metadata,
                })
            } else {
                Err(anyhow!("Not enough account keys"))
            }
        } else {
            Err(anyhow!("Not a parsed transaction"))
        }
    } else {
        Err(anyhow!("Not a JSON transaction"))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PumpTokenInfo {
    pub associated_bonding_curve: String,
    pub bonding_curve: String,
    pub complete: bool,
    pub created_timestamp: i64,
    pub creator: String,
    pub description: String,
    pub image_uri: String,
    pub inverted: bool,
    pub is_currently_live: bool,
    pub king_of_the_hill_timestamp: i64,
    pub last_reply: i64,
    pub market_cap: f64,
    pub market_id: String,
    pub metadata_uri: String,
    pub mint: String,
    pub name: String,
    pub nsfw: bool,
    pub profile_image: Option<String>,
    pub raydium_pool: String,
    pub reply_count: i32,
    pub show_name: bool,
    pub symbol: String,
    pub telegram: Option<String>,
    pub total_supply: i64,
    pub twitter: Option<String>,
    pub usd_market_cap: f64,
    pub username: Option<String>,
    pub virtual_sol_reserves: i64,
    pub virtual_token_reserves: i64,
    pub website: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IPFSMetadata {
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub image: String,
    #[serde(rename = "showName")]
    pub show_name: Option<bool>,
    #[serde(rename = "createdOn")]
    pub created_on: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
}

pub async fn fetch_metadata(mint: &Pubkey) -> Result<PumpTokenInfo> {
    const MAX_RETRIES: u32 = 3;
    const INITIAL_DELAY_MS: u64 = 200;

    let mut retry_count = 0;
    let mut delay_ms = INITIAL_DELAY_MS;

    loop {
        match fetch_metadata_inner(mint).await {
            Ok(metadata) => {
                info!("Metadata fetched successfully");
                return Ok(metadata);
            }
            Err(e) => {
                if retry_count >= MAX_RETRIES {
                    info!("Failed to fetch metadata after all retries");
                    return Err(e);
                }
                info!(
                    "Retry attempt {} failed: {:?}. Retrying in {} ms...",
                    retry_count + 1,
                    e,
                    delay_ms
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                retry_count += 1;
                delay_ms *= 2;
            }
        }
    }
}

async fn fetch_metadata_inner(mint: &Pubkey) -> Result<PumpTokenInfo> {
    let url = format!("https://frontend-api.pump.fun/coins/{}", mint);
    let res = reqwest::get(&url).await?;
    let data = res.json::<serde_json::Value>().await?;
    Ok(serde_json::from_value(data)?)
}

// ============================================================================
// V2 Methods (enhanced 26+ account layout with fee recipients & volume accumulators)
// ============================================================================

/// Helper: pick a random fee recipient from the given list
fn random_recipient(addresses: &[&str; 8]) -> Result<Pubkey> {
    let mut rng = thread_rng();
    let recipient = addresses
        .choose(&mut rng)
        .ok_or_else(|| anyhow!("No fee recipients configured"))?;
    Ok(Pubkey::from_str(recipient)?)
}

/// Build V2 instruction data (discriminator + token_amount + lamports/threshold)
fn build_pump_v2_data(discriminator: [u8; 8], amount: u64, threshold: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&discriminator);
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&threshold.to_le_bytes());
    data
}

/// Calculate the max amount with slippage applied
fn max_amount_with_slippage(input_amount: u64, slippage_bps: u64) -> u64 {
    input_amount
        .checked_mul(slippage_bps.checked_add(10000).unwrap())
        .unwrap()
        .checked_div(10000)
        .unwrap()
}

/// Get the bonding curve PDA for a given mint
pub fn get_bonding_curve_pda(mint: &Pubkey) -> Result<Pubkey> {
    let program_id = Pubkey::from_str(PUMP_FUN_PROGRAM)?;
    let seeds = [b"bonding-curve".as_ref(), mint.as_ref()];
    let (bonding_curve, _bump) = Pubkey::find_program_address(&seeds, &program_id);
    Ok(bonding_curve)
}

/// Get the correct token program for a given mint
pub async fn get_token_program_for_mint(rpc_client: &RpcClient, mint: &Pubkey) -> Result<Pubkey> {
    match rpc_client.get_account(mint).await {
        Ok(account) => {
            if account.owner.to_string() == TOKEN_2022_PROGRAM {
                Ok(Pubkey::from_str(TOKEN_2022_PROGRAM)?)
            } else {
                Ok(Pubkey::from_str(TOKEN_PROGRAM)?)
            }
        }
        Err(_) => Ok(Pubkey::from_str(TOKEN_PROGRAM)?),
    }
}

/// PDA helpers for volume accumulators
fn get_global_volume_accumulator_pda(pump_program: &Pubkey) -> Result<Pubkey> {
    let seeds = [b"global_volume_accumulator".as_ref()];
    let (pda, _bump) = Pubkey::find_program_address(&seeds, pump_program);
    Ok(pda)
}

fn get_user_volume_accumulator_pda(user: &Pubkey, pump_program: &Pubkey) -> Result<Pubkey> {
    let seeds = [b"user_volume_accumulator".as_ref(), user.as_ref()];
    let (pda, _bump) = Pubkey::find_program_address(&seeds, pump_program);
    Ok(pda)
}

fn get_creator_vault_pda(creator: &Pubkey, pump_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], pump_program).0
}

fn get_event_authority_pda(pump_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"__event_authority"], pump_program).0
}

fn get_sharing_config_pda(mint: &Pubkey, fee_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"sharing-config", mint.as_ref()], fee_program).0
}

fn get_fee_config_pda(pump_program: &Pubkey, fee_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"fee_config", pump_program.as_ref()], fee_program).0
}

/// Build V2 accounts for Pump.fun buy/sell (26+ accounts)
fn build_pump_v2_accounts(
    mint: &Pubkey,
    owner: &Pubkey,
    base_token_program: &Pubkey,
    quote_mint: &Pubkey,
    quote_token_program: &Pubkey,
    pump_program: &Pubkey,
    bonding_curve: &Pubkey,
    curve_state: &BondingCurveLayoutV2,
) -> Result<Vec<AccountMeta>> {
    let fee_program = Pubkey::from_str(PUMP_FUN_FEE_PROGRAM)?;
    let fee_recipient = if curve_state.is_mayhem_mode {
        random_recipient(&PUMP_V2_RESERVED_FEE_RECIPIENTS)?
    } else {
        random_recipient(&PUMP_V2_NORMAL_FEE_RECIPIENTS)?
    };
    let buyback_fee_recipient = random_recipient(&PUMP_V2_BUYBACK_FEE_RECIPIENTS)?;

    let associated_quote_fee_recipient = get_associated_token_address_with_program_id(
        &fee_recipient,
        quote_mint,
        quote_token_program,
    );
    let associated_quote_buyback_fee_recipient = get_associated_token_address_with_program_id(
        &buyback_fee_recipient,
        quote_mint,
        quote_token_program,
    );
    let associated_base_bonding_curve = get_associated_token_address_with_program_id(
        bonding_curve,
        mint,
        base_token_program,
    );
    let associated_quote_bonding_curve = get_associated_token_address_with_program_id(
        bonding_curve,
        quote_mint,
        quote_token_program,
    );
    let associated_base_user = get_associated_token_address_with_program_id(
        owner,
        mint,
        base_token_program,
    );
    let associated_quote_user = get_associated_token_address_with_program_id(
        owner,
        quote_mint,
        quote_token_program,
    );
    let creator_vault = get_creator_vault_pda(&curve_state.creator, pump_program);
    let associated_creator_vault = get_associated_token_address_with_program_id(
        &creator_vault,
        quote_mint,
        quote_token_program,
    );
    let sharing_config = get_sharing_config_pda(mint, &fee_program);
    let global_volume_accumulator = get_global_volume_accumulator_pda(pump_program)?;
    let user_volume_accumulator = get_user_volume_accumulator_pda(owner, pump_program)?;
    let associated_user_volume_accumulator = get_associated_token_address_with_program_id(
        &user_volume_accumulator,
        quote_mint,
        quote_token_program,
    );
    let fee_config = get_fee_config_pda(pump_program, &fee_program);
    let event_authority = get_event_authority_pda(pump_program);

    let mut accounts = vec![
        AccountMeta::new_readonly(Pubkey::from_str(PUMP_GLOBAL_ADDRESS)?, false),
        AccountMeta::new_readonly(*mint, false),
        AccountMeta::new_readonly(*quote_mint, false),
        AccountMeta::new_readonly(*base_token_program, false),
        AccountMeta::new_readonly(*quote_token_program, false),
        AccountMeta::new_readonly(Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM)?, false),
        AccountMeta::new(fee_recipient, false),
        AccountMeta::new(associated_quote_fee_recipient, false),
        AccountMeta::new(buyback_fee_recipient, false),
        AccountMeta::new(associated_quote_buyback_fee_recipient, false),
        AccountMeta::new(*bonding_curve, false),
        AccountMeta::new(associated_base_bonding_curve, false),
        AccountMeta::new(associated_quote_bonding_curve, false),
        AccountMeta::new(*owner, true),
        AccountMeta::new(associated_base_user, false),
        AccountMeta::new(associated_quote_user, false),
        AccountMeta::new(creator_vault, false),
        AccountMeta::new(associated_creator_vault, false),
        AccountMeta::new_readonly(sharing_config, false),
    ];

    // Global volume accumulator
    accounts.push(AccountMeta::new_readonly(global_volume_accumulator, false));

    accounts.extend([
        AccountMeta::new(user_volume_accumulator, false),
        AccountMeta::new(associated_user_volume_accumulator, false),
        AccountMeta::new_readonly(fee_config, false),
        AccountMeta::new_readonly(fee_program, false),
        AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID)?, false),
        AccountMeta::new_readonly(event_authority, false),
        AccountMeta::new_readonly(*pump_program, false),
    ]);

    Ok(accounts)
}

/// Calculate token amount out for buy using virtual reserves
pub fn calculate_buy_token_amount(
    sol_amount_in: u64,
    virtual_sol_reserves: u64,
    virtual_token_reserves: u64,
) -> u64 {
    if sol_amount_in == 0 || virtual_sol_reserves == 0 || virtual_token_reserves == 0 {
        return 0;
    }

    // PumpFun bonding curve formula for buy:
    // tokens_out = (sol_in * virtual_token_reserves) / (virtual_sol_reserves + sol_in)
    let sol_amount_in_u128 = sol_amount_in as u128;
    let virtual_sol_reserves_u128 = virtual_sol_reserves as u128;
    let virtual_token_reserves_u128 = virtual_token_reserves as u128;

    let numerator = sol_amount_in_u128.saturating_mul(virtual_token_reserves_u128);
    let denominator = virtual_sol_reserves_u128.saturating_add(sol_amount_in_u128);

    if denominator == 0 {
        return 0;
    }

    numerator.checked_div(denominator).unwrap_or(0) as u64
}

/// Calculate SOL amount out for sell using virtual reserves
pub fn calculate_sell_sol_amount(
    token_amount_in: u64,
    virtual_sol_reserves: u64,
    virtual_token_reserves: u64,
) -> u64 {
    if token_amount_in == 0 || virtual_sol_reserves == 0 || virtual_token_reserves == 0 {
        return 0;
    }

    // PumpFun bonding curve formula for sell:
    // sol_out = (token_in * virtual_sol_reserves) / (virtual_token_reserves + token_in)
    let token_amount_in_u128 = token_amount_in as u128;
    let virtual_sol_reserves_u128 = virtual_sol_reserves as u128;
    let virtual_token_reserves_u128 = virtual_token_reserves as u128;

    let numerator = token_amount_in_u128.saturating_mul(virtual_sol_reserves_u128);
    let denominator = virtual_token_reserves_u128.saturating_add(token_amount_in_u128);

    if denominator == 0 {
        return 0;
    }

    numerator.checked_div(denominator).unwrap_or(0) as u64
}

/// Calculate price using virtual reserves
pub fn calculate_price_from_virtual_reserves(
    virtual_sol_reserves: u64,
    virtual_token_reserves: u64,
) -> f64 {
    if virtual_token_reserves == 0 {
        return 0.0;
    }
    (virtual_sol_reserves as f64) / (virtual_token_reserves as f64)
}

/// Build a V2 buy transaction from mint address
pub async fn build_buy_v2(
    rpc_client: &RpcClient,
    wallet: &Keypair,
    mint: Pubkey,
    amount_sol: f64,
    slippage_bps: u64,
) -> Result<Transaction> {
    let owner = wallet.pubkey();
    let pump_program = Pubkey::from_str(PUMP_FUN_PROGRAM)?;
    let quote_mint = spl_token::native_mint::ID;
    let quote_token_program = Pubkey::from_str(TOKEN_PROGRAM)?;
    let base_token_program = get_token_program_for_mint(rpc_client, &mint).await?;

    let bonding_curve = get_bonding_curve_pda(&mint)?;
    let curve_data = rpc_client.get_account_data(&bonding_curve).await?;
    let curve_state = BondingCurveLayoutV2::decode_v2(&curve_data)?;

    if curve_state.complete {
        return Err(anyhow!("Bonding curve complete; use PumpSwap"));
    }

    let amount_lamports = spl_token::ui_amount_to_amount(amount_sol, spl_token::native_mint::DECIMALS);
    let token_amount = calculate_buy_token_amount(
        amount_lamports,
        curve_state.virtual_sol_reserves,
        curve_state.virtual_token_reserves,
    );
    if token_amount == 0 {
        return Err(anyhow!("Calculated buy amount is zero"));
    }

    let max_sol_cost = max_amount_with_slippage(amount_lamports, slippage_bps);

    let mut instructions = Vec::new();

    // Get or create ATAs
    let associated_base_user = get_associated_token_address_with_program_id(
        &owner, &mint, &base_token_program,
    );
    let associated_quote_user = get_associated_token_address_with_program_id(
        &owner, &quote_mint, &quote_token_program,
    );

    instructions.push(create_associated_token_account_idempotent(
        &owner, &owner, &mint, &base_token_program,
    ));
    instructions.push(create_associated_token_account_idempotent(
        &owner, &owner, &quote_mint, &quote_token_program,
    ));

    let account_metas = build_pump_v2_accounts(
        &mint,
        &owner,
        &base_token_program,
        &quote_mint,
        &quote_token_program,
        &pump_program,
        &bonding_curve,
        &curve_state,
    )?;

    let swap_instruction = Instruction {
        program_id: pump_program,
        accounts: account_metas,
        data: build_pump_v2_data(PUMP_BUY_V2_DISCRIMINATOR, token_amount, max_sol_cost),
    };
    instructions.push(swap_instruction);

    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&owner),
        &[wallet],
        BLOCKHASH_CACHE.get_blockhash().await?,
    );

    Ok(tx)
}

/// Build a V2 sell transaction from mint address (uses RPC to get token balance)
pub async fn build_sell_v2(
    rpc_client: &RpcClient,
    wallet: &Keypair,
    mint: Pubkey,
    token_amount: u64,
) -> Result<Transaction> {
    let owner = wallet.pubkey();
    let pump_program = Pubkey::from_str(PUMP_FUN_PROGRAM)?;
    let quote_mint = spl_token::native_mint::ID;
    let quote_token_program = Pubkey::from_str(TOKEN_PROGRAM)?;
    let base_token_program = get_token_program_for_mint(rpc_client, &mint).await?;

    let bonding_curve = get_bonding_curve_pda(&mint)?;
    let curve_data = rpc_client.get_account_data(&bonding_curve).await?;
    let curve_state = BondingCurveLayoutV2::decode_v2(&curve_data)?;

    let associated_base_user = get_associated_token_address_with_program_id(
        &owner, &mint, &base_token_program,
    );

    // Calculate minimum SOL output with 0 slippage (market sell)
    let expected_sol = calculate_sell_sol_amount(
        token_amount,
        curve_state.virtual_sol_reserves,
        curve_state.virtual_token_reserves,
    );

    let account_metas = build_pump_v2_accounts(
        &mint,
        &owner,
        &base_token_program,
        &quote_mint,
        &quote_token_program,
        &pump_program,
        &bonding_curve,
        &curve_state,
    )?;

    let sell_instruction = Instruction {
        program_id: pump_program,
        accounts: account_metas,
        data: build_pump_v2_data(PUMP_SELL_V2_DISCRIMINATOR, token_amount, expected_sol),
    };

    let mut instructions = vec![];
    // Ensure WSOL ATA exists for receiving SOL
    let wsol_ata = get_associated_token_address_with_program_id(
        &owner, &quote_mint, &quote_token_program,
    );
    instructions.push(create_associated_token_account_idempotent(
        &owner, &owner, &quote_mint, &quote_token_program,
    ));

    // Add compute budget for sell
    instructions.push(
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(69_000),
    );
    instructions.push(
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(69_000),
    );
    instructions.push(sell_instruction);
    instructions.push(transfer(&owner, &get_jito_tip_pubkey(), 30_000));

    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&owner),
        &[wallet],
        BLOCKHASH_CACHE.get_blockhash().await?,
    );

    Ok(tx)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use solana_sdk::signer::EncodableKey;

    use crate::solana::util::env;

    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_fetch_metadata() {
        let metadata = fetch_metadata(
            &Pubkey::from_str("4cRkQ2dntpusYag6Zmvco8T78WxK9Jqh1eEZJox8pump").expect("parse mint"),
        )
        .await
        .expect("fetch_metadata");

        assert_eq!(metadata.name, "🗿".to_string());
        assert_eq!(metadata.symbol, "🗿".to_string());
        assert_eq!(
            metadata.image_uri,
            "https://cf-ipfs.com/ipfs/QmXn5xkUMxNQ5c5Sfct8rFTq9jNi6jsSHm1yLY2nQyeSke".to_string()
        );
        assert_eq!(
            metadata.twitter,
            Some("https://x.com/thefirstgigasol".to_string())
        );
        assert_eq!(
            metadata.telegram,
            Some("https://t.me/+keptGgOKxN45YWRl".to_string())
        );
        assert_eq!(
            metadata.website,
            Some("https://thefirstgiga.com/".to_string())
        );
    }

    #[test]
    fn test_parse_pump_accounts() {
        let sample_tx = std::fs::read_to_string("mocks/pump_fun_tx.json").expect("read tx");
        let tx: EncodedConfirmedTransactionWithStatusMeta =
            serde_json::from_str(&sample_tx).expect("parse tx");
        let accounts = parse_pump_accounts(tx).expect("parse accounts");
        tracing::debug!(?accounts, "parsed_accounts");
        assert!(accounts.mint.to_string() == "6kPvKNrLqg23mApAvHzMKWohhVdSrA54HvrpYud8pump");
        assert!(
            accounts.bonding_curve.to_string() == "6TGz5VAFF6UpSmTSk9327utugSWJCyVeVVFXDtZnMtNp"
        );
        assert!(
            accounts.associated_bonding_curve.to_string()
                == "4VwNGUif2ubbPjx4YNHmxEH7L4Yt2QFeo8uVTrVC3F68"
        );
        assert!(accounts.dev.to_string() == "2wgo94ZaiUNUkFBSKNaKsUgEANgSdex7gRpFKR39DPzw");
    }

    #[tokio::test]
    #[ignore]
    async fn test_buy_pump_token() {
        let lamports = 690000;
        let pump_accounts = PumpAccounts {
            mint: Pubkey::from_str("5KEDcNGebCcLptWzknqVmPRNLHfiHA9Mm2djVE26pump")
                .expect("parse mint"),
            bonding_curve: Pubkey::from_str("Drhj4djqLsPyiA9qK2YmBngteFba8XhhvuQoBToW6pMS")
                .expect("parse bonding curve"),
            associated_bonding_curve: Pubkey::from_str(
                "7uXq8diH862Dh8NgMHt5Tzsai8SvURhH58rArgxvs7o1",
            )
            .expect("parse associated bonding curve"),
            dev: Pubkey::from_str("Gizxxed4uXCzL7Q8DyALDVoEEDfMkSV7XyUNrPDnPJ9J")
                .expect("parse associated user"),
            metadata: Pubkey::default(),
        };
        let wallet = Keypair::read_from_file(env("FUND_KEYPAIR_PATH")).expect("read wallet");
        let tip = 50_000;
        buy_pump_token(
            &wallet,
            BLOCKHASH_CACHE.get_blockhash().await.unwrap(),
            pump_accounts,
            100_000,
            lamports,
            tip,
        )
        .await
        .expect("buy pump token");
    }

    #[tokio::test]
    async fn test_get_bonding_curve_incomplete() {
        let rpc_client = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        let bonding_curve_pubkey = Pubkey::from_str(
            "Drhj4djqLsPyiA9qK2YmBngteFba8XhhvuQoBToW6pMS",
        )
        .expect("parse bonding curve");

        let bonding_curve = get_bonding_curve(&rpc_client, bonding_curve_pubkey)
            .await
            .expect("get bonding curve");

        tracing::debug!(?bonding_curve, "bonding_curve");

        assert!(!bonding_curve.complete);
        assert_ne!(bonding_curve.virtual_token_reserves, 0);
        assert_ne!(bonding_curve.virtual_sol_reserves, 0);
        assert_ne!(bonding_curve.real_token_reserves, 0);
    }

    #[tokio::test]
    async fn test_get_bonding_curve_complete() {
        let rpc_client = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        let bonding_curve_pubkey = Pubkey::from_str(
            "EB5tQ64HwNjaEoKKYAPkZqndwbULX249EuWSnkjfvR3y",
        )
        .expect("parse bonding curve");

        let bonding_curve = get_bonding_curve(&rpc_client, bonding_curve_pubkey)
            .await
            .expect("get bonding curve");

        tracing::debug!(?bonding_curve, "bonding_curve");

        assert!(bonding_curve.complete);
        assert_eq!(bonding_curve.virtual_token_reserves, 0);
        assert_eq!(bonding_curve.virtual_sol_reserves, 0);
        assert_eq!(bonding_curve.real_token_reserves, 0);
    }

    #[tokio::test]
    async fn test_get_pump_token_amount() {
        let bonding_curve = BondingCurveLayout {
            blob1: 6966180631402821399,
            virtual_token_reserves: 1072964268463317,
            virtual_sol_reserves: 30000999057,
            real_token_reserves: 793064268463317,
            real_sol_reserves: 999057,
            blob4: 1000000000000000,
            complete: false,
        };
        let lamports = 500000;
        let expected_token_amount = 17852389307u64;
        let token_amount = get_pump_token_amount(
            bonding_curve.virtual_sol_reserves,
            bonding_curve.virtual_token_reserves,
            Some(bonding_curve.real_token_reserves),
            lamports,
        )
        .expect("get token amount");
        let low_thresh = 0.9 * expected_token_amount as f64;
        let high_thresh = 1.1 * expected_token_amount as f64;
        let token_amount = token_amount as f64;
        assert!(token_amount >= low_thresh);
        assert!(token_amount <= high_thresh);
    }

    #[test]
    fn test_bonding_curve_v2_decode() {
        let data = [
            0x17, 0x17, 0x17, 0x17, 0x17, 0x17, 0x17, 0x17, // discriminator
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // virtual_token_reserves
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // virtual_sol_reserves
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // real_token_reserves
            0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // real_sol_reserves
            0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // token_total_supply
            0x00, // complete = false
            // creator pubkey (32 bytes)
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x00, // is_mayhem_mode = false
            0x00, // is_cashback_coin = false
        ];
        let curve = BondingCurveLayoutV2::decode_v2(&data).unwrap();
        assert_eq!(curve.discriminator, 0x1717171717171717);
        assert_eq!(curve.virtual_token_reserves, 0x0807060504030201);
        assert_eq!(curve.complete, false);
        assert_eq!(curve.is_mayhem_mode, false);
        assert_eq!(curve.is_cashback_coin, false);
    }
}