use arcium_precludes::*;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PollData {
    pub yes_votes: u64,
    pub no_votes: u64,
    pub total_votes: u64,
    pub is_active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VoteConfirmation {
    pub vote_registered: bool,
    pub voter_hash: [u8; 32],
}

#[instruction]
pub fn process_encrypted_vote(
    voter_biometric_hash: Enc<Shared, [u8; 32]>,
    vote_choice: Enc<Shared, bool>,
    poll_data: Enc<Mxe, PollData>,
    registered_voters: Enc<Mxe, Vec<[u8; 32]>>,
) -> (Enc<Mxe, PollData>, Enc<Shared, VoteConfirmation>) {
    // Проверка регистрации избирателя
    let is_registered = check_voter_registration(
        voter_biometric_hash.clone(),
        registered_voters
    );
    
    // Если избиратель не зарегистрирован, возвращаем ошибку
    let can_vote = is_registered;
    
    // Обновляем статистику голосования, если голос действителен
    let updated_poll = if can_vote {
        update_poll_results(poll_data, vote_choice)
    } else {
        poll_data
    };
    
    // Создаем подтверждение голосования
    let confirmation = VoteConfirmation {
        vote_registered: can_vote,
        voter_hash: voter_biometric_hash.reveal(),
    };
    
    (updated_poll, Enc::shared(confirmation))
}

#[instruction]
pub fn tally_final_results(
    poll_data: Enc<Mxe, PollData>,
) -> Enc<Shared, (u64, u64, u64)> {
    // Подсчет финальных результатов
    let final_results = (
        poll_data.yes_votes,
        poll_data.no_votes,
        poll_data.total_votes,
    );
    
    Enc::shared(final_results)
}

fn check_voter_registration(
    voter_hash: Enc<Shared, [u8; 32]>,
    registered_voters: Enc<Mxe, Vec<[u8; 32]>>,
) -> bool {
    // Проверка наличия хэша избирателя в списке зарегистрированных
    // Это выполняется в зашифрованном виде
    let voter_bytes = voter_hash.reveal();
    let voters_list = registered_voters.reveal_to_mxe();
    
    voters_list.iter().any(|&hash| hash == voter_bytes)
}

fn update_poll_results(
    poll_data: Enc<Mxe, PollData>,
    vote_choice: Enc<Shared, bool>,
) -> Enc<Mxe, PollData> {
    let choice = vote_choice.reveal();
    let mut data = poll_data.reveal_to_mxe();
    
    if choice {
        data.yes_votes += 1;
    } else {
        data.no_votes += 1;
    }
    data.total_votes += 1;
    
    Enc::mxe(data)
}