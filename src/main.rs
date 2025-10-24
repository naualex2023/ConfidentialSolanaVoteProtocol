use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::HashMap;

#[derive(BorshSerialize, BorshDeserialize, Debug)]
struct ElectionTally {
    results: HashMap<u8, u32>, // vote_option -> count
    total_valid_votes: u32,
    proof: Vec<u8>,
}

pub struct ArciumTallyClient {
    arcium_endpoint: String,
    cluster_id: String,
}

impl ArciumTallyClient {
    pub fn new(endpoint: String, cluster_id: String) -> Self {
        Self {
            arcium_endpoint: endpoint,
            cluster_id,
        }
    }

    pub async fn tally_election(
        &self,
        encrypted_ballots: Vec<Vec<u8>>,
        election_public_key: [u8; 32],
    ) -> Result<ElectionTally, Box<dyn std::error::Error>> {
        // Подготавливаем MPC задачу для Arcium
        let tally_request = serde_json::json!({
            "encrypted_ballots": encrypted_ballots,
            "election_public_key": election_public_key,
            "cluster_id": self.cluster_id,
            "algorithm": "homomorphic_sum"
        });

        let client = reqwest::Client::new();
        let response = client
            .post(&format!("{}/tally", self.arcium_endpoint))
            .json(&tally_request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err("Arcium tally request failed".into());
        }

        let tally_result: ElectionTally = response.json().await?;
        Ok(tally_result)
    }

    pub async fn validate_tally(
        &self,
        encrypted_ballots: &[Vec<u8>],
        tally_results: &ElectionTally,
        election_public_key: [u8; 32],
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let validation_request = serde_json::json!({
            "encrypted_ballots": encrypted_ballots,
            "tally_results": tally_results.results,
            "election_public_key": election_public_key,
            "proof": tally_results.proof,
            "cluster_id": self.cluster_id,
        });

        let client = reqwest::Client::new();
        let response = client
            .post(&format!("{}/validate", self.arcium_endpoint))
            .json(&validation_request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err("Arcium validation request failed".into());
        }

        let validation_result: bool = response.json().await?;
        Ok(validation_result)
    }
}

pub struct SolanaBallotReader {
    rpc_client: RpcClient,
}

impl SolanaBallotReader {
    pub fn new(rpc_url: String) -> Self {
        Self {
            rpc_client: RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed()),
        }
    }

    pub async fn get_all_encrypted_ballots(
        &self,
        election_pda: &Pubkey,
        program_id: &Pubkey,
    ) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        let mut all_ballots = Vec::new();
        let mut chunk_index = 0;

        loop {
            // Получаем PDA для chunk
            let (chunk_pda, _) = Pubkey::find_program_address(
                &[b"ballot_chunk", election_pda.as_ref(), &chunk_index.to_le_bytes()],
                program_id,
            );

            // Пытаемся получить аккаунт
            if let Ok(account) = self.rpc_client.get_account(&chunk_pda) {
                let chunk_data = BallotChunk::try_from_slice(&account.data)?;
                
                for ballot in chunk_data.ballots {
                    all_ballots.push(ballot.encrypted_data);
                }

                // Проверяем, есть ли следующий чанк
                if let Some(next_chunk) = chunk_data.next_chunk {
                    chunk_index += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(all_ballots)
    }

    pub async fn get_election_public_key(
        &self,
        election_pda: &Pubkey,
    ) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        let account = self.rpc_client.get_account(election_pda)?;
        let election = Election::try_from_slice(&account.data)?;
        Ok(election.public_key)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Конфигурация
    let rpc_url = "https://api.devnet.solana.com".to_string();
    let arcium_endpoint = "https://arcium-api.example.com".to_string();
    let arcium_cluster_id = "cluster-123".to_string();
    
    let election_pda = Pubkey::new_unique(); // Заменить на реальный PDA выборов
    let program_id = Pubkey::new_unique(); // Заменить на реальный program ID

    // Инициализация клиентов
    let ballot_reader = SolanaBallotReader::new(rpc_url);
    let arcium_client = ArciumTallyClient::new(arcium_endpoint, arcium_cluster_id);

    // Получаем все зашифрованные бюллетени
    println!("Fetching encrypted ballots...");
    let encrypted_ballots = ballot_reader.get_all_encrypted_ballots(&election_pda, &program_id).await?;
    println!("Found {} encrypted ballots", encrypted_ballots.len());

    // Получаем публичный ключ выборов
    let election_public_key = ballot_reader.get_election_public_key(&election_pda).await?;

    // Подсчитываем результаты через Arcium
    println!("Submitting tally request to Arcium...");
    let tally_results = arcium_client.tally_election(encrypted_ballots.clone(), election_public_key).await?;
    println!("Tally completed: {:?}", tally_results.results);

    // Валидируем результаты
    println!("Validating tally results...");
    let is_valid = arcium_client.validate_tally(&encrypted_ballots, &tally_results, election_public_key).await?;
    
    if is_valid {
        println!("✅ Tally results are valid!");
        
        // Здесь можно опубликовать результаты в блокчейн
        // или сохранить их для дальнейшего использования
        
        for (vote_option, count) in tally_results.results {
            println!("Option {}: {} votes", vote_option, count);
        }
        println!("Total valid votes: {}", tally_results.total_valid_votes);
    } else {
        println!("❌ Tally results are invalid!");
    }

    Ok(())
}