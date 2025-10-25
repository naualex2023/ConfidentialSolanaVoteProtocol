use arcis_imports::*;
use borsh::{BorshDeserialize, BorshSerialize};

// Импортируем константу из on-chain кода (нужно настроить пути в Cargo.toml)
const MAX_CANDIDATES: usize = 5; 

#[encrypted]
mod circuits {
    use arcis_imports::*;
    use borsh::{BorshDeserialize, BorshSerialize};
    use super::MAX_CANDIDATES; 

    /// Отслеживает зашифрованный счет голосов для MAX_CANDIDATES.
    #[derive(BorshSerialize, BorshDeserialize)]
    pub struct VoteStats {
        candidate_counts: [u64; MAX_CANDIDATES], 
    }

    /// Представляет сложный голос: индекс выбранного кандидата.
    #[derive(BorshSerialize, BorshDeserialize)]
    pub struct UserVote {
        candidate_index: u64, // Индекс кандидата (0, 1, ..., N-1)
    }

    /// Инициализирует зашифрованный счет нулями.
    #[instruction]
    pub fn init_vote_stats(mxe: Mxe) -> Enc<Mxe, VoteStats> {
        let vote_stats = VoteStats { 
            candidate_counts: [0; MAX_CANDIDATES] 
        };
        mxe.from_arcis(vote_stats)
    }

    /// Обрабатывает зашифрованный голос и обновляет текущее состояние.
    #[instruction]
    pub fn vote(
        vote_ctxt: Enc<Shared, UserVote>,
        vote_stats_ctxt: Enc<Mxe, VoteStats>,
    ) -> Enc<Mxe, VoteStats> {
        let user_vote = vote_ctxt.to_arcis();
        let mut vote_stats = vote_stats_ctxt.to_arcis();
        
        let index = user_vote.candidate_index as usize;

        // Конфиденциально инкрементируем счетчик по индексу.
        // Проверка index < MAX_CANDIDATES внутри MPC гарантирует, что 
        // некорректный голос не сломает счетчик, но и не будет учтен.
        if index < MAX_CANDIDATES {
            vote_stats.candidate_counts[index] += 1;
        }

        vote_stats_ctxt.owner.from_arcis(vote_stats)
    }

    /// Раскрывает финальный результат: дешифрует все N счетчиков.
    #[instruction]
    pub fn reveal_result(
        vote_stats_ctxt: Enc<Mxe, VoteStats>,
    ) -> [Public<u64>; MAX_CANDIDATES] {
        let vote_stats = vote_stats_ctxt.to_arcis();
        
        let mut public_results: [Public<u64>; MAX_CANDIDATES] = [Public::default(); MAX_CANDIDATES];
        
        for i in 0..MAX_CANDIDATES {
            public_results[i] = vote_stats.candidate_counts[i].to_public();
        }
        
        public_results
    }
}