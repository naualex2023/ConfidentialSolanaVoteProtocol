// Stops Rust Analyzer complaining about missing configs
#![allow(unexpected_cfgs)]
// Fix warning: use of deprecated method `anchor_lang::prelude::AccountInfo::<'a>::realloc`: Use AccountInfo::resize() instead
#![allow(deprecated)]

use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;
use arcium_client::idl::arcium::types::CallbackAccount;

pub mod state;
pub mod error;

use crate::state::*; 
use crate::error::VoteError; 

// Offsets для MPC схем (должны совпадать с my_lib_enc_ixs.rs)
const COMP_DEF_OFFSET_INIT_VOTE_STATS: u32 = comp_def_offset("init_vote_stats");
const COMP_DEF_OFFSET_VOTE: u32 = comp_def_offset("vote");
const COMP_DEF_OFFSET_REVEAL: u32 = comp_def_offset("reveal_result");

// Типы, генерируемые Arcium IDL из my_lib_enc_ixs.rs
// (Предполагается, что они будут доступны после генерации)
pub struct InitVoteStatsOutput { pub field_0: Enc<Mxe, VoteStats> }
pub struct VoteOutput { pub field_0: Enc<Mxe, VoteStats> }
pub struct RevealResultOutput { pub field_0: [u64; MAX_CANDIDATES] }


declare_id!("GXvE4L1kKLdQZpGruFQbg9i8jR2GFBbZqDT3uvXAEfGs"); // Ваш Program ID

#[arcium_program]
pub mod confidential_voting {
    use super::*;

    // ------------------------------------
    // 1. Инициализация схем (Comp Defs)
    // ------------------------------------

    pub fn init_vote_stats_comp_def(ctx: Context<InitVoteStatsCompDef>) -> Result<()> {
        init_comp_def(ctx.accounts, true, 0, None, None)?;
        Ok(())
    }

    pub fn init_vote_comp_def(ctx: Context<InitVoteCompDef>) -> Result<()> {
        init_comp_def(ctx.accounts, true, 0, None, None)?;
        Ok(())
    }

    pub fn init_reveal_result_comp_def(ctx: Context<InitRevealResultCompDef>) -> Result<()> {
        init_comp_def(ctx.accounts, true, 0, None, None)?;
        Ok(())
    }
    
    // ----------------------
    // 2. Управление реестром
    // ----------------------

    /// Регистрирует группу голосующих в чанке.
    pub fn register_voters(
        ctx: Context<RegisterVoters>, 
        chunk_index: u32, 
        voter_hashes: Vec<[u8; 32]>
    ) -> Result<()> {
        let chunk = &mut ctx.accounts.voter_chunk;
        let election = &ctx.accounts.election;
        
        // 1. Проверка авторизации
        require_keys_eq!(
            election.creator, 
            ctx.accounts.authority.key(), 
            VoteError::NotAuthorized
        );

        // 2. Проверка заполненности чанка (исправлено использование InitSpace)
        let new_len = chunk.voter_hashes.len() + voter_hashes.len();
        if new_len > MAX_ITEMS_PER_CHUNK {
            return err!(VoteError::ChunkFull);
        }
        
        // 3. Заполнение данных
        chunk.election = election.key();
        chunk.chunk_index = chunk_index;
        chunk.voter_hashes.extend(voter_hashes);
        // bump устанавливается при init_if_needed
        
        Ok(())
    }

    // ----------------------
    // 3. Инициализация выборов (Arcium)
    // ----------------------

    /// Создает выборы и запускает MPC для инициализации encrypted_tally нулями.
    pub fn initialize_election(
        ctx: Context<InitializeElection>, 
        election_id: u64,
        title: String,
        start_time: UnixTimestamp,
        end_time: UnixTimestamp,
    ) -> Result<()> {
        let election = &mut ctx.accounts.election_account;
        
        // Базовая инициализация аккаунта выборов
        election.creator = *ctx.accounts.authority.key;
        election.election_id = election_id;
        election.title = title;
        election.start_time = start_time;
        election.end_time = end_time;
        election.state = ElectionState::Draft;
        election.total_votes = 0;
        // bump устанавливается макросом `init`

        // 1. Создаем случайный nonce (для Arcium)
        let clock = Clock::get()?;
        let nonce = clock.unix_timestamp as u128; // Простое, но достаточно для примера
        election.nonce = nonce;
        
        // 2. Настраиваем аргументы для init_vote_stats (нет аргументов, кроме владельца)
        let args = vec![];
        
        // 3. Устанавливаем bump для sign_pda
        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

        // 4. Запуск MPC: init_vote_stats
        queue_computation(
            ctx.accounts, 
            0, // computation_offset
            args, 
            None, // arcium_proof
            vec![InitCallback::callback_ix(&[CallbackAccount {
                pubkey: ctx.accounts.election_account.key(),
                is_writable: true,
            }])],
        )?;

        msg!("Election account created. Awaiting Arcium callback to set initial tally.");
        Ok(())
    }

