// Stops Rust Analyzer complaining about missing configs
// See https://solana.stackexchange.com/questions/17777
#![allow(unexpected_cfgs)]
// Fix warning: use of deprecated method `anchor_lang::prelude::AccountInfo::<'a>::realloc`: Use AccountInfo::resize() instead
// See https://solana.stackexchange.com/questions/22979
#![allow(deprecated)]

use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;
use arcium_client::idl::arcium::types::CallbackAccount;
// use crate::__cpi_client_accounts_reveal_result_callback::RevealResultCallback;
// use crate::__cpi_client_accounts_vote_callback::VoteCallback;
// use crate::__cpi_client_accounts_init_vote_stats_callback::InitVoteStatsCallback;

// Предполагается, что эти модули существуют в вашем проекте
pub mod state;
pub mod error;

use crate::state::*; // Содержит Election, VoterChunk, NullifierAccount, константы и т.д.
use crate::error::VoteError; // Содержит ваши кастомные ошибки

// Offsets для MPC схем (должны совпадать с my_lib_enc_ixs.rs)
const COMP_DEF_OFFSET_INIT_VOTE_STATS: u32 = comp_def_offset("init_vote_stats");
const COMP_DEF_OFFSET_VOTE: u32 = comp_def_offset("vote");
const COMP_DEF_OFFSET_REVEAL: u32 = comp_def_offset("reveal_result");

declare_id!("GXvE4L1kKLdQZpGruFQbg9i8jR2GFBbZqDT3uvXAEfGs"); // Ваш Program ID

#[arcium_program]
pub mod csvp_protocol {
    //use std::env::args;

    use super::*;

