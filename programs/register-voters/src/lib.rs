// Stops Rust Analyzer complaining about missing configs
// See https://solana.stackexchange.com/questions/17777
#![allow(unexpected_cfgs)]
// Fix warning: use of deprecated method `anchor_lang::prelude::AccountInfo::<'a>::realloc`: Use AccountInfo::resize() instead
// See https://solana.stackexchange.com/questions/22979
#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_lang::system_program::ID;

// Предполагается, что эти модули существуют в вашем проекте
pub mod state {
    use anchor_lang::prelude::*;

    // Подберите значение в соответствии с реализацией на уровне проекта.
    pub const MAX_CANDIDATES: usize = 5; // Фиксированное число кандидатов
    pub const MAX_ITEMS_PER_CHUNK: usize = 500;
    pub const ELECTION_SEED: &[u8] = b"election";
    pub const NULLIFIER_SEED: &[u8] = b"nullifier";
    pub const ELECTION_SIGN_PDA_SEED: &[u8] = b"signer_account";
    pub const VOTER_REGISTRY_SEED: &[u8] = b"voter_registry"; 
    // Простые типы, которые требуются в этом файле.
    // При переносе реальной реализации из отдельного файла замените этот модуль.
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
#[account]
#[derive(InitSpace)]
pub struct VoterRegistry { // <-- ИМЯ, КОТОРОЕ ВЫ ИСПОЛЬЗУЕТЕ
    pub election: Pubkey,
    pub chunk_index: u32,
    
    // НОВОЕ ПОЛЕ: Счетчик добавленных хэшей
    pub count: u32, 

    // КРИТИЧЕСКОЕ ИЗМЕНЕНИЕ: ФИКСИРОВАННЫЙ МАССИВ
    // Это резервирует место сразу и не требует realloc при добавлении
    pub voter_hashes: [Pubkey; MAX_ITEMS_PER_CHUNK], 
    
    pub bump: u8,
}
// ...
impl VoterRegistry {
    // Обновляем расчет:
    pub const HEADER_SPACE: usize = 8 + 32 /* election */ + 4 /* chunk_index */ + 4 /* count */ + 1 /* bump */;
    pub const HASHES_SPACE: usize = MAX_ITEMS_PER_CHUNK * 32;
    pub const MAX_SPACE: usize = Self::HEADER_SPACE + Self::HASHES_SPACE;
}
}
pub mod error {
    use anchor_lang::prelude::*;

    /// Custom program errors for the voting program.
    #[error_code]
    pub enum VoteError {
        #[msg("Not authorized")]
        NotAuthorized,

        #[msg("Election is not in Draft state")]
        ElectionNotDraft,

        #[msg("Chunk is full")]
        ChunkFull,
    }
}

use crate::state::*; // Содержит Election, VoterChunk, NullifierAccount, константы и т.д.
use crate::error::VoteError; // Содержит ваши кастомные ошибки
// ... (структуры и импорты)

// 2. СТАНДАРТНЫЙ МОДУЛЬ (для register_voter)
#[program] // <-- ДОБАВИТЬ ВТОРОЙ МАКРОС
pub mod registration { // <-- НОВЫЙ МОДУЛЬ
    use super::*;

      // ----------------------
    // 2. Управление реестром
    // ----------------------

    /// Регистрирует группу голосующих в чанке.

    pub fn register_voters(
        ctx: Context<RegisterVoters>,
        _chunk_index: u32, // index уже находится в PDA, сохраняем для ясности
        voter_hashes: Vec<Pubkey>,
    ) -> Result<()> {
        // Меняем имя переменной с 'chunk' на 'registry' для ясности
        msg!("Registering {} voters in chunk {}", voter_hashes.len(), _chunk_index);
        let registry = &mut ctx.accounts.voter_registry; 
        let election = &ctx.accounts.election;

        // --- 1. Проверки безопасности ---
        msg!("Performing security checks...");
        // 1.1. Только создатель выборов может добавлять избирателей
        require_keys_eq!(
            election.creator,
            ctx.accounts.authority.key(),
            VoteError::NotAuthorized
        );
        msg!("Authority is authorized.");
        // 1.2. Выборы еще не начались
        require!(
            election.state == 0, // 0 - Draft
            VoteError::ElectionNotDraft
        );
        msg!("Election is in Draft state.");
        // --- 2. Проверка на переполнение чанка (до добавления) ---
        let total_new_hashes = voter_hashes.len();
        let current_count = registry.count as usize;

        require!(
            current_count.checked_add(total_new_hashes).is_some() && 
            current_count + total_new_hashes <= MAX_ITEMS_PER_CHUNK,
            VoteError::ChunkFull
        );
        msg!("Chunk has enough space for new voters.");
        // --- 3. Инициализация и заполнение данных (БЕЗ REALLOC) ---

        // Устанавливаем эти поля только при первой инициализации аккаунта
        registry.election = election.key(); 
        registry.chunk_index = _chunk_index; 
        msg!("Filling voter hashes into the registry...");
        // ЗАМЕНА: Используем цикл с прямой записью в массив вместо .extend()
        for (i, hash) in voter_hashes.into_iter().enumerate() {
            let index_to_write = current_count
                .checked_add(i)
                .ok_or(VoteError::ChunkFull)?; // Проверка переполнения
            msg!("Writing hash at index {}", index_to_write);
            // ПРЯМАЯ ЗАПИСЬ В ФИКСИРОВАННЫЙ МАССИВ. Это устраняет realloc.
            registry.voter_hashes[index_to_write] = hash;
            
            // Инкрементируем счетчик
            registry.count = registry.count.checked_add(1).ok_or(VoteError::ChunkFull)?;
        }
        
        // 4. Сохранение bump (только при init_if_needed)
        registry.bump = ctx.bumps.voter_registry;

        Ok(())
    }

