use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;
//use arcium_client::idl::arcium::types::CallbackAccount;

pub mod state;
pub mod error;

use crate::state::*; 
use crate::error::VoteError; 

// Offsets для MPC схем (должны совпадать с circuit/lib.rs)
const COMP_DEF_OFFSET_INIT_VOTE_STATS: u32 = comp_def_offset("init_vote_stats");
const COMP_DEF_OFFSET_VOTE: u32 = comp_def_offset("vote");
const COMP_DEF_OFFSET_REVEAL: u32 = comp_def_offset("reveal_result");

declare_id!("G9sN2L8tD1G9K2J3A8tW4T1D7C3F2B1E6H9g5P4Q2R3"); // Ваш Program ID

#[arcium_program]
pub mod confidential_voting {
    use super::*;

    // ----------------------
    // 1. Управление реестром
    // ----------------------

    /// Регистрирует группу голосующих в чанке.
    pub fn register_voters(ctx: Context<RegisterVoters>, chunk_index: u32, voter_hashes: Vec<[u8; 32]>) -> Result<()> {
        let chunk = &mut ctx.accounts.voter_chunk;
        let election = &ctx.accounts.election;
        
        if election.creator != *ctx.accounts.authority.key {
            return err!(VoteError::NotAuthorized);
        }

        if chunk.voter_hashes.len() + voter_hashes.len() > MAX_ITEMS_PER_CHUNK {
            return err!(VoteError::ChunkFull);
        }

        chunk.election = election.key();
        chunk.chunk_index = chunk_index;
        chunk.voter_hashes.extend(voter_hashes);

        Ok(())
    }

    // ----------------------
    // 2. Инициализация выборов
    // ----------------------

    /// Создает выборы и запускает MPC для инициализации encrypted_tally нулями.
    pub fn initialize_election(
        ctx: Context<InitializeElection>, 
        election_id: u64,
        title: String,
        start_time: UnixTimestamp,
        end_time: UnixTimestamp,
        nonce: u128,
    ) -> Result<()> {
        let election = &mut ctx.accounts.election_account;
        
        election.creator = *ctx.accounts.authority.key;
        election.election_id = election_id;
        election.title = title;
        election.start_time = start_time;
        election.end_time = end_time;
        election.state = ElectionState::Draft; // Начинаем с Draft
        election.total_votes = 0;
        election.nonce = nonce;
        
        // MPC инициализация: запускаем схему, которая вернет зашифрованный массив нулей
        let arcium_accounts = ArciumQueueComputationAccounts {
            payer: ctx.accounts.authority.to_account_info(),
            mxe_account: ctx.accounts.mxe_account.to_account_info(),
            comp_def_account: ctx.accounts.init_comp_def_account.to_account_info(),
            computation_account: ctx.accounts.computation_account.to_account_info(),
            callback_account: ctx.accounts.init_callback.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            arcium_program: ctx.accounts.arcium_program.to_account_info(),
        };

        queue_computation(
            arcium_accounts,
            COMP_DEF_OFFSET_INIT_VOTE_STATS,
            ctx.bumps.get("computation_account").unwrap(),
            ctx.bumps.get("init_callback").unwrap(),
            &[], // Нет входных данных для инициализации
        )?;
        
        election.state = ElectionState::Active; // Переводим в Active после запуска MPC
        Ok(())
    }

    /// Коллбэк после инициализации: записывает зашифрованный агрегат (нули)
    pub fn init_callback(ctx: Context<InitCallback>, _output: ComputationOutputs) -> Result<()> {
        let election = &mut ctx.accounts.election_account;
        
        let mxe_output = _output.get_mxe_output(0)?; // MAX_CANDIDATES * 32 байта
        
        // Записываем нули в encrypted_tally
        for i in 0..MAX_CANDIDATES {
            let start = i * 32;
            election.encrypted_tally[i].copy_from_slice(&mxe_output[start..start + 32]);
        }
        
        Ok(())
    }

    // ----------------------
    // 3. Голосование
    // ----------------------

    /// Голосование: проверяет регистрацию, Nullifier и запускает MPC.
    pub fn cast_vote(
        ctx: Context<CastVote>,
        encrypted_candidate_index: [u8; 32], // Enc<Shared, UserVote>
        nullifier_hash: [u8; 32], 
        voter_hash: [u8; 32],
    ) -> Result<()> {
        let election = &mut ctx.accounts.election_account;
        let clock = Clock::get()?;
        
        // 1. ПРОВЕРКА РЕГИСТРАЦИИ (Voter Chunk)
        if !ctx.accounts.voter_chunk.voter_hashes.contains(&voter_hash) {
            return err!(VoteError::VoterNotRegistered);
        }

        // 2. ПРОВЕРКА ДВОЙНОГО ГОЛОСОВАНИЯ (Nullifier Account)
        // init гарантирует, что если PDA уже существует (т.е. избиратель проголосовал), 
        // транзакция завершится с ошибкой ProgramError::AccountAlreadyInitialized.
        let nullifier = &mut ctx.accounts.nullifier_account;
        nullifier.election_pda = election.key();
        nullifier.nullifier_hash = nullifier_hash;

        // 3. ПРОВЕРКА ПЕРИОДА ВЫБОРОВ
        if election.state != ElectionState::Active 
            || clock.unix_timestamp < election.start_time 
            || clock.unix_timestamp > election.end_time 
        {
            return err!(VoteError::InvalidElectionPeriod);
        }

        // 4. ЗАПУСК MPC для агрегации голоса
        let arcium_accounts = ArciumQueueComputationAccounts {
            payer: ctx.accounts.voter.to_account_info(),
            mxe_account: ctx.accounts.mxe_account.to_account_info(),
            comp_def_account: ctx.accounts.vote_comp_def_account.to_account_info(),
            computation_account: ctx.accounts.computation_account.to_account_info(),
            callback_account: ctx.accounts.vote_callback.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            arcium_program: ctx.accounts.arcium_program.to_account_info(),
        };

        // Входные данные: Encrypted Vote + Current Tally (N*32 bytes) + Nonce
        let mut inputs: Vec<u8> = vec![];
        inputs.extend_from_slice(&encrypted_candidate_index); 
        
        for i in 0..MAX_CANDIDATES {
            inputs.extend_from_slice(&election.encrypted_tally[i]);
        }
        
        inputs.extend_from_slice(&election.nonce.to_le_bytes()); 

        queue_computation(
            arcium_accounts,
            COMP_DEF_OFFSET_VOTE,
            ctx.bumps.get("computation_account").unwrap(),
            ctx.bumps.get("vote_callback").unwrap(),
            &inputs,
        )?;
        
        election.total_votes += 1;
        
        Ok(())
    }

