use std::str::FromStr;

use anyhow::{anyhow, Result};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::transaction::Transaction;

#[derive(Serialize, Deserialize, Debug)]
pub struct PlatformFee {
    pub amount: String,
    #[serde(rename = "feeBps")]
    pub fee_bps: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DynamicSlippage {
    #[serde(rename = "minBps")]
    pub min_bps: i32,
    #[serde(rename = "maxBps")]
    pub max_bps: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RoutePlan {
    #[serde(rename = "swapInfo")]
    pub swap_info: SwapInfo,
    pub percent: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuoteResponse {
    #[serde(rename = "inputMint")]
    pub input_mint: String,
    #[serde(rename = "inAmount")]
    pub in_amount: String,
    #[serde(rename = "outputMint")]
    pub output_mint: String,
    #[serde(rename = "outAmount")]
    pub out_amount: String,
    #[serde(rename = "otherAmountThreshold")]
    pub other_amount_threshold: String,
    #[serde(rename = "swapMode")]
    pub swap_mode: String,
    #[serde(rename = "slippageBps")]
    pub slippage_bps: i32,
    #[serde(rename = "platformFee")]
    pub platform_fee: Option<PlatformFee>,
    #[serde(rename = "priceImpactPct")]
    pub price_impact_pct: String,
    #[serde(rename = "routePlan")]
    pub route_plan: Vec<RoutePlan>,
    #[serde(rename = "contextSlot")]
    pub context_slot: u64,
    #[serde(rename = "timeTaken")]
    pub time_taken: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SwapInfo {
    #[serde(rename = "ammKey")]
    pub amm_key: String,
    pub label: Option<String>,
    #[serde(rename = "inputMint")]
    pub input_mint: String,
    #[serde(rename = "outputMint")]
    pub output_mint: String,
    #[serde(rename = "inAmount")]
    pub in_amount: String,
    #[serde(rename = "outAmount")]
    pub out_amount: String,
    #[serde(rename = "feeAmount")]
    pub fee_amount: String,
    #[serde(rename = "feeMint")]
    pub fee_mint: String,
}

#[derive(Serialize)]
pub struct SwapRequest {
    #[serde(rename = "userPublicKey")]
    pub user_public_key: String,
    #[serde(rename = "wrapAndUnwrapSol")]
    pub wrap_and_unwrap_sol: bool,
    #[serde(rename = "useSharedAccounts")]
    pub use_shared_accounts: bool,
    #[serde(rename = "feeAccount")]
    pub fee_account: Option<String>,
    #[serde(rename = "trackingAccount")]
    pub tracking_account: Option<String>,
    #[serde(rename = "computeUnitPriceMicroLamports")]
    pub compute_unit_price_micro_lamports: Option<u64>,
    #[serde(rename = "prioritizationFeeLamports")]
    pub prioritization_fee_lamports: Option<u64>,
    #[serde(rename = "asLegacyTransaction")]
    pub as_legacy_transaction: bool,
    #[serde(rename = "useTokenLedger")]
    pub use_token_ledger: bool,
    #[serde(rename = "destinationTokenAccount")]
    pub destination_token_account: Option<String>,
    #[serde(rename = "dynamicComputeUnitLimit")]
    pub dynamic_compute_unit_limit: bool,
    #[serde(rename = "skipUserAccountsRpcCalls")]
    pub skip_user_accounts_rpc_calls: bool,
    #[serde(rename = "dynamicSlippage")]
    pub dynamic_slippage: Option<DynamicSlippage>,
    #[serde(rename = "quoteResponse")]
    pub quote_response: QuoteResponse,
}

#[derive(Deserialize, Debug)]
pub struct SwapInstructionsResponse {
    #[serde(rename = "tokenLedgerInstruction")]
    pub token_ledger_instruction: Option<InstructionData>,
    #[serde(rename = "computeBudgetInstructions")]
    pub compute_budget_instructions: Option<Vec<InstructionData>>,
    #[serde(rename = "setupInstructions")]
    pub setup_instructions: Vec<InstructionData>,
    #[serde(rename = "swapInstruction")]
    pub swap_instruction: InstructionData,
    #[serde(rename = "cleanupInstruction")]
    pub cleanup_instruction: Option<InstructionData>,
    #[serde(rename = "addressLookupTableAddresses")]
    pub address_lookup_table_addresses: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct InstructionData {
    #[serde(rename = "programId")]
    pub program_id: String,
    pub accounts: Vec<AccountMeta>,
    pub data: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AccountMeta {
    pub pubkey: String,
    #[serde(rename = "isSigner")]
    pub is_signer: bool,
    #[serde(rename = "isWritable")]
    pub is_writable: bool,
}

pub struct Jupiter;

impl Jupiter {
    pub async fn fetch_quote(
        input_mint: &str,
        output_mint: &str,
        amount: u64,
        slippage: u16,
    ) -> Result<QuoteResponse> {
        let url = format!(
            "https://quote-api.jup.ag/v6/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}&asLegacyTransaction=true",
            input_mint, output_mint, amount, slippage
        );

        let response =
            reqwest::get(&url).await?.json::<QuoteResponse>().await?;
        Ok(response)
    }

    pub async fn swap(
        quote_response: QuoteResponse,
        owner: &Pubkey,
    ) -> Result<Transaction> {
        use solana_client::rpc_client::RpcClient;
        use spl_associated_token_account::{
            get_associated_token_address,
            instruction::create_associated_token_account,
        };
        use spl_token::ID as TOKEN_PROGRAM_ID;
        use std::env;
        use solana_program::system_program;
        use std::str::FromStr;
    
        let rpc_url = env::var("SOLANA_RPC_URL").unwrap_or_else(|_| {
            "https://api.mainnet-beta.solana.com".to_string()
        });
    
        let rpc_client = RpcClient::new(rpc_url);
    
        // 🔥 1️⃣ Определяем mint входного и выходного токенов
        // let input_mint = Pubkey::from_str(&quote_response.input_mint)
        //     .map_err(|_| anyhow!("Invalid input mint"))?;
        let output_mint = Pubkey::from_str(&quote_response.output_mint)
            .map_err(|_| anyhow!("Invalid output mint"))?;
    
        // 🔥 2️⃣ Проверяем, является ли входной или выходной токен SOL
        // let is_input_sol = input_mint == system_program::ID;
        let is_output_sol = output_mint == system_program::ID;
    
        let mut instructions = Vec::new();
    
        // // 🔥 3️⃣ Определяем ATA для входного и выходного токена (если не SOL)
        // let input_ata = if is_input_sol {
        //     None // SOL не использует ATA
        // } else {
        //     Some(get_associated_token_address(owner, &input_mint))
        // };
    
        let output_ata = if is_output_sol {
            None // SOL не использует ATA
        } else {
            Some(get_associated_token_address(owner, &output_mint))
        };
    
        // 🔥 5️⃣ Запрашиваем swap-инструкции у Jupiter
        let swap_request = SwapRequest {
            user_public_key: owner.to_string(),
            wrap_and_unwrap_sol: is_output_sol, // Jupiter сам оборачивает SOL
            use_shared_accounts: false,
            fee_account: None,
            tracking_account: None,
            compute_unit_price_micro_lamports: None,
            prioritization_fee_lamports: None,
            as_legacy_transaction: false,
            use_token_ledger: false,
            destination_token_account: output_ata.map(|ata| ata.to_string()), // None, если SOL
            dynamic_compute_unit_limit: false,
            skip_user_accounts_rpc_calls: true,
            dynamic_slippage: None,
            quote_response,
        };
    
        let client = reqwest::Client::new();
        let raw_res = client
            .post("https://quote-api.jup.ag/v6/swap-instructions")
            .json(&swap_request)
            .send()
            .await?;
    
        if !raw_res.status().is_success() {
            let error = raw_res.text().await.map_err(|e| anyhow!(e))?;
            return Err(anyhow!("Jupiter Swap Error: {}", error));
        }
    
        let response = raw_res
            .json::<SwapInstructionsResponse>()
            .await
            .map_err(|e| anyhow!("Failed to parse swap response: {}", e))?;
    
        // 🔥 6️⃣ Выполняем setupInstrductions (создание необходимых аккаунтов)
        for setup_ix in response.setup_instructions {
            println!("✅ Добавляем setup-инструкцию: {:?}", setup_ix);
            instructions.push(Self::convert_instruction_data(setup_ix)?);
        }
    
        // 🔥 7️⃣ Добавляем основную swap-инструкцию
        instructions.push(Self::convert_instruction_data(response.swap_instruction)?);
    
        // // 🔥 8️⃣ Добавляем Compute Budget (если есть)
        // if let Some(compute_budget_instructions) = response.compute_budget_instructions {
        //     for compute_ix in compute_budget_instructions {
        //         instructions.push(Self::convert_instruction_data(compute_ix)?);
        //     }
        // }
    
        // // 🔥 9️⃣ Добавляем Cleanup-инструкцию (если есть)
        // if let Some(cleanup_ix) = response.cleanup_instruction {
        //     instructions.push(Self::convert_instruction_data(cleanup_ix)?);
        // }
    
        // 🔥 10️⃣ Получаем свежий `blockhash`
        let blockhash = rpc_client.get_latest_blockhash()?;
    
        // ✅ 11️⃣ Создаём транзакцию и применяем blockhash
        let mut tx = Transaction::new_with_payer(&instructions, Some(owner));
        tx.message.recent_blockhash = blockhash;
    
        Ok(tx)
    }    

    // pub async fn swap(
    //     quote_response: QuoteResponse,
    //     owner: &Pubkey,
    // ) -> Result<Transaction> {
    //     use solana_client::rpc_client::RpcClient;
    //     use spl_associated_token_account::{
    //         get_associated_token_address, 
    //         instruction::create_associated_token_account,
    //     };
    //     use spl_token::ID as TOKEN_PROGRAM_ID;
    //     use std::env;
    //     use solana_program::system_program;
    //     use std::str::FromStr;
        
    //     let rpc_url = env::var("SOLANA_RPC_URL").unwrap_or_else(|_| {
    //         "https://api.mainnet-beta.solana.com".to_string()
    //     });
        
    //     println!("QUOTE: {:#?}", quote_response);
    //     let rpc_client = RpcClient::new(rpc_url);
    
    //     // 🔥 1️⃣ Определяем mint входного и выходного токенов
    //     let input_mint = Pubkey::from_str(&quote_response.input_mint)
    //         .map_err(|_| anyhow!("Invalid input mint"))?;
    //     let output_mint = Pubkey::from_str(&quote_response.output_mint)
    //         .map_err(|_| anyhow!("Invalid output mint"))?;
    
    //     // 🔥 2️⃣ Проверяем, является ли входной или выходной токен SOL
    //     let is_input_sol = input_mint == system_program::ID;
    //     let is_output_sol = output_mint == system_program::ID;
    
    //     let mut instructions = Vec::new();
    
    //     // 🔥 3️⃣ Определяем ATA для входного и выходного токена (если не SOL)
    //     let input_ata = if is_input_sol {
    //         None // SOL не использует ATA
    //     } else {
    //         Some(get_associated_token_address(owner, &input_mint))
    //     };
    
    //     let output_ata = if is_output_sol {
    //         None // SOL не использует ATA
    //     } else {
    //         Some(get_associated_token_address(owner, &output_mint))
    //     };
    
    //     // 🔥 4️⃣ Создаём `input_ata`, если его нет (Только для SPL-токенов)
    //     if let Some(input_ata) = input_ata {
    //         if rpc_client.get_account(&input_ata).is_err() {
    //             println!("⚠️ `input_ata` не найден! Нужно создать его.");
    //             let ata_ix = create_associated_token_account(
    //                 owner, owner, &input_mint, &TOKEN_PROGRAM_ID,
    //             );
    //             instructions.push(ata_ix);
    //         } else {
    //             println!("✅ `input_ata` найден, используем существующий.");
    //         }
    //     }
    
    //     // 🔥 5️⃣ Создаём `output_ata`, если его нет (Только для SPL-токенов)
    //     if let Some(output_ata) = output_ata {
    //         if rpc_client.get_account(&output_ata).is_err() {
    //             println!("⚠️ `output_ata` не найден! Добавляем создание...");
    //             let ata_ix = create_associated_token_account(
    //                 owner, owner, &output_mint, &TOKEN_PROGRAM_ID,
    //             );
    //             instructions.push(ata_ix);
    //         } else {
    //             println!("✅ `output_ata` найден, используем существующий.");
    //         }
    //     }
    
    //     // 🔥 6️⃣ Запрашиваем swap-инструкции у Jupiter
    //     let swap_request = SwapRequest {
    //         user_public_key: owner.to_string(),
    //         wrap_and_unwrap_sol: is_input_sol || is_output_sol, // Jupiter сам оборачивает SOL
    //         use_shared_accounts: false,
    //         fee_account: None,
    //         tracking_account: None,
    //         compute_unit_price_micro_lamports: None,
    //         prioritization_fee_lamports: None,
    //         as_legacy_transaction: false,
    //         use_token_ledger: false,
    //         destination_token_account: output_ata.map(|ata| ata.to_string()), // None, если SOL
    //         dynamic_compute_unit_limit: false,
    //         skip_user_accounts_rpc_calls: true,
    //         dynamic_slippage: None,
    //         quote_response,
    //     };
    
    //     let client = reqwest::Client::new();
    //     let raw_res = client
    //         .post("https://quote-api.jup.ag/v6/swap-instructions")
    //         .json(&swap_request)
    //         .send()
    //         .await?;
    
    //     if !raw_res.status().is_success() {
    //         let error = raw_res.text().await.map_err(|e| anyhow!(e))?;
    //         return Err(anyhow!("Jupiter Swap Error: {}", error));
    //     }
    
    //     let response = raw_res
    //         .json::<SwapInstructionsResponse>()
    //         .await
    //         .map_err(|e| anyhow!("Failed to parse swap response: {}", e))?;
    
    //     // ✅ 7️⃣ Добавляем основную swap-инструкцию
    //     instructions.push(Self::convert_instruction_data(response.swap_instruction)?);
    
    //     // ✅ 8️⃣ Добавляем Compute Budget (если есть)
    //     if let Some(compute_budget_instructions) = response.compute_budget_instructions {
    //         for compute_ix in compute_budget_instructions {
    //             instructions.push(Self::convert_instruction_data(compute_ix)?);
    //         }
    //     }
    
    //     // ✅ 9️⃣ Добавляем Cleanup-инструкцию (если есть)
    //     if let Some(cleanup_ix) = response.cleanup_instruction {
    //         instructions.push(Self::convert_instruction_data(cleanup_ix)?);
    //     }
    
    //     // 🔥 10️⃣ Получаем свежий `blockhash`
    //     let blockhash = rpc_client.get_latest_blockhash()?;
    
    //     // println!("🔍 Проверяем инструкции перед отправкой:");
    //     // for (i, instruction) in instructions.iter().enumerate() {
    //     //     println!("🔹 Инструкция {}:", i);
    //     //     println!("- Программа: {}", instruction.program_id);
    //     //     println!("- Аккаунты:");
    //     //     for account in &instruction.accounts {
    //     //         println!("  * {:#?}", account);
    //     //     }
    //     //     println!("- Данные (base58): {}", bs58::encode(&instruction.data).into_string());

    //     //     let raw_data = bs58::decode("4pm4hNwxvXF14GVtu1ugpDWsWDpr3yP359uvrTXYwJSN9LFrp1fZgAMjqh")
    //     //         .into_vec()
    //     //         .expect("Ошибка декодирования данных");
    //     //     println!("🔍 Декодированные данные Jupiter: {:?}", raw_data);
    //     //     println!("📏 Длина данных: {}", raw_data.len());
    //     // }

    
    //     // ✅ 11️⃣ Создаём транзакцию и применяем blockhash
    //     let mut tx = Transaction::new_with_payer(&instructions, Some(owner));
    //     tx.message.recent_blockhash = blockhash;
    
    //     Ok(tx)
    // }
    

    // pub async fn swap(
    //     quote_response: QuoteResponse,
    //     owner: &Pubkey,
    // ) -> Result<Transaction> {
    //     let swap_request = SwapRequest {
    //         user_public_key: owner.to_string(),
    //         wrap_and_unwrap_sol: false, // Avoid adding extra SOL wrapping logic
    //         use_shared_accounts: false, // Reduce extra accounts
    //         fee_account: None,
    //         tracking_account: None,
    //         compute_unit_price_micro_lamports: None,
    //         prioritization_fee_lamports: None,
    //         as_legacy_transaction: false, // Switch to VersionedTransaction for better size handling
    //         use_token_ledger: false, // Avoid adding extra ledger instructions
    //         destination_token_account: Some(owner.to_string()), // Use an existing token account if possible
    //         dynamic_compute_unit_limit: false, // Avoid adding unnecessary compute budget changes
    //         skip_user_accounts_rpc_calls: true,
    //         dynamic_slippage: None,
    //         quote_response,
    //     };

    //     let client = reqwest::Client::new();
    //     let raw_res = client
    //         .post("https://quote-api.jup.ag/v6/swap-instructions")
    //         .json(&swap_request)
    //         .send()
    //         .await?;

    //     if !raw_res.status().is_success() {
    //         let error = raw_res.text().await.map_err(|e| anyhow!(e))?;
    //         return Err(anyhow!(error));
    //     }

    //     let response = raw_res
    //         .json::<SwapInstructionsResponse>()
    //         .await
    //         .map_err(|e| anyhow!(e))?;

    //     let mut instructions = Vec::new();

    //     // Swap instruction (required)
    //     instructions
    //         .push(Self::convert_instruction_data(response.swap_instruction)?);

    //     // If compute budget instructions exist, take only the first one
    //     if let Some(compute_budget_instructions) =
    //         response.compute_budget_instructions
    //     {
    //         if !compute_budget_instructions.is_empty() {
    //             instructions.push(Self::convert_instruction_data(
    //                 compute_budget_instructions[0].clone(),
    //             )?);
    //         }
    //     }

    //     // If cleanup instruction exists, include it (optional)
    //     if let Some(cleanup_ix) = response.cleanup_instruction {
    //         instructions.push(Self::convert_instruction_data(cleanup_ix)?);
    //     }

    //     // Create the transaction
    //     let tx = Transaction::new_with_payer(&instructions, Some(owner));
    //     Ok(tx)
    // }

    fn convert_instruction_data(
        ix_data: InstructionData,
    ) -> Result<solana_sdk::instruction::Instruction> {
        let program_id = Pubkey::from_str(&ix_data.program_id)?;

        let accounts = ix_data
            .accounts
            .into_iter()
            .map(|acc| {
                Ok(solana_sdk::instruction::AccountMeta {
                    pubkey: Pubkey::from_str(&acc.pubkey)?,
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let data = BASE64_STANDARD.decode(ix_data.data)?;

        Ok(solana_sdk::instruction::Instruction {
            program_id,
            accounts,
            data,
        })
    }
}