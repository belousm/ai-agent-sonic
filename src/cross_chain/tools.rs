use anyhow::{anyhow, Result};
use rig_tool_macro::tool;

use crate::common::wrap_unsafe;
use crate::signer::SignerContext;

use super::approvals::{create_approval_transaction, get_allowance};
use super::lifi::LiFi;

// TODO support sponsored transactions here
// it would save a lot of gas if we could drip on any chain,
// fees are substantially higher if the user has an empty wallet on the dest chain

#[tool(description = "
Get a quote for a multichain swap (or bridge).

This might be required in case the user wonders how much it would cost to
perform a swap or bridge. It is also good in case you would like to validate the
token addresses and other params with the user before executing

from_token_symbol is the symbol of the token to swap from.
to_token_symbol is the symbol of the token to swap to.
amount is the amount of tokens to swap.

the from_token_symbol and to_token_symbol can either be a solana public key, evm
address or a symbol.

The amount has to be a string to avoid precision loss. The amount is accounting
for decimals, e.g. 1e6 for 1 USDC but 1e18 for 1 SOL.

Note that sometimes the quote will return a transaction request, with an address that might require approval.
In that case, you can use the approve_token tool to approve the token.

Supported from_chains:
- sol
- arb

Supported to_chains:
- sol
- arb
")]
pub async fn get_multichain_quote(
    from_token_symbol: String,
    to_token_symbol: String,
    amount: String,
    from_chain: String,
    to_chain: String,
) -> Result<serde_json::Value> {
    let signer = SignerContext::current().await;
    let lifi = LiFi::new(None);

    let from_address = if from_chain == "sol" {
        signer.pubkey()
    } else {
        signer.address()
    };

    let to_address = if to_chain == "sol" {
        signer.pubkey()
    } else {
        signer.address()
    };

    let quote = lifi
        .get_quote(
            &from_chain,
            &to_chain,
            &from_token_symbol,
            &to_token_symbol,
            &from_address,
            &to_address,
            &amount,
        )
        .await
        .map_err(|e| {
            anyhow!(
                "{:#?}",
                e.to_string().chars().take(300).collect::<String>()
            )
        })?;

    Ok(quote.summary())
}

#[tool(description = "
Multichain swap (or bridge).

This can be used for any swap, solana to solana, evm to evm, solana to evm,
evm to solana, etc.

Use this in case of the user trying to swap any tokens that exist on two remote
chains, or would like to bridge the tokens

Don't use this in case you are not certain about all of the params, use the
get_multichain_quote tool instead to validate the params in that case.

from_token_symbol is the symbol of the token to bridge from.
to_token_symbol is the symbol of the token to bridge to.
amount is the amount of tokens to bridge.

The amount has to be a string to avoid precision loss. The amount is accounting
for decimals, e.g. 1e6 for 1 USDC but 1e18 for 1 SOL.

Supported from_chains:
- sol
- arb

Supported to_chains:
- sol
- arb
")]
pub async fn multichain_swap(
    from_token_symbol: String,
    to_token_symbol: String,
    amount: String,
    from_chain: String,
    to_chain: String,
) -> Result<String> {
    let signer = SignerContext::current().await;
    let lifi = LiFi::new(None);

    let from_address = if from_chain == "sol" {
        signer.pubkey()
    } else {
        signer.address()
    };

    let to_address = if to_chain == "sol" {
        signer.pubkey()
    } else {
        signer.address()
    };

    let quote = lifi
        .get_quote(
            &from_chain,
            &to_chain,
            &from_token_symbol,
            &to_token_symbol,
            &from_address,
            &to_address,
            &amount,
        )
        .await
        .map_err(|e| {
            anyhow!(
                "{:#?}",
                e.to_string().chars().take(300).collect::<String>()
            )
        })?;

    match quote.transaction_request {
        Some(transaction_request) => {
            wrap_unsafe(move || async move {
                if transaction_request.is_solana() {
                    signer
                        .sign_and_send_encoded_solana_transaction(
                            transaction_request.data,
                        )
                        .await
                } else {
                    signer
                        .sign_and_send_json_evm_transaction(
                            transaction_request.to_json_rpc()?,
                        )
                        .await
                }
            })
            .await
        }
        None => Err(anyhow!("No transaction request")),
    }
}

#[tool(description = "
Check if a token has enough approval for a spender.

token_address is the ERC20 token contract address
spender_address is the address that needs approval
amount is the amount to check approval for (in token decimals)

Returns 'true' if approved, 'false' if not approved
")]
pub async fn check_approval(
    token_address: String,
    spender_address: String,
    amount: String,
) -> Result<String> {
    let signer = SignerContext::current().await;
    let owner_address = signer.address();

    let allowance =
        get_allowance(&token_address, &owner_address, &spender_address)
            .await?;
    let amount = amount
        .parse::<u128>()
        .map_err(|_| anyhow!("Invalid amount"))?;

    Ok((allowance >= amount).to_string())
}

#[tool(description = "
Approve a token for a spender.

token_address is the ERC20 token contract address
spender_address is the address that needs approval
amount is the amount to approve (in token decimals)
")]
pub async fn approve_token(
    token_address: String,
    spender_address: String,
    amount: String,
) -> Result<String> {
    let signer = SignerContext::current().await;
    let owner_address = signer.address();

    let transaction = create_approval_transaction(
        &token_address,
        &spender_address,
        amount.parse::<u128>()?,
        &owner_address,
    )?;

    wrap_unsafe(move || async move {
        signer
            .sign_and_send_json_evm_transaction(transaction)
            .await
            .map_err(|e| anyhow!(e.to_string()))
    })
    .await?;

    Ok("Approved".to_string())
}
