use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{clock::Clock, Sysvar},
    msg,
    program::invoke_signed,
    system_instruction,
    rent::Rent,
    sysvar::Sysvar as SolanaSysvar,
};

use borsh::BorshSerialize;

use crate::{
    error::VoteError,
    instruction::VoteInstruction,
    state::{
        Election, VoterChunk, ReceiptChunk, BallotChunk, EncryptedBallot, ElectionState,
        ELECTION_SEED, VOTER_CHUNK_SEED, RECEIPT_CHUNK_SEED, BALLOT_CHUNK_SEED,
        ELECTION_DISCRIMINATOR, VOTER_CHUNK_DISCRIMINATOR, 
        RECEIPT_CHUNK_DISCRIMINATOR, BALLOT_CHUNK_DISCRIMINATOR,
        MAX_ITEMS_PER_CHUNK,
    },
};

pub struct Processor;

impl Processor {
    pub fn process_instruction(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        let instruction = VoteInstruction::unpack(instruction_data)?;

        match instruction {
            VoteInstruction::InitializeElection {
                election_id,
                title,
                description,
                start_time,
                end_time,
                arcium_cluster_id,
                public_key,
            } => {
                Self::process_initialize_election(
                    program_id,
                    accounts,
                    election_id,
                    title,
                    description,
                    start_time,
                    end_time,
                    arcium_cluster_id,
                    public_key,
                )
            }
            VoteInstruction::RegisterVoters { voter_hashes, chunk_index } => {
                Self::process_register_voters(
                    program_id,
                    accounts,
                    voter_hashes,
                    chunk_index,
                )
            }
            VoteInstruction::CastVote { 
                voter_hash, 
                encrypted_vote, 
                receipt_id, 
                nullifier, 
                arcium_proof 
            } => {
                Self::process_cast_vote(
                    program_id,
                    accounts,
                    voter_hash,
                    encrypted_vote,
                    receipt_id,
                    nullifier,
                    arcium_proof,
                )
            }
            VoteInstruction::StartTallying => {
                Self::process_start_tallying(program_id, accounts)
            }
            VoteInstruction::CompleteElection => {
                Self::process_complete_election(program_id, accounts)
            }
        }
    }

    fn process_initialize_election(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        election_id: u64,
        title: String,
        description: String,
        start_time: i64,
        end_time: i64,
        arcium_cluster_id: String,
        public_key: [u8; 32],
    ) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        
        let creator = next_account_info(accounts_iter)?;
        let election_account = next_account_info(accounts_iter)?;
        let system_program = next_account_info(accounts_iter)?;

        if !creator.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let clock = Clock::get()?;
        if start_time < clock.unix_timestamp || end_time <= start_time {
            return Err(VoteError::InvalidElectionPeriod.into());
        }

        // Создаем PDA для выборов
        let (election_pda, election_bump) = Pubkey::find_program_address(
            &[ELECTION_SEED, &election_id.to_le_bytes(), creator.key.as_ref()],
            program_id,
        );

        if election_pda != *election_account.key {
            return Err(ProgramError::InvalidArgument);
        }

        // Инициализируем аккаунт выборов
        let election = Election {
            discriminator: ELECTION_DISCRIMINATOR,
            is_initialized: true,
            creator: *creator.key,
            election_id,
            title,
            description,
            start_time,
            end_time,
            state: ElectionState::Draft,
            arcium_cluster_id,
            public_key,
            total_votes: 0,
            bump: election_bump,
        };

        election.serialize(&mut &mut election_account.data.borrow_mut()[..])?;