    /// Коллбэк после `init_vote_stats`: записывает зашифрованный массив нулей.
    #[arcium_callback(encrypted_ix = "init_vote_stats")]
    pub fn init_callback(
        ctx: Context<InitCallback>, 
        output: ComputationOutputs<InitVoteStatsOutput>
    ) -> Result<()> {
        let o = match output {
            ComputationOutputs::Success(InitVoteStatsOutput { field_0 }) => field_0,
            _ => return err!(VoteError::AbortedComputation),
        };

        msg!("Arcium callback received: Setting initial encrypted tally and nonce.");
        let election = &mut ctx.accounts.election_account;

        // Декодируем и сохраняем шифротексты и nonce
        election.encrypted_tally = o.ciphertexts.try_into().map_err(|_| ErrorCode::ConstraintRaw)?;
        election.nonce = o.nonce;
        election.state = ElectionState::Active; // Выборы готовы к голосованию

        Ok(())
    }

    // ----------------------
    // 4. Голосование (Arcium)
    // ----------------------

    /// Принимает голос: проверяет регистрацию, Nullifier и запускает MPC `vote`.
    pub fn cast_vote(
        ctx: Context<CastVote>,
        voter_chunk_index: u32,
        // Аргументы для Enc<Shared, UserVote> (голоса)
        vote_ciphertext: [u8; 32], 
        vote_encryption_pubkey: [u8; 32],
        vote_nonce: u128,
        // Аргументы для проверок
        nullifier_hash: [u8; 32],
        voter_hash: [u8; 32],
    ) -> Result<()> {
        let election = &mut ctx.accounts.election_account;
        let clock = Clock::get()?;
        
        // 1. ПРОВЕРКА ПЕРИОДА ВЫБОРОВ
        require!(
            election.state == ElectionState::Active &&
            clock.unix_timestamp >= election.start_time &&
            clock.unix_timestamp <= election.end_time,
            VoteError::InvalidElectionPeriod
        );

        // 2. ПРОВЕРКА РЕГИСТРАЦИИ (Voter Chunk)
        // `voter_chunk` уже верифицирован (seeds, has_one) в `#[derive(Accounts)]`
        require!(
            ctx.accounts.voter_chunk.voter_hashes.contains(&voter_hash),
            VoteError::VoterNotRegistered
        );

        // 3. ПРОВЕРКА ДВОЙНОГО ГОЛОСОВАНИЯ (Nullifier Account)
        // `init` в `#[derive(Accounts)]` атомарно создает PDA. 
        // Если он уже существует, транзакция упадет с ProgramError::AccountAlreadyInitialized.
        let nullifier = &mut ctx.accounts.nullifier_account;
        nullifier.election_pda = election.key();
        nullifier.nullifier_hash = nullifier_hash;
        nullifier.bump = ctx.bumps.nullifier_account; // Сохраняем bump для дебага

        // 4. ПОДГОТОВКА АРГУМЕНТОВ ДЛЯ MPC `vote`
        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
        
        let mut args = vec![
            // Аргумент 1 (vote_ctxt): Enc<Shared, UserVote>
            Argument::ArcisPubkey(vote_encryption_pubkey),
            Argument::PlaintextU128(vote_nonce),
            Argument::Encrypted(vote_ciphertext), 

            // Аргумент 2 (vote_stats_ctxt): Enc<Mxe, VoteStats>
            Argument::PlaintextU128(election.nonce),
        ];
        
        // Добавляем N шифротекстов из `encrypted_tally`
        for ct in election.encrypted_tally {
            args.push(Argument::Encrypted(ct));
        }

        // 5. ЗАПУСК MPC для агрегации голоса
        queue_computation(
            ctx.accounts,
            0, // computation_offset
            args,
            None, // arcium_proof
            vec![VoteCallback::callback_ix(&[CallbackAccount {
                pubkey: ctx.accounts.election_account.key(),
                is_writable: true,
            }])],
        )?;
        
        election.total_votes += 1; // Увеличиваем публичный счетчик
        
        msg!("Vote cast successfully. Awaiting Arcium callback to update tally.");
        Ok(())
    }

