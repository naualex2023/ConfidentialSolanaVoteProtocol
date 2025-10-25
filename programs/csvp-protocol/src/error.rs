use anchor_lang::prelude::*;

#[error_code]
pub enum VoteError {
    #[msg("Voter is not registered in the Voter Chunk.")]
    VoterNotRegistered,
    #[msg("The voter has already cast a vote (Nullifier already exists).")]
    AlreadyVoted,
    #[msg("Not authorized to perform this action.")]
    NotAuthorized,
    #[msg("Chunk Full, cannot register more voters in this chunk.")]
    ChunkFull,
    #[msg("Invalid Election Period.")]
    InvalidElectionPeriod,
    #[msg("Invalid Candidate Index. Must be less than MAX_CANDIDATES.")]
    InvalidCandidateIndex,
}