        msg!("Election initialized with Arcium cluster");
        Ok(())
    }

    fn process_register_voters(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        voter_hashes: Vec<[u8; 32]>,
        chunk_index: u32,
    ) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        
        let creator = next_account_info(accounts_iter)?;
        let election_account = next_account_info(accounts_iter)?;
        let voter_chunk_account = next_account_info(accounts_iter)?;
        let system_program = next_account_info(accounts_iter)?;

        if !creator.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let election = Election::try_from_slice(&election_account.data.borrow())?;
        
        if election.creator != *creator.key {
            return Err(VoteError::NotAuthorized.into());
        }

        // Создаем или обновляем чанк избирателей
        let mut voter_chunk = if voter_chunk_account.data_is_empty() {
            VoterChunk {
                discriminator: VOTER_CHUNK_DISCRIMINATOR,
                is_initialized: true,
                election: *election_account.key,
                chunk_index,
                next_chunk: None,
                voter_hashes: Vec::new(),
                bump: 0,
            }
        } else {
            VoterChunk::try_from_slice(&voter_chunk_account.data.borrow())?
        };

        // Проверяем, что чанк не переполнен
        if voter_chunk.voter_hashes.len() + voter_hashes.len() > MAX_ITEMS_PER_CHUNK {
            return Err(VoteError::ChunkFull.into());
        }

        // Добавляем избирателей
        voter_chunk.voter_hashes.extend(voter_hashes);
        voter_chunk.serialize(&mut &mut voter_chunk_account.data.borrow_mut()[..])?;

        msg!("Registered {} voters in chunk {}", voter_hashes.len(), chunk_index);
        Ok(())
    }

    fn process_cast_vote(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        voter_hash: [u8; 32],
        encrypted_vote: Vec<u8>,
        receipt_id: [u8; 32],
        nullifier: [u8; 32],
        arcium_proof: Vec<u8>,
    ) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        
        let voter = next_account_info(accounts_iter)?;
        let election_account = next_account_info(accounts_iter)?;
        let voter_chunk_account = next_account_info(accounts_iter)?;
        let receipt_chunk_account = next_account_info(accounts_iter)?;
        let ballot_chunk_account = next_account_info(accounts_iter)?;

        if !voter.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mut election = Election::try_from_slice(&election_account.data.borrow())?;
        let clock = Clock::get()?;

        if election.state != ElectionState::Active {
            return Err(VoteError::ElectionNotActive.into());
        }

        if clock.unix_timestamp < election.start_time || clock.unix_timestamp > election.end_time {
            return Err(VoteError::InvalidElectionPeriod.into());
        }

        // Проверяем, что избиратель зарегистрирован (ищем по всем чанкам)
        if !Self::is_voter_registered(program_id, election_account.key, voter_hash)? {
            return Err(VoteError::VoterNotRegistered.into());
        }

        // Проверяем, что nullifier не использовался (ищем по всем чанкам)
        if Self::has_voter_voted(program_id, election_account.key, nullifier)? {
            return Err(VoteError::AlreadyVoted.into());
        }

        // Обновляем receipt чанк
        let mut receipt_chunk = ReceiptChunk::try_from_slice(&receipt_chunk_account.data.borrow())?;
        receipt_chunk.receipt_ids.push(receipt_id);
        receipt_chunk.nullifiers.push(nullifier);
        receipt_chunk.serialize(&mut &mut receipt_chunk_account.data.borrow_mut()[..])?;

        // Обновляем ballot чанк
        let mut ballot_chunk = BallotChunk::try_from_slice(&ballot_chunk_account.data.borrow())?;
        let ballot = EncryptedBallot {
            encrypted_data: encrypted_vote,
            receipt_id,
            timestamp: clock.unix_timestamp,
            arcium_proof,
        };
        ballot_chunk.ballots.push(ballot);
        ballot_chunk.serialize(&mut &mut ballot_chunk_account.data.borrow_mut()[..])?;

        // Обновляем election
        election.total_votes += 1;
        election.serialize(&mut &mut election_account.data.borrow_mut()[..])?;

        msg!("Vote cast successfully");
        Ok(())
    }

    fn process_start_tallying(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        
        let creator = next_account_info(accounts_iter)?;
        let election_account = next_account_info(accounts_iter)?;

        if !creator.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mut election = Election::try_from_slice(&election_account.data.borrow())?;
        
        if election.creator != *creator.key {
            return Err(VoteError::NotAuthorized.into());
        }

        election.state = ElectionState::Tallying;
        election.serialize(&mut &mut election_account.data.borrow_mut()[..])?;

        msg!("Election tallying started");
        Ok(())
    }

    fn process_complete_election(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        
        let creator = next_account_info(accounts_iter)?;
        let election_account = next_account_info(accounts_iter)?;

        if !creator.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mut election = Election::try_from_slice(&election_account.data.borrow())?;
        
        if election.creator != *creator.key {
            return Err(VoteError::NotAuthorized.into());
        }

        election.state = ElectionState::Completed;
        election.serialize(&mut &mut election_account.data.borrow_mut()[..])?;

        msg!("Election completed with {} votes", election.total_votes);
        Ok(())
    }

    // Поиск избирателя по всем чанкам
    fn is_voter_registered(
        program_id: &Pubkey,
        election_pda: &Pubkey,
        voter_hash: [u8; 32],
    ) -> Result<bool, ProgramError> {
        let mut chunk_index = 0;
        
        loop {
            let (chunk_pda, _) = Pubkey::find_program_address(
                &[VOTER_CHUNK_SEED, election_pda.as_ref(), &chunk_index.to_le_bytes()],
                program_id,
            );

            // Пытаемся получить аккаунт
            // В реальной реализации нужно использовать cross-program invocation
            // Для демо возвращаем true если нашли в первом чанке
            let chunk_account = match crate::entrypoint::get_account(&chunk_pda) {
                Ok(account) => account,
                Err(_) => break, // Чанк не существует
            };

            let chunk = VoterChunk::try_from_slice(&chunk_account.data.borrow())?;
            if chunk.voter_hashes.contains(&voter_hash) {
                return Ok(true);
            }

            if chunk.next_chunk.is_none() {
                break;
            }
            
            chunk_index += 1;
        }

        Ok(false)
    }

    // Проверка nullifier по всем чанкам
    fn has_voter_voted(
        program_id: &Pubkey,
        election_pda: &Pubkey,
        nullifier: [u8; 32],
    ) -> Result<bool, ProgramError> {
        let mut chunk_index = 0;
        
        loop {
            let (chunk_pda, _) = Pubkey::find_program_address(
                &[RECEIPT_CHUNK_SEED, election_pda.as_ref(), &chunk_index.to_le_bytes()],
                program_id,
            );

            // Пытаемся получить аккаунт
            let chunk_account = match crate::entrypoint::get_account(&chunk_pda) {
                Ok(account) => account,
                Err(_) => break,
            };

            let chunk = ReceiptChunk::try_from_slice(&chunk_account.data.borrow())?;
            if chunk.nullifiers.contains(&nullifier) {
                return Ok(true);
            }

            if chunk.next_chunk.is_none() {
                break;
            }
            
            chunk_index += 1;
        }

        Ok(false)
    }
}