    // ------------------------------------
    // 1. Инициализация схем (Boilerplate)
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
        voter_hashes: Vec<[u8; 32]>,
    ) -> Result<()> {
        let chunk = &mut ctx.accounts.voter_registry;
        let election = &ctx.accounts.election;

        // 1. Проверка: Только создатель выборов может добавлять избирателей
        require_keys_eq!(
            election.creator,
            ctx.accounts.authority.key(),
            VoteError::NotAuthorized
        );

        // 2. Проверка: Чанк не переполнен
        require!(
            chunk.voter_hashes.len() + voter_hashes.len() <= MAX_ITEMS_PER_CHUNK,
            VoteError::ChunkFull
        );
        
        // 3. Проверка: Выборы еще не начались
        require!(
            election.state == 0,//ElectionState::Draft,
            VoteError::ElectionNotDraft
        );

        // 4. Инициализация и заполнение данных
        chunk.election = election.key();
        chunk.chunk_index = chunk_index;
        chunk.voter_hashes.extend(voter_hashes);
        chunk.bump = ctx.bumps.voter_registry;

        Ok(())
    }

    // ----------------------
    // 3. Инициализация выборов (Arcium)
    // ----------------------

    /// Создает выборы и запускает MPC для инициализации encrypted_tally нулями.
    pub fn init_election(
        ctx: Context<InitializeElection>,
        computation_offset: u64,
        election_id: u64,
        title: String,
        start_time: u64,
        end_time: u64,
        nonce: u128,
    ) -> Result<()> {
        msg!("Initializing new election...");
        let election = &mut ctx.accounts.election_account;

        election.creator = *ctx.accounts.authority.key;
        election.election_id = election_id;
        election.title = title;
        election.start_time = start_time;
        election.end_time = end_time;
        election.state = 0;//ElectionState::Draft; // Начинаем с Draft
        election.total_votes = 0;
        election.nonce = nonce;
        election.bump = ctx.bumps.election_account;
        // `nonce` и `encrypted_tally` будут установлены коллбэком
           // --- ОБЯЗАТЕЛЬНАЯ ИНИЦИАЛИЗАЦИЯ ВСЕХ ПОЛЕЙ ---
        // Инициализируем массивы нулями. Это необходимо для успешной сериализации.
        election.encrypted_tally = [[0; 32]; MAX_CANDIDATES];
        election.final_result = [0; MAX_CANDIDATES];
        // ----------------------------------------------
        let args = vec![Argument::PlaintextU128(nonce)];

        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

        // Запуск MPC: init_vote_stats
        // Эта схема не принимает аргументов (mxe: Mxe - это владелец, а не arg)
        queue_computation(
            ctx.accounts, // Используем ArciumQueueComputationAccounts
            computation_offset, // computation_offset (0 для нового)
            args, // Нет входных аргументов
            None, // Нет arcium_proof
            vec![InitVoteStatsCallback::callback_ix(&[CallbackAccount {
                pubkey: ctx.accounts.election_account.key(),
                is_writable: true,
            }])],
        )?;

        msg!("Election account created. Awaiting Arcium callback to set initial tally.");
        Ok(())
    }

    /// Коллбэк после `init_vote_stats`: записывает зашифрованный массив нулей.
    #[arcium_callback(encrypted_ix = "init_vote_stats")]
    pub fn init_vote_stats_callback(
        ctx: Context<InitVoteStatsCallback>,
        output: ComputationOutputs<InitVoteStatsOutput>,
    ) -> Result<()> {
        let o = match output {
            ComputationOutputs::Success(InitVoteStatsOutput { field_0 }) => field_0,
            _ => return Err(ErrorCode::AbortedComputation.into()),
        };

        msg!("Arcium callback received: Setting initial encrypted tally and nonce.");
        let election = &mut ctx.accounts.election_account;

        // Проверяем, что Arcium вернул правильное кол-во шифротекстов
        require!(
            o.ciphertexts.len() == MAX_CANDIDATES,
            VoteError::InvalidTallySize
        );

        // Сохраняем зашифрованные нули и nonce
        election.encrypted_tally = o.ciphertexts.try_into().map_err(|_| ErrorCode::ConstraintRaw)?;
        election.nonce = o.nonce;
        election.state = 1;//ElectionState::Active; // Выборы готовы к голосованию

        Ok(())
    }

    // ----------------------
    // 4. Голосование (Arcium)
    // ----------------------

    /// Принимает голос: проверяет регистрацию, Nullifier и запускает MPC `vote`.
    pub fn cast_vote(
        ctx: Context<CastVote>,
        computation_offset: u64,
        voter_chunk_index: u32,
        // Аргументы для `Enc<Shared, UserVote>`
        vote_ciphertext: [u8; 32], // Зашифрованный `UserVote { candidate_index }`
        vote_encryption_pubkey: [u8; 32],
        vote_nonce: u128,
        // Аргументы для проверок
        nullifier_hash: [u8; 32],
        voter_hash: [u8; 32],
    ) -> Result<()> {
        let election = & ctx.accounts.election_account;
        let el_key = election.key();
        let clock = Clock::get()?;
        
        // 1. ПРОВЕРКА ПЕРИОДА ВЫБОРОВ
        require!(
            election.state == 1 && //ElectionState::Active &&
            clock.unix_timestamp as u64 >= election.start_time &&
            clock.unix_timestamp as u64 <= election.end_time,
            VoteError::InvalidElectionPeriod
        );

        // 2. ПРОВЕРКА РЕГИСТРАЦИИ (Voter Chunk)
        // `voter_chunk` уже верифицирован (seeds, has_one) в `#[derive(Accounts)]`
        require!(
            ctx.accounts.voter_registry.voter_hashes.contains(&voter_hash),
            VoteError::VoterNotRegistered
        );

        // 3. ПРОВЕРКА ДВОЙНОГО ГОЛОСОВАНИЯ (Nullifier Account)
        // `init` в `#[derive(Accounts)]` атомарно создает PDA.
        // Если он уже существует, транзакция упадет (AccountAlreadyInitialized).
        let nullifier = &mut ctx.accounts.nullifier_account;
        nullifier.election_pda = election.key();
        nullifier.nullifier_hash = nullifier_hash;
        nullifier.bump = ctx.bumps.nullifier_account;

        // 4. ПОДГОТОВКА АРГУМЕНТОВ ДЛЯ MPC `vote`
        //ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
        
        let mut args = vec![
            // Аргумент 1: Enc<Shared, UserVote>
            Argument::ArcisPubkey(vote_encryption_pubkey),
            Argument::PlaintextU128(vote_nonce),
            Argument::EncryptedU64(vote_ciphertext), // т.к. UserVote { u64 } - это 1 шифротекст

            // Аргумент 2: Enc<Mxe, VoteStats>
            Argument::PlaintextU128(election.nonce),
        ];
        
        // Добавляем N шифротекстов из `encrypted_tally`
        for ct in election.encrypted_tally {
            args.push(Argument::EncryptedU64(ct));
        }

        // 5. ЗАПУСК MPC для агрегации голоса
        queue_computation(
            ctx.accounts,
            computation_offset, // computation_offset
            args,
            None, // arcium_proof
            vec![VoteCallback::callback_ix(&[CallbackAccount {
                pubkey: el_key,
                is_writable: true,
            }])],
        )?;
        
              
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

        require!(
            o.ciphertexts.len() == MAX_CANDIDATES,
            VoteError::InvalidTallySize
        );
        
        // Обновляем зашифрованный счет и nonce
        election.encrypted_tally = o.ciphertexts.try_into().map_err(|_| ErrorCode::ConstraintRaw)?;
        election.nonce = o.nonce;
        election.total_votes += 1; // Увеличиваем публичный счетчик
        
        Ok(())
    }
    
    // ----------------------
    // 5. Раскрытие результата (Arcium)
    // ----------------------

    /// Запускает MPC `reveal_result` (только создатель).
    pub fn reveal_result(ctx: Context<RevealResult>,
        computation_offset: u64,
        id: u32,) -> Result<()> {
        // require!(
        //     ctx.accounts.payer.key() == ctx.accounts.election_account.authority,
        //     VoteError::InvalidAuthority
        // );

        // msg!("Revealing voting result for election with id {}", id);
        let election = &mut ctx.accounts.election_account;
        
        // 1. ПРОВЕРКА (уже сделана в `has_one = authority`)
        
        // 2. ПРОВЕРКА: Выборы должны быть завершены (или в процессе подсчета)
        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp as u64 > election.end_time || election.state == 2, //ElectionState::Tallying,
            VoteError::InvalidElectionPeriod
        );

        // 3. ПОДГОТОВКА АРГУМЕНТОВ ДЛЯ MPC `reveal_result`
        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

        let mut args = vec![
            // Аргумент 1: Enc<Mxe, VoteStats> (финальный счет)
            Argument::PlaintextU128(election.nonce),
        ];
        for ct in election.encrypted_tally {
            args.push(Argument::EncryptedU64(ct));
        }
        election.state = 2;//ElectionState::Tallying;
        // 4. ЗАПУСК MPC
        queue_computation(
            ctx.accounts,
            computation_offset, // computation_offset
            args,
            None, // arcium_proof
            vec![RevealResultCallback::callback_ix(&[CallbackAccount {
                pubkey: ctx.accounts.election_account.key(),
                is_writable: true,
            }])],
        )?;
        
        msg!("Reveal requested. Awaiting Arcium callback for final results.");

        Ok(())
    }


    /// Коллбэк: записывает финальный РАСШИФРОВАННЫЙ результат.
    #[arcium_callback(encrypted_ix = "reveal_result")]
    pub fn reveal_result_callback(
        ctx: Context<RevealResultCallback>,
        output: ComputationOutputs<RevealResultOutput>,
    ) -> Result<()> {
        let public_results = match output {
            // `field_0` здесь - это `[u64; MAX_CANDIDATES]`
            ComputationOutputs::Success(RevealResultOutput { field_0 }) => field_0,
            _ => return err!(VoteError::AbortedComputation),
        };

        msg!("Arcium callback received: Saving public final results.");
        let election = &mut ctx.accounts.election_account;

        require!(
            public_results.len() == MAX_CANDIDATES,
            VoteError::InvalidTallySize
        );

        // Записываем публичный, расшифрованный результат
        election.final_result = public_results.try_into().map_err(|_| ErrorCode::ConstraintRaw)?;
        election.state = 3;//ElectionState::Completed;

        Ok(())
    }
}