    /// Коллбэк: записывает новый агрегат (N*32 байта)
    pub fn vote_callback(ctx: Context<VoteCallback>, _output: ComputationOutputs) -> Result<()> {
        let election = &mut ctx.accounts.election_account;

        let mxe_output = _output.get_mxe_output(0)?; 
        
        // Обновляем все N счетчиков
        for i in 0..MAX_CANDIDATES {
            let start = i * 32;
            election.encrypted_tally[i].copy_from_slice(&mxe_output[start..start + 32]);
        }
        
        Ok(())
    }
    
    // ----------------------
    // 4. Раскрытие результата
    // ----------------------

    // ... (reveal_result - запускает MPC COMP_DEF_OFFSET_REVEAL)

    /// Коллбэк: записывает финальный расшифрованный результат (N*8 байт)
    pub fn reveal_callback(ctx: Context<RevealCallback>, _output: ComputationOutputs) -> Result<()> {
        let election = &mut ctx.accounts.election_account;
        
        let public_output = _output.get_public_output()?; 

        // Десериализуем N u64 счетчиков
        for i in 0..MAX_CANDIDATES {
            let start = i * 8;
            election.final_result[i] = u64::from_le_bytes(public_output[start..start + 8].try_into().unwrap());
        }

        election.state = ElectionState::Completed;

        Ok(())
    }
}

// ------------------------------------------------------------------
// Структуры Контекстов (Accounts Structs)
// ------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(chunk_index: u32, voter_hashes: Vec<[u8; 32]>)]
pub struct RegisterVoters<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    #[account(mut)]
    pub election: Account<'info, Election>,
    
    #[account(
        init_if_needed,
        payer = authority,
        space = VoterChunk::MAX_SPACE, 
        seeds = [VOTER_CHUNK_SEED, election.key().as_ref(), chunk_index.to_le_bytes().as_ref()],
        bump
    )]
    pub voter_chunk: Account<'info, VoterChunk>,
    
    pub system_program: Program<'info, System>,
}


// Контекст для голосования
#[derive(Accounts)]
#[instruction(encrypted_candidate_index: [u8; 32], nullifier_hash: [u8; 32], voter_hash: [u8; 32])]
pub struct CastVote<'info> {
    #[account(mut)]
    pub voter: Signer<'info>,
    
    #[account(mut)]
    pub election_account: Account<'info, Election>,
    
    // Аккаунт VoterChunk (для проверки регистрации)
    /// CHECK: Must be the correct voter chunk
    pub voter_chunk: Account<'info, VoterChunk>, 
    
    // Nullifier (init) - предотвращение двойного голосования
    #[account(
        init, // init гарантирует fail, если аккаунт существует
        payer = voter,
        space = 8 + NullifierAccount::INIT_SPACE,
        seeds = [NULLIFIER_SEED, election_account.key().as_ref(), nullifier_hash.as_ref()],
        bump,
    )]
    pub nullifier_account: Account<'info, NullifierAccount>,
    
    // Аккаунты Arcium (для queue_computation)
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    /// CHECK: Comp Def for vote
    pub vote_comp_def_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Computation account
    pub computation_account: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Callback for vote update
    pub vote_callback: UncheckedAccount<'info>,
    
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

// Контекст для инициализации (пример)
#[derive(Accounts)]
pub struct InitializeElection<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    
    #[account(
        init,
        payer = authority,
        space = 8 + Election::INIT_SPACE,
        seeds = [ELECTION_SEED, authority.key().as_ref(), &ctx.bumps.election_account.to_le_bytes()],
        bump
    )]
    pub election_account: Account<'info, Election>,
    
    // Аккаунты Arcium
    // ...
}

#[derive(Accounts)]
pub struct InitCallback<'info> {
    #[account(mut)]
    pub election_account: Account<'info, Election>,
    #[account(mut)]
    /// CHECK: Callback account
    pub callback_account: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct VoteCallback<'info> {
    #[account(mut)]
    pub election_account: Account<'info, Election>,
    #[account(mut)]
    /// CHECK: Callback account
    pub callback_account: UncheckedAccount<'info>,
}