    /// Коллбэк: записывает новый обновленный зашифрованный счет (`encrypted_tally`).
    #[arcium_callback(encrypted_ix = "vote")]
    pub fn vote_callback(
        ctx: Context<VoteCallback>,
        output: ComputationOutputs<VoteOutput>,
    ) -> Result<()> {
        let o = match output {
            ComputationOutputs::Success(VoteOutput { field_0 }) => field_0,
            _ => return err!(VoteError::AbortedComputation),
        };

        msg!("Arcium callback received: Updating encrypted tally.");
        let election = &mut ctx.accounts.election_account;

        // Обновляем зашифрованный счет и nonce
        election.encrypted_tally = o.ciphertexts.try_into().map_err(|_| ErrorCode::ConstraintRaw)?;
        election.nonce = o.nonce;
        
        Ok(())
    }
    
    // ----------------------
    // 5. Раскрытие результата (Arcium)
    // ----------------------

    /// Запускает MPC `reveal_result` (только создатель).
    pub fn reveal_result(ctx: Context<RevealResult>) -> Result<()> {
        let election = &mut ctx.accounts.election_account;
        let clock = Clock::get()?;

        // 1. ПРОВЕРКА: Только создатель и выборы должны быть завершены
        require_keys_eq!(
            election.creator, 
            ctx.accounts.authority.key(), 
            VoteError::NotAuthorized
        );
        
        require!(
            clock.unix_timestamp > election.end_time || election.state == ElectionState::Tallying,
            VoteError::InvalidElectionPeriod
        );

        // 2. ПОДГОТОВКА АРГУМЕНТОВ ДЛЯ MPC `reveal_result`
        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

        let mut args = vec![
            // Аргумент 1 (vote_stats_ctxt): Enc<Mxe, VoteStats>
            Argument::PlaintextU128(election.nonce),
        ];
        for ct in election.encrypted_tally {
            args.push(Argument::Encrypted(ct));
        }

        // 3. ЗАПУСК MPC
        queue_computation(
            ctx.accounts,
            0, // computation_offset
            args,
            None, // arcium_proof
            vec![RevealCallback::callback_ix(&[CallbackAccount {
                pubkey: ctx.accounts.election_account.key(),
                is_writable: true,
            }])],
        )?;

        election.state = ElectionState::Tallying;
        msg!("Reveal requested. Awaiting Arcium callback for final results.");

        Ok(())
    }


    /// Коллбэк: записывает финальный РАСШИФРОВАННЫЙ результат.
    #[arcium_callback(encrypted_ix = "reveal_result")]
    pub fn reveal_callback(
        ctx: Context<RevealCallback>,
        output: ComputationOutputs<RevealResultOutput>,
    ) -> Result<()> {
        let public_results = match output {
            // `field_0` здесь - это `[u64; MAX_CANDIDATES]`
            ComputationOutputs::Success(RevealResultOutput { field_0 }) => field_0,
            _ => return err!(VoteError::AbortedComputation),
        };

        msg!("Arcium callback received: Saving public final results.");
        let election = &mut ctx.accounts.election_account;

        // Записываем публичный, расшифрованный результат
        election.final_result = public_results.try_into().map_err(|_| ErrorCode::ConstraintRaw)?;
        election.state = ElectionState::Completed;

        Ok(())
    }
}

// ------------------------------------------------------------------
// Структуры Контекстов (Accounts Structs)
// ------------------------------------------------------------------

// --- Comp Defs ---

#[init_computation_definition_accounts("init_vote_stats", payer)]
#[derive(Accounts)]
pub struct InitVoteStatsCompDef<'info> {
    // Аккаунты добавляются макросом
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[init_computation_definition_accounts("vote", payer)]
#[derive(Accounts)]
pub struct InitVoteCompDef<'info> {
    // Аккаунты добавляются макросом
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[init_computation_definition_accounts("reveal_result", payer)]
#[derive(Accounts)]
pub struct InitRevealResultCompDef<'info> {
    // Аккаунты добавляются макросом
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}


