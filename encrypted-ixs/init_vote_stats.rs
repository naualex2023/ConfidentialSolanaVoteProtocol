use arcis_imports::*;

#[instruction]
pub fn init_vote_stats(mxe: Mxe) -> Enc<Mxe, VoteStats> {
    let vote_stats = VoteStats {
        candidate_counts: [0; MAX_CANDIDATES],
    };
    
    mxe.from_arcis(vote_stats)
}