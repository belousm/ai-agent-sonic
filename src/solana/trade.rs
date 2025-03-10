use crate::solana::jup::Jupiter;
use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::transaction::Transaction;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::{Keypair, Signer};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account,
};
use spl_token::ID as TOKEN_PROGRAM_ID;
use std::env;
use std::str::FromStr;
use solana_program::system_program;

pub async fn create_trade_transaction(
    input_mint: String,
    input_amount: u64,
    output_mint: String,
    slippage_bps: u16,
    owner: &Pubkey,
) -> Result<Transaction> {
    let quote = Jupiter::fetch_quote(
        &input_mint,
        &output_mint,
        input_amount,
        slippage_bps,
    )
    .await
    .map_err(|e| anyhow!("Failed to fetch quote: {}", e.to_string()))?;

    let tx = Jupiter::swap(quote, owner)
        .await
        .map_err(|e| anyhow!("Failed to swap: {}", e.to_string()))?;
    println!("---------SWAP CREATED-------");

    Ok(tx)
}

pub async fn create_ata_if_needed(
    owner: &Pubkey,
    mint: &Pubkey,
) -> Result<Transaction> {
    let ata = get_associated_token_address(owner, mint);
    let rpc_url = env::var("SOLANA_RPC_URL").unwrap_or_else(|_| {
        "https://api.mainnet-beta.solana.com".to_string()
    });

    let rpc_client = RpcClient::new(rpc_url);
    if rpc_client.get_account(&ata).is_err() {
        println!("⚠️ `ATA {}` не найден! Создаём...", ata);

        let ata_ix = create_associated_token_account(
            owner, owner, mint, &TOKEN_PROGRAM_ID,
        );

        let blockhash = rpc_client.get_latest_blockhash()?;
        let tx = Transaction::new_with_payer(
            &[ata_ix],
            Some(owner),
        );

        println!("✅ `ATA {}` создан.", ata);
        Ok(tx)
    } else {
        println!("✅ `ATA {}` найден, используем существующий.", ata);
        Ok(Transaction::new_with_payer(&[], Some(owner)))
    }
}

#[cfg(test)]
mod tests {
    use crate::solana::{constants, util::load_keypair_for_tests};

    use super::*;
    use solana_sdk::native_token::sol_to_lamports;
    use solana_sdk::signer::Signer;

    #[tokio::test]
    async fn test_trade() {
        let keypair = load_keypair_for_tests();
        let result = create_trade_transaction(
            constants::WSOL.to_string(),
            sol_to_lamports(0.001),
            "FUAfBo2jgks6gB4Z4LfZkqSZgzNucisEHqnNebaRxM1P".to_string(),
            300,
            &keypair.pubkey(),
        )
        .await;
        tracing::debug!("{:?}", result);

        assert!(result.is_ok());
    }
}