// ------------------------------------------------------------------
// Структуры Контекстов (Accounts Structs)
// ------------------------------------------------------------------

// --- Boilerplate: Инициализация Схем ---

#[init_computation_definition_accounts("init_vote_stats", payer)]
#[derive(Accounts)]
pub struct InitVoteStatsCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account,
    pub comp_def_account: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[init_computation_definition_accounts("vote", payer)]
#[derive(Accounts)]
pub struct InitVoteCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account,
    pub comp_def_account: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[init_computation_definition_accounts("reveal_result", payer)]
#[derive(Accounts)]
pub struct InitRevealResultCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account,
    pub comp_def_account: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}


// --- Контекст: Регистрация избирателей ---

#[derive(Accounts)]
#[instruction(chunk_index: u32, voter_hashes: Vec<[u8; 32]>)]
pub struct RegisterVoters<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    // Проверяем, что authority = election.creator в самой функции
    #[account(mut)]
    pub election: Account<'info, Election>,
    
    #[account(
        init_if_needed,
        payer = authority,
        space = 8 + VoterRegistry::MAX_SPACE, 
        seeds = [VOTER_REGISTRY_SEED, election.key().as_ref(), chunk_index.to_le_bytes().as_ref()],
        bump
    )]
    pub voter_registry: Box<Account<'info, VoterRegistry>>,
    
    pub system_program: Program<'info, System>,
}

