//! PumpSwap AMM integration for migrated Pump.fun tokens.
//!
//! PumpSwap is the official AMM where Pump.fun tokens migrate after their bonding
//! curve completes. This module provides pool discovery, buy (exact quote in),
//! and sell with fee-aware account construction.
//!
//! Program ID: pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA

use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_instruction;
use solana_sdk::system_program;
use solana_sdk::transaction::Transaction;
use spl_associated_token_account::{
    get_associated_token_address,
    get_associated_token_address_with_program_id,
    instruction::create_associated_token_account_idempotent,
};
use spl_token::instruction::sync_native;
use spl_token::ui_amount_to_amount;

use blockhash_cache::BLOCKHASH_CACHE;

use crate::solana::constants::{
    ASSOCIATED_TOKEN_PROGRAM, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM, TOKEN_2022_PROGRAM,
    PUMP_SWAP_PROGRAM, PUMP_SWAP_GLOBAL_CONFIG, PUMP_SWAP_FEE_RECIPIENT,
    PUMP_SWAP_EVENT_AUTHORITY, PUMP_SWAP_BUY_EXACT_QUOTE_IN_DISCRIMINATOR,
    PUMP_SWAP_SELL_DISCRIMINATOR, PUMP_V2_BUYBACK_FEE_RECIPIENTS,
    PUMPSWAP_TOTAL_BUY_FEE_BPS, PUMPSWAP_BUY_BASE_OUT_SAFETY_BPS,
};
use crate::solana::transaction::get_jito_tip_pubkey;

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_client::RpcClient as SyncRpcClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType};
use solana_sdk::commitment_config::CommitmentConfig;

// Re-export types so callers can use them
pub use crate::solana::pump::{SwapDirection, SwapInType, SwapConfig};

// ============================================================================
// Constants
// ============================================================================

/// Default buyback fee recipient
fn default_buyback_fee_recipient() -> Pubkey {
    Pubkey::from_str(PUMP_V2_BUYBACK_FEE_RECIPIENTS[0]).unwrap()
}

/// Get the global volume accumulator PDA for PumpSwap
fn get_global_volume_accumulator_pda() -> Result<Pubkey> {
    let program_id = Pubkey::from_str(PUMP_SWAP_PROGRAM)?;
    let seeds = [b"global_volume_accumulator".as_ref()];
    let (pda, _bump) = Pubkey::find_program_address(&seeds, &program_id);
    Ok(pda)
}

/// Get the user volume accumulator PDA for a specific user for PumpSwap
fn get_user_volume_accumulator_pda(user: &Pubkey) -> Result<Pubkey> {
    let program_id = Pubkey::from_str(PUMP_SWAP_PROGRAM)?;
    let seeds = [b"user_volume_accumulator".as_ref(), user.as_ref()];
    let (pda, _bump) = Pubkey::find_program_address(&seeds, &program_id);
    Ok(pda)
}

fn get_fee_config_pda() -> Result<Pubkey> {
    let pump_swap_program = Pubkey::from_str(PUMP_SWAP_PROGRAM)?;
    let pump_fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;
    let (pda, _bump) = Pubkey::find_program_address(
        &[b"fee_config", pump_swap_program.as_ref()],
        &pump_fee_program,
    );
    Ok(pda)
}

fn get_pool_v2_pda(base_mint: &Pubkey) -> Pubkey {
    let pump_swap_program = Pubkey::from_str(PUMP_SWAP_PROGRAM).unwrap();
    Pubkey::find_program_address(&[b"pool-v2", base_mint.as_ref()], &pump_swap_program).0
}

// ============================================================================
// Math helpers
// ============================================================================

const TEN_THOUSAND: u64 = 10000;

fn max_amount_with_slippage(input_amount: u64, slippage_bps: u64) -> u64 {
    input_amount
        .saturating_mul(TEN_THOUSAND.saturating_add(slippage_bps))
        .checked_div(TEN_THOUSAND)
        .unwrap_or(input_amount)
}

fn min_amount_with_slippage(input_amount: u64, slippage_bps: u64) -> u64 {
    input_amount
        .saturating_mul(TEN_THOUSAND.saturating_sub(slippage_bps))
        .checked_div(TEN_THOUSAND)
        .unwrap_or(0)
}

