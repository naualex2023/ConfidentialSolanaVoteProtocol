use thiserror::Error;
use solana_program::program_error::ProgramError;

#[derive(Error, Debug, Copy, Clone)]
pub enum VoteError {
    #[error("Invalid Instruction")]
    InvalidInstruction,
    
    #[error("Already Voted")]
    AlreadyVoted,
    
    #[error("Not Authorized")]
    NotAuthorized,
    
    #[error("Election Not Active")]
    ElectionNotActive,
    
    #[error("Invalid Election Period")]
    InvalidElectionPeriod,
    
    #[error("Voter Not Registered")]
    VoterNotRegistered,
    
    #[error("Chunk Full")]
    ChunkFull,
}

impl From<VoteError> for ProgramError {
    fn from(e: VoteError) -> Self {
        ProgramError::Custom(e as u32)
    }
}