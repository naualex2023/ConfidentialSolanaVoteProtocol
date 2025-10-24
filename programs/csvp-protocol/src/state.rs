use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    program_pack::{IsInitialized, Sealed},
    pubkey::Pubkey,
    clock::UnixTimestamp,
};

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub enum ElectionState {
    Draft,
    Active,
    Tallying,
    Completed,
    Cancelled,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct Election {
    pub discriminator: u8,
    pub is_initialized: bool,
    pub creator: Pubkey,
    pub election_id: u64,
    pub title: String,
    pub description: String,
    pub start_time: UnixTimestamp,
    pub end_time: UnixTimestamp,
    pub state: ElectionState,
    pub arcium_cluster_id: String,
    pub public_key: [u8; 32],
    pub total_votes: u32,
    pub bump: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct VoterChunk {
    pub discriminator: u8,
    pub is_initialized: bool,
    pub election: Pubkey,
    pub chunk_index: u32,
    pub next_chunk: Option<Pubkey>,
    pub voter_hashes: Vec<[u8; 32]>,
    pub bump: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct ReceiptChunk {
    pub discriminator: u8,
    pub is_initialized: bool,
    pub election: Pubkey,
    pub chunk_index: u32,
    pub next_chunk: Option<Pubkey>,
    pub receipt_ids: Vec<[u8; 32]>,
    pub nullifiers: Vec<[u8; 32]>,
    pub bump: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct BallotChunk {
    pub discriminator: u8,
    pub is_initialized: bool,
    pub election: Pubkey,
    pub chunk_index: u32,
    pub next_chunk: Option<Pubkey>,
    pub ballots: Vec<EncryptedBallot>,
    pub bump: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct EncryptedBallot {
    pub encrypted_data: Vec<u8>,
    pub receipt_id: [u8; 32],
    pub timestamp: UnixTimestamp,
    pub arcium_proof: Vec<u8>,
}

// Константы
pub const MAX_ITEMS_PER_CHUNK: usize = 500;
pub const ELECTION_SEED: &[u8] = b"election";
pub const VOTER_CHUNK_SEED: &[u8] = b"voter_chunk";
pub const RECEIPT_CHUNK_SEED: &[u8] = b"receipt_chunk";
pub const BALLOT_CHUNK_SEED: &[u8] = b"ballot_chunk";

// Дискриминаторы
pub const ELECTION_DISCRIMINATOR: u8 = 0;
pub const VOTER_CHUNK_DISCRIMINATOR: u8 = 1;
pub const RECEIPT_CHUNK_DISCRIMINATOR: u8 = 2;
pub const BALLOT_CHUNK_DISCRIMINATOR: u8 = 3;

impl Sealed for Election {}
impl Sealed for VoterChunk {}
impl Sealed for ReceiptChunk {}
impl Sealed for BallotChunk {}

impl IsInitialized for Election {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl IsInitialized for VoterChunk {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl IsInitialized for ReceiptChunk {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl IsInitialized for BallotChunk {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}