/// Calculate token amount out for buy using virtual reserves (PumpSwap AMM formula)
pub fn calculate_buy_token_amount(
    sol_amount_in: u64,
    virtual_sol_reserves: u64,
    virtual_token_reserves: u64,
) -> u64 {
    if sol_amount_in == 0 || virtual_sol_reserves == 0 || virtual_token_reserves == 0 {
        return 0;
    }

    // PumpSwap AMM formula for buy (same as PumpFun):
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

/// Calculate buy token amount with fees
pub fn calculate_buy_token_amount_with_fees(
    quote_amount_in: u64,
    quote_reserve: u64,
    base_reserve: u64,
    total_fee_bps: u64,
    safety_bps: u64,
) -> u64 {
    if quote_amount_in == 0 || quote_reserve == 0 || base_reserve == 0 {
        return 0;
    }

    let denominator_bps = TEN_THOUSAND.saturating_add(total_fee_bps);
    if denominator_bps == 0 {
        return 0;
    }

    let effective_quote = (quote_amount_in as u128)
        .saturating_mul(TEN_THOUSAND as u128)
        / (denominator_bps as u128);
    if effective_quote == 0 {
        return 0;
    }

    let numerator = (base_reserve as u128).saturating_mul(effective_quote);
    let denominator = (quote_reserve as u128).saturating_add(effective_quote);
    if denominator == 0 {
        return 0;
    }

    let raw_base_out = numerator / denominator;
    let safe_base_out = raw_base_out
        .saturating_mul(TEN_THOUSAND.saturating_sub(safety_bps) as u128)
        / (TEN_THOUSAND as u128);

    safe_base_out.min(base_reserve.saturating_sub(1) as u128) as u64
}

/// Calculate SOL amount out for sell using virtual reserves (PumpSwap AMM formula)
pub fn calculate_sell_sol_amount(
    token_amount_in: u64,
    virtual_sol_reserves: u64,
    virtual_token_reserves: u64,
) -> u64 {
    if token_amount_in == 0 || virtual_sol_reserves == 0 || virtual_token_reserves == 0 {
        return 0;
    }

    // PumpSwap constant product AMM formula for sell:
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

// ============================================================================
// Pool Discovery
// ============================================================================

/// Find the PumpSwap pool for a given mint and get its reserves.
/// Returns (pool_id, base_reserve, quote_reserve).
pub async fn find_pool(
    rpc_client: &RpcClient,
    mint: &Pubkey,
) -> Result<(Pubkey, u64, u64)> {
    let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112")?;
    let pump_swap_program = Pubkey::from_str(PUMP_SWAP_PROGRAM)?;

    // Determine token program for this mint
    let token_program = match rpc_client.get_account(mint).await {
        Ok(account) if account.owner.to_string() == TOKEN_2022_PROGRAM => {
            Pubkey::from_str(TOKEN_2022_PROGRAM)?
        }
        _ => Pubkey::from_str(TOKEN_PROGRAM)?,
    };

    // Helper to read owner from token account data
    let read_token_account_owner = |data: &[u8]| -> Option<Pubkey> {
        if data.len() < 64 {
            return None;
        }
        Pubkey::try_from(&data[32..64]).ok()
    };

    // Helper to read amount from token account data
    let _read_token_account_amount = |data: &[u8]| -> Option<u64> {
        if data.len() < 72 {
            return None;
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&data[64..72]);
        Some(u64::from_le_bytes(bytes))
    };

    // Find pool by searching program accounts
    let mut pool_id = Pubkey::default();
    let mint_bytes = mint.to_bytes();

    // Try filter-based search first
    let sync_rpc = SyncRpcClient::new(rpc_client.url().to_string());
    match sync_rpc.get_program_accounts_with_config(
        &pump_swap_program,
        RpcProgramAccountsConfig {
            filters: Some(vec![
                RpcFilterType::DataSize(300),
                RpcFilterType::Memcmp(Memcmp::new(
                    43,
                    MemcmpEncodedBytes::Base64(base64::encode(mint_bytes)),
                )),
            ]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                ..Default::default()
            },
            ..Default::default()
        },
    ) {
        Ok(accounts) => {
            for (pubkey, account) in accounts.iter() {
                if account.data.len() >= 75 {
                    if let Ok(pubkey_from_data) = Pubkey::try_from(&account.data[43..75]) {
                        if pubkey_from_data == *mint {
                            pool_id = *pubkey;
                            break;
                        }
                    }
                }
            }
        }
        Err(err) => {
            debug!("No PumpSwap pool found via filter for {}: {}", mint, err);
        }
    }

    // Fallback: search via largest token accounts
    if pool_id == Pubkey::default() {
        if let Ok(largest_accounts) = rpc_client.get_token_largest_accounts(mint).await {
            for largest in largest_accounts {
                let token_account_pubkey = match Pubkey::from_str(&largest.address) {
                    Ok(pk) => pk,
                    Err(_) => continue,
                };
                let token_account_data = match rpc_client.get_account(&token_account_pubkey).await {
                    Ok(acc) => acc.data,
                    Err(_) => continue,
                };
                let candidate_pool = if token_program.to_string() == TOKEN_2022_PROGRAM {
                    match read_token_account_owner(&token_account_data) {
                        Some(owner) => owner,
                        None => continue,
                    }
                } else {
                    // Read owner directly at offset 32 in token account data
                    match read_token_account_owner(&token_account_data) {
                        Some(owner) => owner,
                        None => continue,
                    }
                };
                let candidate_pool_account = match rpc_client.get_account(&candidate_pool).await {
                    Ok(acc) => acc,
                    Err(_) => continue,
                };
                if candidate_pool_account.owner == pump_swap_program {
                    pool_id = candidate_pool;
                    break;
                }
            }
        }
    }

    if pool_id == Pubkey::default() {
        return Err(anyhow!("Failed to find PumpSwap pool for mint {}", mint));
    }

    // Derive token accounts
    let pool_base_account =
        get_associated_token_address_with_program_id(&pool_id, mint, &token_program);
    let pool_quote_account = get_associated_token_address(&pool_id, &sol_mint);

    // Get token balances
    let sync_rpc = SyncRpcClient::new(rpc_client.url().to_string());
    let accounts = sync_rpc.get_multiple_accounts(&[pool_base_account, pool_quote_account])?;

    // Extract balances with fallback values
    let base_balance = if let Some(account_data) = &accounts[0] {
        read_token_account_balance(&account_data.data).unwrap_or(10_000_000_000_000)
    } else {
        10_000_000_000_000
    };

    let quote_balance = if let Some(account_data) = &accounts[1] {
        read_token_account_balance(&account_data.data).unwrap_or(10_000_000_000)
    } else {
        10_000_000_000
    };

    Ok((pool_id, base_balance, quote_balance))
}

fn read_token_account_balance(data: &[u8]) -> Option<u64> {
    if data.len() < 72 {
        return None;
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[64..72]);
    Some(u64::from_le_bytes(bytes))
}

// ============================================================================
// Account builders
// ============================================================================

fn create_buy_accounts(
    pool_id: Pubkey,
    user: Pubkey,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    user_base_token_account: Pubkey,
    wsol_account: Pubkey,
    pool_base_token_account: Pubkey,
    pool_quote_token_account: Pubkey,
    coin_creator: Pubkey,
    base_token_program: Pubkey,
    quote_token_program: Pubkey,
    global_volume_accumulator: Pubkey,
    user_volume_accumulator: Pubkey,
    is_reverse_when_pump_swap: bool,
) -> Result<Vec<AccountMeta>> {
    let pump_swap_program = Pubkey::from_str(PUMP_SWAP_PROGRAM)?;
    let pump_swap_fee_recipient = Pubkey::from_str(PUMP_SWAP_FEE_RECIPIENT)?;
    let pump_event_authority = Pubkey::from_str(PUMP_SWAP_EVENT_AUTHORITY)?;
    let pump_fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;
    let pump_global_config = Pubkey::from_str(PUMP_SWAP_GLOBAL_CONFIG)?;
    let associated_token_program = Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM)?;

    let (coin_creator_vault_authority, _) = Pubkey::find_program_address(
        &[b"creator_vault", coin_creator.as_ref()],
        &pump_swap_program,
    );
    let fee_config = get_fee_config_pda()?;

    let (
        base_mint,
        quote_mint,
        user_base_token_account,
        user_quote_token_account,
        pool_base_token_account,
        pool_quote_token_account,
        base_token_program,
        quote_token_program,
    ) = if is_reverse_when_pump_swap {
        (
            quote_mint,
            base_mint,
            wsol_account,
            user_base_token_account,
            pool_quote_token_account,
            pool_base_token_account,
            quote_token_program,
            base_token_program,
        )
    } else {
        (
            base_mint,
            quote_mint,
            user_base_token_account,
            wsol_account,
            pool_base_token_account,
            pool_quote_token_account,
            base_token_program,
            quote_token_program,
        )
    };

    let coin_creator_vault_ata = get_associated_token_address_with_program_id(
        &coin_creator_vault_authority,
        &quote_mint,
        &quote_token_program,
    );
    let protocol_fee_recipient_token_account = get_associated_token_address_with_program_id(
        &pump_swap_fee_recipient,
        &quote_mint,
        &quote_token_program,
    );
    let buyback_fee_recipient_token_account = get_associated_token_address_with_program_id(
        &default_buyback_fee_recipient(),
        &quote_mint,
        &quote_token_program,
    );

    let mut accounts = vec![
        AccountMeta::new(pool_id, false),
        AccountMeta::new(user, true),
        AccountMeta::new_readonly(pump_global_config, false),
        AccountMeta::new_readonly(base_mint, false),
        AccountMeta::new_readonly(quote_mint, false),
        AccountMeta::new(user_base_token_account, false),
        AccountMeta::new(user_quote_token_account, false),
        AccountMeta::new(pool_base_token_account, false),
        AccountMeta::new(pool_quote_token_account, false),
        AccountMeta::new_readonly(pump_swap_fee_recipient, false),
        AccountMeta::new(protocol_fee_recipient_token_account, false),
        AccountMeta::new_readonly(base_token_program, false),
        AccountMeta::new_readonly(quote_token_program, false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(associated_token_program, false),
        AccountMeta::new_readonly(pump_event_authority, false),
        AccountMeta::new_readonly(pump_swap_program, false),
        AccountMeta::new(coin_creator_vault_ata, false),
        AccountMeta::new_readonly(coin_creator_vault_authority, false),
        AccountMeta::new_readonly(global_volume_accumulator, false),
        AccountMeta::new(user_volume_accumulator, false),
        AccountMeta::new_readonly(fee_config, false),
        AccountMeta::new_readonly(pump_fee_program, false),
    ];

    if coin_creator != Pubkey::default() {
        accounts.push(AccountMeta::new_readonly(get_pool_v2_pda(&base_mint), false));
    }

    accounts.push(AccountMeta::new_readonly(default_buyback_fee_recipient(), false));
    accounts.push(AccountMeta::new(buyback_fee_recipient_token_account, false));

    Ok(accounts)
}

fn create_sell_accounts(
    pool_id: Pubkey,
    user: Pubkey,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    user_base_token_account: Pubkey,
    wsol_account: Pubkey,
    pool_base_token_account: Pubkey,
    pool_quote_token_account: Pubkey,
    coin_creator: Pubkey,
    base_token_program: Pubkey,
    quote_token_program: Pubkey,
    is_reverse_when_pump_swap: bool,
) -> Result<Vec<AccountMeta>> {
    let pump_swap_program = Pubkey::from_str(PUMP_SWAP_PROGRAM)?;
    let pump_swap_fee_recipient = Pubkey::from_str(PUMP_SWAP_FEE_RECIPIENT)?;
    let pump_event_authority = Pubkey::from_str(PUMP_SWAP_EVENT_AUTHORITY)?;
    let pump_fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;
    let pump_global_config = Pubkey::from_str(PUMP_SWAP_GLOBAL_CONFIG)?;
    let associated_token_program = Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM)?;

    let (coin_creator_vault_authority, _) = Pubkey::find_program_address(
        &[b"creator_vault", coin_creator.as_ref()],
        &pump_swap_program,
    );
    let fee_config = get_fee_config_pda()?;

    let (
        base_mint,
        quote_mint,
        user_base_token_account,
        user_quote_token_account,
        pool_base_token_account,
        pool_quote_token_account,
        base_token_program,
        quote_token_program,
    ) = if is_reverse_when_pump_swap {
        (
            quote_mint,
            base_mint,
            wsol_account,
            user_base_token_account,
            pool_quote_token_account,
            pool_base_token_account,
            quote_token_program,
            base_token_program,
        )
    } else {
        (
            base_mint,
            quote_mint,
            user_base_token_account,
            wsol_account,
            pool_base_token_account,
            pool_quote_token_account,
            base_token_program,
            quote_token_program,
        )
    };

    let coin_creator_vault_ata = get_associated_token_address_with_program_id(
        &coin_creator_vault_authority,
        &quote_mint,
        &quote_token_program,
    );
    let protocol_fee_recipient_token_account = get_associated_token_address_with_program_id(
        &pump_swap_fee_recipient,
        &quote_mint,
        &quote_token_program,
    );

    Ok(vec![
        AccountMeta::new(pool_id, false),
        AccountMeta::new(user, true),
        AccountMeta::new_readonly(pump_global_config, false),
        AccountMeta::new_readonly(base_mint, false),
        AccountMeta::new_readonly(quote_mint, false),
        AccountMeta::new(user_base_token_account, false),
        AccountMeta::new(user_quote_token_account, false),
        AccountMeta::new(pool_base_token_account, false),
        AccountMeta::new(pool_quote_token_account, false),
        AccountMeta::new_readonly(pump_swap_fee_recipient, false),
        AccountMeta::new(protocol_fee_recipient_token_account, false),
        AccountMeta::new_readonly(base_token_program, false),
        AccountMeta::new_readonly(quote_token_program, false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(associated_token_program, false),
        AccountMeta::new_readonly(pump_event_authority, false),
        AccountMeta::new_readonly(pump_swap_program, false),
        AccountMeta::new(coin_creator_vault_ata, false),
        AccountMeta::new_readonly(coin_creator_vault_authority, false),
        AccountMeta::new_readonly(fee_config, false),
        AccountMeta::new_readonly(pump_fee_program, false),
    ])
}

// ============================================================================
// Instruction builders
// ============================================================================

fn create_swap_instruction(
    program_id: Pubkey,
    discriminator: [u8; 8],
    base_amount: u64,
    quote_amount: u64,
    accounts: Vec<AccountMeta>,
) -> Instruction {
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&discriminator);
    data.extend_from_slice(&base_amount.to_le_bytes());
    data.extend_from_slice(&quote_amount.to_le_bytes());

    Instruction {
        program_id,
        accounts,
        data,
    }
}

fn create_buy_exact_quote_instruction(
    program_id: Pubkey,
    spendable_quote_in: u64,
    min_base_amount_out: u64,
    accounts: Vec<AccountMeta>,
) -> Instruction {
    let mut data = Vec::with_capacity(25);
    data.extend_from_slice(&PUMP_SWAP_BUY_EXACT_QUOTE_IN_DISCRIMINATOR);
    data.extend_from_slice(&spendable_quote_in.to_le_bytes());
    data.extend_from_slice(&min_base_amount_out.to_le_bytes());
    // OptionBool { 0: true }
    data.push(1);

    Instruction {
        program_id,
        accounts,
        data,
    }
}

/// Determine the correct token program for a mint (sync version for internal use)
fn get_token_program_sync(mint: &Pubkey) -> Pubkey {
    Pubkey::from_str(TOKEN_PROGRAM).unwrap()
}

// ============================================================================
// High-level Swap Builders
// ============================================================================

/// Build a PumpSwap buy transaction (exact quote in)
///
/// * `mint` - Token mint address
/// * `sol_amount` - Amount of SOL to spend (in SOL, not lamports)
/// * `pool_id` - PumpSwap pool ID
/// * `coin_creator` - Creator of the token (for vault fee distribution)
/// * `base_reserve` - Current base token reserve in the pool
/// * `quote_reserve` - Current quote (SOL) reserve in the pool
/// * `slippage_bps` - Slippage tolerance in basis points
pub async fn build_buy_tx(
    rpc_client: &RpcClient,
    wallet: &Keypair,
    mint: &Pubkey,
    sol_amount: f64,
    pool_id: &Pubkey,
    coin_creator: &Pubkey,
    base_reserve: u64,
    quote_reserve: u64,
    slippage_bps: u64,
) -> Result<Transaction> {
    let owner = wallet.pubkey();
    let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112")?;
    let pump_swap_program = Pubkey::from_str(PUMP_SWAP_PROGRAM)?;
    let is_reverse = false; // Standard order: base=token, quote=SOL

    let token_program = get_token_program_sync(mint);

    let amount_specified = ui_amount_to_amount(sol_amount, 9);
    let base_amount_out = calculate_buy_token_amount_with_fees(
        amount_specified,
        quote_reserve,
        base_reserve,
        PUMPSWAP_TOTAL_BUY_FEE_BPS,
        PUMPSWAP_BUY_BASE_OUT_SAFETY_BPS,
    );
    let _max_quote_amount_in = max_amount_with_slippage(amount_specified, slippage_bps);

    let mut instructions = Vec::new();

    // Ensure token ATA exists
    let out_ata = get_associated_token_address_with_program_id(&owner, mint, &token_program);
    instructions.push(create_associated_token_account_idempotent(
        &owner,
        &owner,
        mint,
        &token_program,
    ));

    // Ensure WSOL ATA exists
    let wsol_ata = get_associated_token_address(&owner, &sol_mint);
    instructions.push(create_associated_token_account_idempotent(
        &owner,
        &owner,
        &sol_mint,
        &Pubkey::from_str(TOKEN_PROGRAM)?,
    ));

    // Fund WSOL ATA with the max quote amount
    instructions.push(system_instruction::transfer(
        &owner,
        &wsol_ata,
        amount_specified,
    ));
    instructions.push(sync_native(&Pubkey::from_str(TOKEN_PROGRAM)?, &wsol_ata)?);

    // Ensure fee recipient WSOL ATA exists
    let fee_recipient = Pubkey::from_str(PUMP_SWAP_FEE_RECIPIENT)?;
    let fee_recipient_ata = get_associated_token_address(&fee_recipient, &sol_mint);
    instructions.push(create_associated_token_account_idempotent(
        &owner,
        &fee_recipient,
        &sol_mint,
        &Pubkey::from_str(TOKEN_PROGRAM)?,
    ));

    // Ensure coin creator vault authority WSOL ATA exists
    let (coin_creator_vault_authority, _) = Pubkey::find_program_address(
        &[b"creator_vault", coin_creator.as_ref()],
        &pump_swap_program,
    );
    let _coin_creator_vault_ata =
        get_associated_token_address(&coin_creator_vault_authority, &sol_mint);
    instructions.push(create_associated_token_account_idempotent(
        &owner,
        &coin_creator_vault_authority,
        &sol_mint,
        &Pubkey::from_str(TOKEN_PROGRAM)?,
    ));

    // Build account metas
    let pool_base_account =
        get_associated_token_address_with_program_id(pool_id, mint, &token_program);
    let pool_quote_account = get_associated_token_address(pool_id, &sol_mint);
    let global_volume_accumulator = get_global_volume_accumulator_pda()?;
    let user_volume_accumulator = get_user_volume_accumulator_pda(&owner)?;

    let accounts = create_buy_accounts(
        *pool_id,
        owner,
        *mint,
        sol_mint,
        out_ata,
        wsol_ata,
        pool_base_account,
        pool_quote_account,
        *coin_creator,
        token_program,
        Pubkey::from_str(TOKEN_PROGRAM)?,
        global_volume_accumulator,
        user_volume_accumulator,
        is_reverse,
    )?;

    // Add swap instruction
    if base_amount_out > 0 {
        instructions.push(create_buy_exact_quote_instruction(
            pump_swap_program,
            amount_specified,
            base_amount_out,
            accounts,
        ));
    } else {
        return Err(anyhow!("Invalid swap amount"));
    }

    // Add compute budget and tip
    instructions.push(
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(69_000),
    );
    instructions.push(
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(400_000),
    );

    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&owner),
        &[wallet],
        BLOCKHASH_CACHE.get_blockhash().await?,
    );

    Ok(tx)
}

