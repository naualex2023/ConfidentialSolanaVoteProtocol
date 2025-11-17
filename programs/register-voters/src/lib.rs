#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;

use crate::state::*;

declare_id!("F9QNYA8Bp36Hg3N79k3C6NNuc98zAUtE1qv9nxMkAN9j");
//pub use self::registration as RegistrationProgram;
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

// ✅ ШАГ 1: Создаем публичную пустую структуру с уникальным именем.
// Это и есть тип 'T' для Program<'info, T> в другом крейте.
pub struct RegistrationProgramId; 

// ✅ ШАГ 2: Реализуем для нее трейт Id, используя ID программы, 
// объявленный в начале этого файла (declare_id!).
impl anchor_lang::Id for RegistrationProgramId {
    fn id() -> Pubkey {
        crate::ID
        // Если crate::ID не работает, используйте просто ID, если он в scope:
        // ID 
    }
}

// ✅ ШАГ 3: Убедитесь, что VoterProof публично экспортирован
pub use crate::state::VoterProof;
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