// --- Контекст: Инициализация Выборов (Arcium) ---

#[queue_computation_accounts("init_vote_stats", authority)]
#[derive(Accounts)]
#[instruction(computation_offset: u64,election_id: u64)]
//#[instruction(computation_offset: u64, id: u32)]
pub struct InitializeElection<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
       #[account(
        init_if_needed, // 👈 Обязательно init
        payer = authority,
        space = 9, 
        seeds = [&SIGN_PDA_SEED],
        bump, // 👈 Обязательно bump
        address = derive_sign_pda!(),
    )]
    pub sign_pda_account: Account<'info, SignerAccount>,
    // pub sign_pda_account: Account<'info, SignerAccount>,
    // #[account(
    //     address = derive_mxe_pda!()
    // )]
    // pub mxe_account: Account<'info, MXEAccount>,
    #[account(
        address = derive_mxe_pda!()
    )]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    // 🔥 КОРРЕКТНАЯ ДЕКЛАРАЦИЯ SIGN PDA:
    // #[account(
    //     init, // 👈 Обязательно init
    //     payer = authority,
    //     space = 8 + Election::INIT_SPACE, 
    //     seeds = [SIGN_PDA_SEED, election_account.key().as_ref()],
    //     bump // 👈 Обязательно bump
    // )]
    // pub sign_pda_account: Account<'info, SignerAccount>,
    #[account(
        mut,
        address = derive_mempool_pda!()
    )]
    /// CHECK: mempool_account, checked by the arcium program
    pub mempool_account: UncheckedAccount<'info>,
    #[account(
        mut,
        address = derive_execpool_pda!()
    )]
    /// CHECK: executing_pool, checked by the arcium program
    pub executing_pool: UncheckedAccount<'info>,
    #[account(
        mut,
        address = derive_comp_pda!(computation_offset)
    )]
    /// CHECK: computation_account, checked by the arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(
        address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_VOTE_STATS)
    )]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(
        mut,
        address = derive_cluster_pda!(mxe_account)
    )]
    pub cluster_account: Account<'info, Cluster>,
    #[account(
        mut,
        address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS,
    )]
    pub pool_account: Account<'info, FeePool>,
    #[account(
        address = ARCIUM_CLOCK_ACCOUNT_ADDRESS,
    )]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    #[account(
        init,
        payer = authority,
        //space = 8 + Election::INIT_SPACE, // Убедитесь, что INIT_SPACE в state.rs верный
        space=500, // Временно, замените на правильное значение
        seeds = [ELECTION_SEED, authority.key().as_ref(), election_id.to_le_bytes().as_ref()],
        bump
    )]
    pub election_account: Account<'info, Election>,
    
    // // system_program, arcium_program и все аккаунты Arcium
    // // автоматически добавляются макросом `queue_computation_accounts`
    // pub system_program: Program<'info, System>,
    // pub arcium_program: Program<'info, Arcium>,
    
    // #[account(
    //     init,
    //     payer = payer,
    //     space = 8 + PollAccount::INIT_SPACE,
    //     seeds = [b"poll", payer.key().as_ref(), id.to_le_bytes().as_ref()],
    //     bump,
    // )]
    // pub poll_acc: Account<'info, PollAccount>,
}