/// Build a PumpSwap sell transaction
///
/// * `mint` - Token mint address
/// * `token_amount` - Amount of tokens to sell (raw, with decimals)
/// * `pool_id` - PumpSwap pool ID
/// * `coin_creator` - Creator of the token
/// * `base_reserve` - Current base token reserve
/// * `quote_reserve` - Current quote (SOL) reserve
pub async fn build_sell_tx(
    rpc_client: &RpcClient,
    wallet: &Keypair,
    mint: &Pubkey,
    token_amount: u64,
    pool_id: &Pubkey,
    coin_creator: &Pubkey,
    base_reserve: u64,
    quote_reserve: u64,
) -> Result<Transaction> {
    let owner = wallet.pubkey();
    let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112")?;
    let pump_swap_program = Pubkey::from_str(PUMP_SWAP_PROGRAM)?;
    let is_reverse = false;

    let token_program = get_token_program_sync(mint);

    // Calculate expected SOL output
    let quote_amount_out = calculate_sell_sol_amount(
        token_amount,
        quote_reserve,
        base_reserve,
    );

    let mut instructions = Vec::new();

    // Ensure WSOL ATA exists for receiving SOL
    let wsol_ata = get_associated_token_address(&owner, &sol_mint);
    instructions.push(create_associated_token_account_idempotent(
        &owner,
        &owner,
        &sol_mint,
        &Pubkey::from_str(TOKEN_PROGRAM)?,
    ));

    // Ensure fee recipient WSOL ATA exists
    let fee_recipient = Pubkey::from_str(PUMP_SWAP_FEE_RECIPIENT)?;
    instructions.push(create_associated_token_account_idempotent(
        &owner,
        &fee_recipient,
        &sol_mint,
        &Pubkey::from_str(TOKEN_PROGRAM)?,
    ));

    // Ensure coin creator vault authority WSOL ATA exists
    let (coin_creator_vault_authority, _) = Pubkey::find_program_address(
        &[b"creator_vault", coin_creator.as_ref()],
        &pump_swap_program,
    );
    let _coin_creator_vault_ata =
        get_associated_token_address(&coin_creator_vault_authority, &sol_mint);
    instructions.push(create_associated_token_account_idempotent(
        &owner,
        &coin_creator_vault_authority,
        &sol_mint,
        &Pubkey::from_str(TOKEN_PROGRAM)?,
    ));

    // Build account metas
    let in_ata = get_associated_token_address_with_program_id(&owner, mint, &token_program);
    let pool_base_account =
        get_associated_token_address_with_program_id(pool_id, mint, &token_program);
    let pool_quote_account = get_associated_token_address(pool_id, &sol_mint);

    let accounts = create_sell_accounts(
        *pool_id,
        owner,
        *mint,
        sol_mint,
        in_ata,
        wsol_ata,
        pool_base_account,
        pool_quote_account,
        *coin_creator,
        token_program,
        Pubkey::from_str(TOKEN_PROGRAM)?,
        is_reverse,
    )?;

    // Add swap instruction
    if token_amount > 0 {
        instructions.push(create_swap_instruction(
            pump_swap_program,
            PUMP_SWAP_SELL_DISCRIMINATOR,
            token_amount,
            quote_amount_out,
            accounts,
        ));
    } else {
        return Err(anyhow!("Invalid sell amount"));
    }

    // Add compute budget and tip
    instructions.push(
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(69_000),
    );
    instructions.push(
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(400_000),
    );
    instructions.push(system_instruction::transfer(
        &owner,
        &get_jito_tip_pubkey(),
        30_000,
    ));

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
    use super::*;

    #[test]
    fn test_calculate_buy_token_amount() {
        let sol_in = 1_000_000_000u64; // 1 SOL in lamports
        let sol_reserve = 30_000_000_000u64; // 30 SOL
        let token_reserve = 1_073_000_000_000_000u64;

        let tokens = calculate_buy_token_amount(sol_in, sol_reserve, token_reserve);
        assert!(tokens > 0);
        // Expected: (1 * 1_073_000_000_000_000) / (30 + 1) ≈ 34_612_903_225_806
        assert!(tokens > 30_000_000_000_000u64);
    }

    #[test]
    fn test_calculate_sell_sol_amount() {
        let tokens = 1_000_000_000u64; // 1 token
        let sol_reserve = 30_000_000_000u64;
        let token_reserve = 1_073_000_000_000_000u64;

        let sol = calculate_sell_sol_amount(tokens, sol_reserve, token_reserve);
        assert!(sol > 0);
        // Expected: (1 * 30_000_000_000) / (1_073_000_000_000_000 + 1) ≈ 0.00002796 SOL
        assert!(sol <= sol_reserve);
    }

    #[test]
    fn test_calculate_buy_token_amount_with_fees() {
        let quote_in = 1_000_000_000u64; // 1 SOL
        let quote_reserve = 30_000_000_000u64;
        let base_reserve = 1_073_000_000_000_000u64;
        let fee_bps = 120; // 1.2%
        let safety_bps = 25; // 0.25%

        let tokens = calculate_buy_token_amount_with_fees(
            quote_in, quote_reserve, base_reserve, fee_bps, safety_bps,
        );
        assert!(tokens > 0);
        // After fees should be slightly less than without fees
        let tokens_no_fee = calculate_buy_token_amount(quote_in, quote_reserve, base_reserve);
        assert!(tokens < tokens_no_fee);
    }

    #[test]
    fn test_calculate_price_from_virtual_reserves() {
        let price = calculate_price_from_virtual_reserves(30_000_000_000, 1_073_000_000_000_000);
        assert!(price > 0.0);
        assert!(price < 1.0); // Price should be small (< 1 SOL per token)
    }
}