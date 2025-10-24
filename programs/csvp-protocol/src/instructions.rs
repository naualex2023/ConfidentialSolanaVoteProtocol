use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    program_error::ProgramError,
    pubkey::Pubkey,
    clock::UnixTimestamp,
};

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub enum VoteInstruction {
    InitializeElection {
        election_id: u64,
        title: String,
        description: String,
        start_time: UnixTimestamp,
        end_time: UnixTimestamp,
        arcium_cluster_id: String,
        public_key: [u8; 32],
    },

    RegisterVoters {
        voter_hashes: Vec<[u8; 32]>,
        chunk_index: u32,
    },

    CastVote {
        voter_hash: [u8; 32],
        encrypted_vote: Vec<u8>,
        receipt_id: [u8; 32],
        nullifier: [u8; 32],
        arcium_proof: Vec<u8>,
    },

    StartTallying,
    CompleteElection,
}

impl VoteInstruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (tag, rest) = input.split_first().ok_or(ProgramError::InvalidInstructionData)?;
        
        Ok(match tag {
            0 => {
                let args = InitializeElectionArgs::try_from_slice(rest)?;
                Self::InitializeElection {
                    election_id: args.election_id,
                    title: args.title,
                    description: args.description,
                    start_time: args.start_time,
                    end_time: args.end_time,
                    arcium_cluster_id: args.arcium_cluster_id,
                    public_key: args.public_key,
                }
            }
            1 => {
                let args = RegisterVotersArgs::try_from_slice(rest)?;
                Self::RegisterVoters {
                    voter_hashes: args.voter_hashes,
                    chunk_index: args.chunk_index,
                }
            }
            2 => {
                let args = CastVoteArgs::try_from_slice(rest)?;
                Self::CastVote {
                    voter_hash: args.voter_hash,
                    encrypted_vote: args.encrypted_vote,
                    receipt_id: args.receipt_id,
                    nullifier: args.nullifier,
                    arcium_proof: args.arcium_proof,
                }
            }
            3 => Self::StartTallying,
            4 => Self::CompleteElection,
            _ => return Err(ProgramError::InvalidInstructionData),
        })
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
struct InitializeElectionArgs {
    election_id: u64,
    title: String,
    description: String,
    start_time: UnixTimestamp,
    end_time: UnixTimestamp,
    arcium_cluster_id: String,
    public_key: [u8; 32],
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
struct RegisterVotersArgs {
    voter_hashes: Vec<[u8; 32]>,
    chunk_index: u32,
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
struct CastVoteArgs {
    voter_hash: [u8; 32],
    encrypted_vote: Vec<u8>,
    receipt_id: [u8; 32],
    nullifier: [u8; 32],
    arcium_proof: Vec<u8>,
}