#[callback_accounts("init_vote_stats")]
#[derive(Accounts)]
pub struct InitVoteStatsCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_VOTE_STATS))]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(address = ::anchor_lang::solana_program::sysvar::instructions::ID)]
    /// CHECK: instructions_sysvar
    pub instructions_sysvar: AccountInfo<'info>,
    
    /// CHECK: election_account, верифицируется Arcium
    #[account(mut)]
    pub election_account: Account<'info, Election>,
}

// --- Контекст: Голосование (Arcium) ---

#[queue_computation_accounts("vote", voter)]
#[derive(Accounts)]
#[instruction(
    voter_chunk_index: u32,
    vote_ciphertext: [u8; 32],
    vote_encryption_pubkey: [u8; 32],
    vote_nonce: u128,
    nullifier_hash: [u8; 32], 
    voter_hash: [u8; 32],computation_offset: u64
)]
pub struct CastVote<'info> {
    #[account(mut)]
    pub voter: Signer<'info>,
        //pub sign_pda_account: Account<'info, SignerAccount>,
// 🔥 ДОБАВИТЬ ЭТОТ АККАУНТ ДЛЯ ПОДПИСИ MPC
    // Он должен быть помечен #[account(mut)] для использования в queue_computation
    #[account(
        mut, 
        // Ищем PDA, используя те же сиды, что и при инициализации
        seeds = [ELECTION_SIGN_PDA_SEED, election_account.key().as_ref()],
        // Указываем, что бамп должен совпадать с полем в аккаунте
        bump = sign_pda_account.bump, 
    )]
    // ВНИМАНИЕ: Если вы переименовали его в `mpc_signer_pda` в InitializeElection,
    // вы должны использовать здесь то же имя: `mpc_signer_pda`!
    pub sign_pda_account: Account<'info, SignerAccount>,
    #[account(
        address = derive_mxe_pda!()
    )]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(
        mut,
        address = derive_mempool_pda!()
    )]
    /// CHECK: mempool_account, checked by the arcium program
    pub mempool_account: UncheckedAccount<'info>,
    #[account(
        mut,
        address = derive_execpool_pda!()
    )]
    /// CHECK: executing_pool, checked by the arcium program
    pub executing_pool: UncheckedAccount<'info>,
    #[account(
        mut,
        address = derive_comp_pda!(computation_offset)
    )]
    /// CHECK: computation_account, checked by the arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(
        address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_VOTE_STATS)
    )]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(
        mut,
        address = derive_cluster_pda!(mxe_account)
    )]
    pub cluster_account: Account<'info, Cluster>,
    #[account(
        mut,
        address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS,
    )]
    pub pool_account: Account<'info, FeePool>,
    #[account(
        address = ARCIUM_CLOCK_ACCOUNT_ADDRESS,
    )]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    // #[account(
    //     init,
    //     payer = authority,
    //     space = 8 + Election::INIT_SPACE, // Убедитесь, что INIT_SPACE в state.rs верный
    //     seeds = [ELECTION_SEED, authority.key().as_ref(), election_id.to_le_bytes().as_ref()],
    //     bump
    // )]
    // pub election_account: Account<'info, Election>,
    #[account(mut)]
    pub election_account: Account<'info, Election>,
    // #[account(mut,
    //     seeds = [b"election", creator.key().as_ref(), election_id.to_le_bytes().as_ref()],
    //     bump = election_account.bump,
    //     has_one = creator
    // )]
    // pub election_account: Account<'info, Election>,
    #[account(
        address = voter_registry.election,
    )]
    pub election: UncheckedAccount<'info>,
    // Аккаунт VoterChunk (для проверки регистрации)
    #[account(
        seeds = [
            VOTER_REGISTRY_SEED, 
            election_account.key().as_ref(), 
            voter_chunk_index.to_le_bytes().as_ref()
        ],
        bump = voter_registry.bump,
        has_one = election, // Проверяем, что чанк принадлежит этим выборам
    )]
    pub voter_registry: Box<Account<'info, VoterRegistry>>, 
    
    // Nullifier (init) - предотвращение двойного голосования
    #[account(
        init, // init = атомарная проверка на существование
        payer = voter,
        space = 8 + NullifierAccount::INIT_SPACE,
        seeds = [NULLIFIER_SEED, election_account.key().as_ref(), nullifier_hash.as_ref()],
        bump,
    )]
    pub nullifier_account: Account<'info, NullifierAccount>,
    
    // system_program, arcium_program и аккаунты Arcium
    // добавляются макросом
    // pub system_program: Program<'info, System>,
    // pub arcium_program: Program<'info, Arcium>,
}

