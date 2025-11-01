#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;

use crate::state::*;

declare_id!("CGZp3yAZwuL9WQbQYpWRgw3fTyXesExjtoSi7sfC29zu");

#[program]
pub mod registration {
    use super::*;

    // ИНСТРУКЦИЯ СТАЛА ПРОЩЕ
    pub fn register_voter(ctx: Context<RegisterVoter>, _chunk_index: u32, voter_hash: Pubkey) -> Result<()> {
        // Мы инициализируем аккаунт. 
        // Логика перенесена в макрос #[account(init ...)]
        // Если аккаунт уже существует, транзакция упадет.
        
        // Вы можете сохранить здесь ссылку на "выборы", если нужно
        // ctx.accounts.voter_proof.election = ctx.accounts.election.key();
        
        // Мы сохраняем хэш, чтобы его можно было прочитать (хотя он и так в адресе)
        ctx.accounts.voter_proof.voter_hash = voter_hash;
        
        msg!("Voter registered with hash: {}", voter_hash);
        Ok(())
    }

    // register_voters теперь не нужен или потребует 
    // передачи Vec<AccountInfo> для инициализации
}

// =========================================================================
// STATE
// =========================================================================
pub mod state {
    use anchor_lang::prelude::*;

    // 🛑 БОЛЬШЕ НЕТ ГИГАНТСКОЙ СТРУКТУРЫ
    pub const VOTER_REGISTRY_SEED: &[u8] = b"voters_registry"; 

    // ✅ НОВАЯ КРОШЕЧНАЯ СТРУКТУРА
    // Используем обычный Borsh (стандартный #[account])
    #[account]
    #[derive(InitSpace)]
    pub struct VoterProof {
        // Pubkey выборов, к которым относится этот хэш
        // pub election: Pubkey, // (32 байта)
        
        // Сам хэш, просто для удобства чтения
        pub voter_hash: Pubkey, // (32 байта)
    }
}

// =========================================================================
// ACCOUNTS
// =========================================================================

#[derive(Accounts)]
#[instruction(chunk_index: u32, voter_hash: Pubkey)] // chunk_index можно убрать, но оставим для совместимости с тестом
pub struct RegisterVoter<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    // ✅ НОВЫЙ PDA: ОДИН НА ХЭШ
    #[account(
        init, // Мы создаем новый аккаунт
        payer = authority,
        // Размер: 8 (дискриминатор) + 32 (voter_hash) = 40 байт
        space = 8 + VoterProof::INIT_SPACE, 
        // ✅ СИДЫ ТЕПЕРЬ ЗАВИСЯТ ОТ ХЭША, А НЕ ОТ ЧАНКА
        seeds = [
            VOTER_REGISTRY_SEED, 
            voter_hash.as_ref() // Используем хэш как сид
        ],
        bump
    )]
    // ✅ БОЛЬШЕ НЕТ AccountLoader, ИСПОЛЬЗУЕМ ОБЫЧНЫЙ Account
    pub voter_proof: Account<'info, VoterProof>, 
    
    pub system_program: Program<'info, System>,
}

// ... (структура RegisterVoters удалена для простоты) ...

// =========================================================================
// ERRORS
// =========================================================================
#[error_code]
pub enum ErrorCode {
    #[msg("Chunk is full")] // Ошибка больше не актуальна
    ChunkFull,
}