      /// Инструкция для регистрации одного избирателя.
    /// ПРИМЕЧАНИЕ: Хэш избирателя передается как Pubkey (32 байта, Base58), 
    /// что позволяет Anchor автоматически его декодировать.
    pub fn register_voter(
        ctx: Context<RegisterVoters>,
        _chunk_index: u32, // index уже находится в PDA, сохраняем для ясности
        voter_hash: Pubkey, // ИЗМЕНЕНИЕ: Теперь принимаем Pubkey
    ) -> Result<()> {
        let registry = &mut ctx.accounts.voter_registry; 
        let election = &ctx.accounts.election;

        // --- 1. Проверки безопасности ---
        msg!("Performing security checks...");
        
        // 1.1. Только создатель выборов может добавлять избирателей
        require_keys_eq!(
            election.creator,
            ctx.accounts.authority.key(),
            VoteError::NotAuthorized
        );
        
        // 1.2. Выборы еще не начались
        require!(
            election.state == 0, // 0 - Draft
            VoteError::ElectionNotDraft
        );
        msg!("Checks passed. Current count: {}", registry.count);


        // 1.3. Проверка на переполнение чанка 
        let current_count = registry.count as usize;
        require!(
            current_count < MAX_ITEMS_PER_CHUNK,
            VoteError::ChunkFull
        );
        
        // --- 2. Декодирование Base58 в [u8; 32] ---
        // ЭТОТ ШАГ УДАЛЕН, ТАК КАК Pubkey УЖЕ ДЕКОДИРОВАЛ ДАННЫЕ.
        // Мы используем voter_hash напрямую, так как он уже является 32-байтовым объектом.
        
        // --- 3. Запись данных (БЕЗ REALLOC) ---

        // Устанавливаем эти поля только при первой инициализации аккаунта
        if registry.count == 0 {
            registry.election = election.key(); 
            registry.chunk_index = _chunk_index; 
        }

        // Прямая запись в фиксированный массив по текущему счетчику
        let index_to_write = current_count;
        
        // ПРИМЕЧАНИЕ: Здесь мы предполагаем, что в state.rs вы изменили 
        // VoterRegistry::voter_hashes на массив Pubkey.
        registry.voter_hashes[index_to_write] = voter_hash; 
        
        // Инкрементируем счетчик
        registry.count = registry.count.checked_add(1).ok_or(VoteError::ChunkFull)?;

        // Сохранение bump
        registry.bump = ctx.bumps.voter_registry;
        msg!("Voter hash recorded at index {}. New count: {}", index_to_write, registry.count);

        Ok(())
    }
}
// #[derive(Accounts)]
// pub struct RegisterVoters<'info> {
//     #[account(
//         init_if_needed,
//         payer = authority,
//         space = 8 + std::mem::size_of::<state::VoterRegistry>(),
//         seeds = [
//             VOTER_REGISTRY_SEED,
//             election.key().as_ref(),
//             _chunk_index.to_le_bytes().as_ref()
//         ],
//         bump
//     )]
//     pub voter_registry: Account<'info, state::VoterRegistry>,
    
//     #[account()]
//     pub election: Account<'info, state::Election>,
    
//     #[account(mut)]
//     pub authority: Signer<'info>,
    
//     pub system_program: Program<'info, System>,
// }
#[derive(Accounts)]
#[instruction(chunk_index: u32, voter_hashes: Vec<Pubkey>)]
pub struct RegisterVoters<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    // Проверяем, что authority = election.creator в самой функции
    #[account(mut)]
    pub election: Account<'info, Election>,
    
    #[account(
        init_if_needed,
        payer = authority,
        //space = 8 + VoterRegistry::MAX_SPACE, 
        space=16100, // Временно, замените на правильное значение
        seeds = [VOTER_REGISTRY_SEED, election.key().as_ref(), chunk_index.to_le_bytes().as_ref()],
        bump
    )]
    pub voter_registry: Box<Account<'info, VoterRegistry>>,
    
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
#[instruction(chunk_index: u32, voter_hash: Pubkey)]
pub struct RegisterVoter<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    // Проверяем, что authority = election.creator в самой функции
    #[account(mut)]
    pub election: Account<'info, Election>,
    
    #[account(
        init_if_needed,
        payer = authority,
        //space = 8 + VoterRegistry::MAX_SPACE, 
        space=16100, // Временно, замените на правильное значение
        seeds = [VOTER_REGISTRY_SEED, election.key().as_ref(), chunk_index.to_le_bytes().as_ref()],
        bump
    )]
    pub voter_registry: Box<Account<'info, VoterRegistry>>,
    
    pub system_program: Program<'info, System>,
}