#[callback_accounts("vote")]
#[derive(Accounts)]
pub struct VoteCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_VOTE))]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(address = ::anchor_lang::solana_program::sysvar::instructions::ID)]
    /// CHECK: instructions_sysvar
    pub instructions_sysvar: AccountInfo<'info>,
    
    /// CHECK: election_account, верифицируется Arcium
    #[account(mut)]
    pub election_account: Account<'info, Election>,
}

// --- Контекст: Раскрытие Результатов (Arcium) ---

#[queue_computation_accounts("reveal_result", authority)]
#[derive(Accounts)]
#[instruction(computation_offset: u64,id: u32)]
pub struct RevealResult<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
        //pub sign_pda_account: Account<'info, SignerAccount>,
        #[account(
        init_if_needed, // 👈 Обязательно init
        payer = authority,
        space = 9, 
        seeds = [&ELECTION_SIGN_PDA_SEED],
        bump, // 👈 Обязательно bump
        address = derive_sign_pda!(),
    )]
    pub sign_pda_account: Account<'info, SignerAccount>,
    #[account(
        address = derive_mxe_pda!()
    )]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(
        mut,
        address = derive_mempool_pda!()
    )]
    /// CHECK: mempool_account, checked by the arcium program
    pub mempool_account: UncheckedAccount<'info>,
    #[account(
        mut,
        address = derive_execpool_pda!()
    )]
    /// CHECK: executing_pool, checked by the arcium program
    pub executing_pool: UncheckedAccount<'info>,
    #[account(
        mut,
        address = derive_comp_pda!(computation_offset)
    )]
    /// CHECK: computation_account, checked by the arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(
        address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_VOTE_STATS)
    )]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(
        mut,
        address = derive_cluster_pda!(mxe_account)
    )]
    pub cluster_account: Account<'info, Cluster>,
    #[account(
        mut,
        address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS,
    )]
    pub pool_account: Account<'info, FeePool>,
    #[account(
        address = ARCIUM_CLOCK_ACCOUNT_ADDRESS,
    )]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    // #[account(
    //     init,
    //     payer = authority,
    //     space = 8 + Election::INIT_SPACE, // Убедитесь, что INIT_SPACE в state.rs верный
    //     seeds = [ELECTION_SEED, authority.key().as_ref(), election_id.to_le_bytes().as_ref()],
    //     bump
    // )]
    // pub election_account: Account<'info, Election>,
    #[account(
        address = election_account.creator,
    )]
    pub creator: UncheckedAccount<'info>,
    #[account(
        mut,
        has_one = creator // Только создатель может раскрыть результаты
    )]
    pub election_account: Account<'info, Election>,
    //     #[account(mut,
    //     seeds = [b"election", authority.key().as_ref(), id.to_le_bytes().as_ref()],
    //     bump = election_account.bump,
    //     has_one = creator
    // )]
    // pub election_account: Account<'info, Election>,

    // system_program, arcium_program и аккаунты Arcium
    // добавляются макросом
    // pub system_program: Program<'info, System>,
    // pub arcium_program: Program<'info, Arcium>,
}

#[callback_accounts("reveal_result")]
#[derive(Accounts)]
pub struct RevealResultCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_REVEAL))]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(address = ::anchor_lang::solana_program::sysvar::instructions::ID)]
    /// CHECK: instructions_sysvar
    pub instructions_sysvar: AccountInfo<'info>,

    /// CHECK: election_account, верифицируется Arcium
    #[account(mut)]
    pub election_account: Account<'info, Election>,
}


#[error_code]
pub enum ErrorCode {
    #[msg("Invalid authority")]
    InvalidAuthority,
    #[msg("The computation was aborted")]
    AbortedComputation,
    #[msg("Cluster not set")]    
    ClusterNotSet,
    #[msg("Constraint raw data conversion failed")]
    ConstraintRaw,
}

#[event]
pub struct VoteEvent {
    pub timestamp: i64,
}

#[event]
pub struct RevealResultEvent {
    pub output: bool,
}