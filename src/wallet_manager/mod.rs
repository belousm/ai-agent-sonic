pub mod config;
pub mod kv_store;
pub mod types;
pub mod util;

use anyhow::{anyhow, Result};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

use config::PrivyConfig;
use serde_json::{json, Value};
use types::{
    CreateWalletRequest, CreateWalletResponse, PrivyClaims, SendRawTransactionRequest, SignAndSendEvmTransactionParams, SignAndSendEvmTransactionRequest, SignAndSendTransactionParams, SignAndSendTransactionRequest, SignAndSendTransactionResponse, SignTransactionParams, SignTransactionRequest, SignTransactionResponse, User, WalletAccount
};

#[cfg(feature = "solana")]
use util::transaction_to_base64;

use util::create_http_client;

use crate::signer::Transaction;

pub struct WalletManager {
    privy_config: PrivyConfig,
    http_client: reqwest::Client,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct UserSession {
    pub(crate) user_id: String,
    pub(crate) session_id: String,
    pub(crate) wallet_address: String,
    pub(crate) pubkey: String,
}

impl UserSession {
    pub fn new(
        user_id: &str,
        session_id: &str,
        wallet_address: &str,
        pubkey: &str,
    ) -> Self {
        Self {
            user_id: user_id.to_string(),
            session_id: session_id.to_string(),
            wallet_address: wallet_address.to_string(),
            pubkey: pubkey.to_string(),
        }
    }
}

impl WalletManager {
    pub fn new(privy_config: PrivyConfig) -> Self {
        let http_client = create_http_client(&privy_config);
        Self {
            privy_config,
            http_client,
        }
    }

    pub async fn auth_user(&self, telegram_id: i64) -> Result<String> {
        let response = self
            .http_client
            .post("https://auth.privy.io/api/v1/authenticate")
            .header(
                "Authorization",
                format!("Bearer {}", self.privy_config.app_secret),
            )
            .json(&json!({
                "app_id": self.privy_config.app_id,
                "identifier": telegram_id.to_string(),
                "auth_type": "telegram",
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Authentication failed: {}",
                response.text().await?
            ));
        }

        let response_json: serde_json::Value = response.json().await?;
        let access_token = response_json["access_token"]
            .as_str()
            .ok_or_else(|| anyhow!("Failed to extract access_token"))?
            .to_string();

        Ok(access_token)
    }