// --- Регистрация ---

#[derive(Accounts)]
#[instruction(chunk_index: u32)]
pub struct RegisterVoters<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    #[account(mut)]
    pub election: Account<'info, Election>,
    
    #[account(
        init_if_needed,
        payer = authority,
        // Использование MAX_SPACE для корректного резервирования места
        space = VoterChunk::MAX_SPACE, 
        seeds = [VOTER_CHUNK_SEED, election.key().as_ref(), chunk_index.to_le_bytes().as_ref()],
        bump
    )]
    pub voter_chunk: Account<'info, VoterChunk>,
    
    pub system_program: Program<'info, System>,
}

// --- Инициализация Выборов ---

#[queue_computation_accounts("init_vote_stats", authority)]
#[derive(Accounts)]
#[instruction(election_id: u64)]
pub struct InitializeElection<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    #[account(
        init,
        payer = authority,
        space = 8 + Election::INIT_SPACE, // INIT_SPACE из state.rs
        seeds = [ELECTION_SEED, authority.key().as_ref(), election_id.to_le_bytes().as_ref()],
        bump
    )]
    pub election_account: Account<'info, Election>,
    
    // Аккаунты Arcium, system_program и arcium_program добавляются макросом
}

#[callback_accounts("init_vote_stats")]
#[derive(Accounts)]
pub struct InitCallback<'info> {
    // Аккаунты Arcium добавляются макросом
    #[account(mut)]
    /// CHECK: election_account, верифицируется Arcium
    pub election_account: Account<'info, Election>,
}

// --- Голосование ---

#[queue_computation_accounts("vote", voter)]
#[derive(Accounts)]
#[instruction(
    voter_chunk_index: u32,
    vote_ciphertext: [u8; 32],
    vote_encryption_pubkey: [u8; 32],
    vote_nonce: u128,
    nullifier_hash: [u8; 32], 
    voter_hash: [u8; 32]
)]
pub struct CastVote<'info> {
    #[account(mut)]
    pub voter: Signer<'info>,
    
    #[account(mut)]
    pub election_account: Account<'info, Election>,
    
    // Аккаунт VoterChunk (для проверки регистрации)
    // Добавлены проверки seeds и has_one для безопасности.
    #[account(
        seeds = [
            VOTER_CHUNK_SEED, 
            election_account.key().as_ref(), 
            voter_chunk_index.to_le_bytes().as_ref()
        ],
        bump = voter_chunk.bump,
        has_one = election_account // Проверяем, что чанк принадлежит этим выборам
    )]
    pub voter_chunk: Account<'info, VoterChunk>, 
    
    // Nullifier (init) - АТОМАРНОЕ предотвращение двойного голосования
    #[account(
        init, // init = атомарная проверка на существование (ProgramError::AccountAlreadyInitialized)
        payer = voter,
        space = 8 + NullifierAccount::INIT_SPACE,
        seeds = [NULLIFIER_SEED, election_account.key().as_ref(), nullifier_hash.as_ref()],
        bump,
    )]
    pub nullifier_account: Account<'info, NullifierAccount>,
    
    // Аккаунты Arcium, system_program и arcium_program добавляются макросом
}

#[callback_accounts("vote")]
#[derive(Accounts)]
pub struct VoteCallback<'info> {
    // Аккаунты Arcium добавляются макросом
    #[account(mut)]
    /// CHECK: election_account, верифицируется Arcium
    pub election_account: Account<'info, Election>,
}

// --- Раскрытие Результатов ---

#[queue_computation_accounts("reveal_result", authority)]
#[derive(Accounts)]
pub struct RevealResult<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    #[account(
        mut,
        has_one = authority // Только создатель может раскрыть результаты (проверка в instruction)
    )]
    pub election_account: Account<'info, Election>,

    // Аккаунты Arcium, system_program и arcium_program добавляются макросом
}

#[callback_accounts("reveal_result")]
#[derive(Accounts)]
pub struct RevealCallback<'info> {
    // Аккаунты Arcium добавляются макросом
    #[account(mut)]
    /// CHECK: election_account, верифицируется Arcium
    pub election_account: Account<'info, Election>,
}