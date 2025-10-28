use anchor_lang::prelude::*;
use ::borsh::{BorshDeserialize, BorshSerialize};
use solana_program::clock::UnixTimestamp;

// Константы
pub const MAX_CANDIDATES: usize = 5; // Фиксированное число кандидатов
pub const MAX_ITEMS_PER_CHUNK: usize = 500;
pub const ELECTION_SEED: &[u8] = b"election";
pub const VOTER_CHUNK_SEED: &[u8] = b"voter_chunk";
pub const NULLIFIER_SEED: &[u8] = b"nullifier";


// ------------------------------------------------------------------
// Структуры Аккаунтов
// ------------------------------------------------------------------

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, AnchorSerialize, AnchorDeserialize)]
pub enum ElectionState {
    Draft,
    Active,
    Tallying,
    Completed,
    Cancelled,
}

/// Главный аккаунт выборов (Election)
#[account]
#[derive(InitSpace)]
pub struct Election {
    pub creator: Pubkey,
    pub election_id: u64,
    #[max_len(50)]
    pub title: String,
    pub start_time: UnixTimestamp,
    pub end_time: UnixTimestamp,
    pub state: ElectionState,
    pub total_votes: u32,
    
    // Поля Arcium/MPC
    pub nonce: u128,
    /// Encrypted vote tallies: [32-byte ciphertext; MAX_CANDIDATES]
    pub encrypted_tally: [[u8; 32]; MAX_CANDIDATES], 
    /// Final decrypted result: [u64; MAX_CANDIDATES]
    pub final_result: [u64; MAX_CANDIDATES], 
}

/// Аккаунт для реестра голосующих (VoterChunk)
#[account]
pub struct VoterChunk {
    pub election: Pubkey,
    pub chunk_index: u32,
    #[max_len(MAX_ITEMS_PER_CHUNK)] 
    pub voter_hashes: Vec<[u8; 32]>, // Хеши зарегистрированных избирателей
    pub bump: u8, // Добавьте bump для полноты
}

impl VoterChunk {
    // Включая 8 байт для discriminator Anchor
    pub const HEADER_SIZE: usize = 8 + 32 /* election */ + 4 /* chunk_index */ + 1 /* bump */;
    
    // 4 байта для длины Vec + (32 байта * MAX_ITEMS_PER_CHUNK)
    pub const VEC_SIZE: usize = 4 + 32 * MAX_ITEMS_PER_CHUNK;
    
    pub const MAX_SPACE: usize = Self::HEADER_SIZE + Self::VEC_SIZE;
}

/// Аккаунт для предотвращения двойного голосования (NullifierAccount)
#[account]
#[derive(InitSpace)] // <<---- ЭТО КРИТИЧНО!
pub struct NullifierAccount {
    pub election_pda: Pubkey,
    pub nullifier_hash: [u8; 32],
    pub bump: u8, // <<---- ДОБАВЛЕНО
}

// Константа INIT_SPACE будет сгенерирована:
// 8 (дискриминатор) + 32 (Pubkey) + 32 ([u8; 32]) + 1 (u8 bump) = 73 байта