    pub async fn create_wallet(&self) -> Result<CreateWalletResponse> {
        let request = CreateWalletRequest {
            chain_type: "solana".to_string(),
        };

        let response = self
            .http_client
            .post("https://api.privy.io/v1/wallets")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to create wallet: {} - {}",
                response.status(),
                response.text().await?
            ));
        }
        let result = response.json().await?;
        // println!("WALLET CREATION: {:#?}", result);

        Ok(result)

        // Ok(response.json().await?)
    }

    pub async fn authenticate_user(
        &self,
        access_token: &str,
    ) -> Result<UserSession> {
        let claims = self.validate_access_token(access_token)?;
        let user = self.get_user_by_id(&claims.user_id).await?;

        // Initialize basic session data
        let mut session = UserSession {
            user_id: user.id,
            session_id: claims.session_id,
            wallet_address: String::new(),
            pubkey: String::new(),
        };

        let solana_wallet =
            find_wallet(&user.linked_accounts, "solana", "privy")?;
        session.pubkey = solana_wallet.address.clone();

        let evm_wallet =
            find_wallet(&user.linked_accounts, "ethereum", "privy")?;
        session.wallet_address = evm_wallet.address.clone();

        Ok(session)
    }

    #[cfg(feature = "evm")]
    pub async fn sign_and_send_evm_transaction(
        &self,
        address: String,
        transaction: alloy::rpc::types::TransactionRequest,
    ) -> Result<String> {
        self.sign_and_send_json_evm_transaction(
            address,
            serde_json::to_value(transaction)?,
        )
        .await
    }

    #[cfg(feature = "solana")]
    pub async fn sign_and_send_solana_transaction(
        &self,
        address: String,
        transaction: &solana_sdk::transaction::Transaction,
    ) -> Result<String> {
        self.sign_and_send_encoded_solana_transaction(
            address,
            transaction_to_base64(transaction)?,
        )
        .await
    }

    pub async fn sign_and_send_json_evm_transaction(
        &self,
        address: String,
        mut transaction: serde_json::Value,
    ) -> Result<String> {
        use sqlx::{postgres::PgPoolOptions, Row};
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use std::time::{SystemTime, UNIX_EPOCH};
        use anyhow::anyhow;

        let database_url = "postgres://admin:admin@127.0.0.1:5432/wallets";
    
        // Подключение к базе данных
        let db_pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to database");
    
        let wallet_id: Option<String> = sqlx::query(
            r#"
            SELECT wallet_id FROM wallets 
            WHERE address = $1 AND current_wallet = TRUE
            LIMIT 1
            "#,
        )
        .bind(&address)
        .fetch_optional(&db_pool)
        .await?
        .map(|row| row.get("wallet_id"));
    
        let wallet_id = match wallet_id {
            Some(id) => id,
            None => return Err(anyhow!("Wallet ID not found for this wallet_pubkey")),
        };

        if let Value::Object(ref mut obj) = transaction {
            obj.insert("type".to_string(), Value::Number(0.into())); // Ensure type is a number
        }
    
        let request = SignTransactionRequest {
            address,
            chain_type: "ethereum".to_string(),
            method: "eth_signTransaction".to_string(),
            // caip2: "eip155:146".to_string(), // TODO: параметризовать это
            params: SignTransactionParams { transaction },
        };
    
        let url = format!("https://api.privy.io/v1/wallets/{}/rpc", wallet_id);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs().to_string();
        let signature = URL_SAFE_NO_PAD.encode(format!("{}{}", self.privy_config.app_id, timestamp));
    
        println!("PRIVY REQUEST: {:#?}", request);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Basic {}", base64::encode(format!("{}:{}", self.privy_config.app_id, self.privy_config.app_secret))))
            .header("privy-app-id", &self.privy_config.app_id)
            .header("privy-authorization-signature", signature)
            .json(&request)
            .send()
            .await?;
        
        println!("PRIVY RESPONSE: {:#?}", response);

        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to send transaction: {}",
                response.text().await?
            ));
        }

        let result: SignTransactionResponse = response.json().await?;
        let signed_tx = result.data.signed_transaction;

        let send_request = SendRawTransactionRequest {
            method: "eth_sendRawTransaction".to_string(),
            params: vec![signed_tx],
        };
        
        let rpc_response = self
            .http_client
            .post("https://rpc.soniclabs.com")  // Используй Infura, Alchemy или свой RPC
            .json(&send_request)
            .send()
            .await?;
        
        if !rpc_response.status().is_success() {
            return Err(anyhow!(
                "Failed to broadcast transaction: {}",
                rpc_response.text().await?
            ));
        }
        
        let tx_hash: String = rpc_response.json().await?;
        Ok(tx_hash)        
    
        // let result: SignAndSendTransactionResponse = response.json().await?;
        // Ok(result.data.hash)
    }    

    // pub async fn sign_and_send_json_evm_transaction(
    //     &self,
    //     address: String,
    //     transaction: serde_json::Value,
    // ) -> Result<String> {
    //     let request = SignAndSendEvmTransactionRequest {
    //         address,
    //         chain_type: "ethereum".to_string(),
    //         method: "eth_signTransaction".to_string(),
    //         caip2: "eip155:42161".to_string(), // TODO parametrize this - hardcoded arb
    //         params: SignAndSendEvmTransactionParams { transaction },
    //     };

    //     let response = self
    //         .http_client
    //         .post("https://auth.privy.io/api/v1/wallets/rpc")
    //         .json(&request)
    //         .send()
    //         .await?;

    //     if !response.status().is_success() {
    //         return Err(anyhow!(
    //             "Failed to send transaction: {}",
    //             response.text().await?
    //         ));
    //     }

    //     println!("RESPONSE EVM: {:#?}", response);

    //     let result: SignAndSendTransactionResponse = response.json().await?;
    //     Ok(result.data.hash)
    // }

    // pub async fn sign_and_send_encoded_solana_transaction(
    //     &self,
    //     address: String,
    //     encoded_transaction: String,
    // ) -> Result<String> {
    //     let request = SignAndSendTransactionRequest {
    //         address,
    //         chain_type: "solana".to_string(),
    //         method: "signAndSendTransaction".to_string(),
    //         caip2: "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp".to_string(),
    //         params: SignAndSendTransactionParams {
    //             transaction: encoded_transaction,
    //             encoding: "base64".to_string(),
    //         },
    //     };
    //     println!("I'AM IN PRIVY SIGNER");
    //     let response = self
    //         .http_client
    //         .post("https://api.privy.io/v1/wallets/rpc")
    //         .json(&request)
    //         .send()
    //         .await?;
    //     println!("RESPONSE: {:#?}", response);
    //     if !response.status().is_success() {
    //         return Err(anyhow!(
    //             "Failed to sign transaction: {}",
    //             response.text().await?
    //         ));
    //     }

    //     let result: SignAndSendTransactionResponse = response.json().await?;
    //     Ok(result.data.hash)
    // }

    pub async fn sign_and_send_encoded_solana_transaction(
        &self,
        address: String,
        encoded_transaction: String,
    ) -> Result<String> {
        use sqlx::{postgres::PgPoolOptions, Row};
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, decode, Engine as _};
        use std::time::{SystemTime, UNIX_EPOCH};
        use anyhow::anyhow;
        use solana_sdk::transaction::Transaction;
        use solana_sdk::bs58;

        let database_url = "postgres://admin:admin@127.0.0.1:5432/wallets";

        // Просто подключаемся к базе
        let db_pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to database");

        let wallet_id: Option<String> = sqlx::query(
            r#"
            SELECT wallet_id FROM wallets 
            WHERE address = $1 AND current_wallet = TRUE
            LIMIT 1
            "#,
        )
        .bind(&address)
        .fetch_optional(&db_pool)
        .await?
        .map(|row| row.get("wallet_id"));

        let wallet_id = match wallet_id {
            Some(id) => id,
            None => return Err(anyhow!("Wallet ID not found for this wallet_pubkey")),
        };

        // 1️⃣ Декодируем base64 в байты
        let decoded_bytes = match decode(encoded_transaction.clone()) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("❌ Ошибка декодирования base64: {}", e);
                return Ok(Default::default());
            }
        };

        // 2️⃣ Десериализуем в Solana Transaction
        let tx: Transaction = match bincode::deserialize(&decoded_bytes) {
            Ok(transaction) => transaction,
            Err(e) => {
                eprintln!("❌ Ошибка парсинга транзакции: {}", e);
                return Ok(Default::default());
            }
        };
        // let message = tx.message();
        // println!("\n✅ Инструкции:");
        // for (i, instruction) in message.instructions.iter().enumerate() {
        //     println!("\n🔹 Инструкция {}:", i);
        //     println!("- Программа: {}", message.account_keys[instruction.program_id_index as usize]);

        //     // Выводим список аккаунтов, участвующих в инструкции
        //     println!("- Аккаунты:");
        //     for account in &instruction.accounts {
        //         println!("  * {}", message.account_keys[*account as usize]);
        //     }

        //     // Декодируем `data` инструкции, если возможно
        //     println!("- Данные инструкции (base58): {}", bs58::encode(&instruction.data).into_string());
        // }

        let request = SignAndSendTransactionRequest {
            address,
            chain_type: "solana".to_string(),
            method: "signAndSendTransaction".to_string(),
            caip2: "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp".to_string(),
            params: SignAndSendTransactionParams {
                transaction: encoded_transaction,
                encoding: "base64".to_string(),
            },
        };

        let url = format!("https://api.privy.io/v1/wallets/{}/rpc", wallet_id);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs().to_string();
        let signature = URL_SAFE_NO_PAD.encode(format!("{}{}", self.privy_config.app_id, timestamp));

        println!("I'AM IN PRIVY SIGNER");

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Basic {}", base64::encode(format!("{}:{}", self.privy_config.app_id, self.privy_config.app_secret))))
            .header("privy-app-id", &self.privy_config.app_id)
            .header("privy-authorization-signature", signature)
            .json(&request)
            .send()
            .await?;
            
        println!("RESPONSE: {:#?}", response);
        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to sign transaction: {}",
                response.text().await?
            ));
        }

        let result: SignAndSendTransactionResponse = response.json().await?;
        Ok(result.data.hash)
    }

    pub fn validate_access_token(
        &self,
        access_token: &str,
    ) -> Result<PrivyClaims> {
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_issuer(&["privy.io"]);
        validation.set_audience(&[self.privy_config.app_id.clone()]);

        let key = DecodingKey::from_ec_pem(
            self.privy_config.verification_key.as_bytes(),
        )?;

        let token_data =
            decode::<PrivyClaims>(access_token, &key, &validation)
                .map_err(|_| anyhow!("Failed to authenticate"))?;

        Ok(token_data.claims)
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<User> {
        let url = format!("https://auth.privy.io/api/v1/users/{}", user_id);

        let response = self.http_client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to get user data: {}",
                response.status()
            ));
        }
        let text = response.text().await?;
        // dbg!(serde_json::from_str::<serde_json::Value>(&text)?);
        Ok(serde_json::from_str(&text)?)
    }
}

fn find_wallet<'a>(
    linked_accounts: &'a [types::LinkedAccount],
    chain_type: &str,
    wallet_client: &str,
) -> Result<&'a WalletAccount> {
    linked_accounts
        .iter()
        .find_map(|account| match account {
            types::LinkedAccount::Wallet(wallet) => {
                if wallet.delegated
                    && wallet.chain_type == chain_type
                    && wallet.wallet_client == wallet_client
                {
                    Some(wallet)
                } else {
                    None
                }
            }
            _ => None,
        })
        .ok_or_else(|| {
            anyhow!("Could not find a delegated {} wallet", chain_type)
        })
}
