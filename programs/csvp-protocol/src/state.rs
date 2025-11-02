use anchor_lang::prelude::*;
//use ::borsh::{BorshDeserialize, BorshSerialize};
//use solana_program::clock::UnixTimestamp;

// Константы
pub const MAX_CANDIDATES: usize = 5; // Фиксированное число кандидатов
pub const MAX_ITEMS_PER_CHUNK: usize = 500;
pub const ELECTION_SEED: &[u8] = b"election";
// pub const VOTER_CHUNK_SEED: &[u8] = b"voter_chunk";
pub const VOTER_REGISTRY_SEED: &[u8] = b"voters_registry"; // <-- НОВОЕ ИМЯ
pub const NULLIFIER_SEED: &[u8] = b"nullifier";
pub const ELECTION_SIGN_PDA_SEED: &[u8] = b"signer_account";


// ------------------------------------------------------------------
// Структуры Аккаунтов
// ------------------------------------------------------------------

#[derive( Debug, Clone, PartialEq, AnchorSerialize, AnchorDeserialize)]
#[repr(u8)]
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
    pub start_time: u64,
    pub end_time: u64,
    pub state: u64,
    pub total_votes: u32,
    pub bump: u8, // <-- Необходим для сохранения bump-сида
    // Поля Arcium/MPC
    pub nonce: u128,
    /// Encrypted vote tallies: [32-byte ciphertext; MAX_CANDIDATES]
    pub encrypted_tally: [[u8; 32]; MAX_CANDIDATES], 
    /// Final decrypted result: [u64; MAX_CANDIDATES]
    pub final_result: [u64; MAX_CANDIDATES], 
}

/// Аккаунт для реестра голосующих (VoterChunk)
pub const HASH_LEN: usize = 32;

// #[account]
// #[derive(InitSpace)]
// pub struct VoterRegistry { // <-- ИМЯ, КОТОРОЕ ВЫ ИСПОЛЬЗУЕТЕ
//     pub election: Pubkey,
//     pub chunk_index: u32,
    
//     // НОВОЕ ПОЛЕ: Счетчик добавленных хэшей
//     pub count: u32, 

//     // КРИТИЧЕСКОЕ ИЗМЕНЕНИЕ: ФИКСИРОВАННЫЙ МАССИВ
//     // Это резервирует место сразу и не требует realloc при добавлении
//     pub voter_hashes: [Pubkey; MAX_ITEMS_PER_CHUNK], 
    
//     pub bump: u8,
// }
// // ...
// impl VoterRegistry {
//     // Обновляем расчет:
//     pub const HEADER_SPACE: usize = 8 + 32 /* election */ + 4 /* chunk_index */ + 4 /* count */ + 1 /* bump */;
//     pub const HASHES_SPACE: usize = MAX_ITEMS_PER_CHUNK * HASH_LEN;
//     pub const MAX_SPACE: usize = Self::HEADER_SPACE + Self::HASHES_SPACE;
// }

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