use anchor_lang::prelude::*;

// ... (структуры и импорты)

// 2. СТАНДАРТНЫЙ МОДУЛЬ (для register_voter)
#[program] // <-- ДОБАВИТЬ ВТОРОЙ МАКРОС
pub mod registration { // <-- НОВЫЙ МОДУЛЬ
    use super::*;

      // ----------------------
    // 2. Управление реестром
    // ----------------------

    /// Регистрирует группу голосующих в чанке.

    pub fn register_voters(
        ctx: Context<RegisterVoters>,
        _chunk_index: u32, // index уже находится в PDA, сохраняем для ясности
        voter_hashes: Vec<Pubkey>,
    ) -> Result<()> {
        // Меняем имя переменной с 'chunk' на 'registry' для ясности
        msg!("Registering {} voters in chunk {}", voter_hashes.len(), _chunk_index);
        let registry = &mut ctx.accounts.voter_registry; 
        let election = &ctx.accounts.election;

        // --- 1. Проверки безопасности ---
        msg!("Performing security checks...");
        // 1.1. Только создатель выборов может добавлять избирателей
        require_keys_eq!(
            election.creator,
            ctx.accounts.authority.key(),
            VoteError::NotAuthorized
        );
        msg!("Authority is authorized.");
        // 1.2. Выборы еще не начались
        require!(
            election.state == 0, // 0 - Draft
            VoteError::ElectionNotDraft
        );
        msg!("Election is in Draft state.");
        // --- 2. Проверка на переполнение чанка (до добавления) ---
        let total_new_hashes = voter_hashes.len();
        let current_count = registry.count as usize;

        require!(
            current_count.checked_add(total_new_hashes).is_some() && 
            current_count + total_new_hashes <= MAX_ITEMS_PER_CHUNK,
            VoteError::ChunkFull
        );
        msg!("Chunk has enough space for new voters.");
        // --- 3. Инициализация и заполнение данных (БЕЗ REALLOC) ---

        // Устанавливаем эти поля только при первой инициализации аккаунта
        registry.election = election.key(); 
        registry.chunk_index = _chunk_index; 
        msg!("Filling voter hashes into the registry...");
        // ЗАМЕНА: Используем цикл с прямой записью в массив вместо .extend()
        for (i, hash) in voter_hashes.into_iter().enumerate() {
            let index_to_write = current_count
                .checked_add(i)
                .ok_or(VoteError::ChunkFull)?; // Проверка переполнения
            msg!("Writing hash at index {}", index_to_write);
            // ПРЯМАЯ ЗАПИСЬ В ФИКСИРОВАННЫЙ МАССИВ. Это устраняет realloc.
            registry.voter_hashes[index_to_write] = hash;
            
            // Инкрементируем счетчик
            registry.count = registry.count.checked_add(1).ok_or(VoteError::ChunkFull)?;
        }
        
        // 4. Сохранение bump (только при init_if_needed)
        registry.bump = ctx.bumps.voter_registry;

        Ok(())
    }

      /// Инструкция для регистрации одного избирателя.
    /// ПРИМЕЧАНИЕ: Хэш избирателя передается как Pubkey (32 байта, Base58), 
    /// что позволяет Anchor автоматически его декодировать.
    pub fn register_voter(
        ctx: Context<RegisterVoters>,
        _chunk_index: u32, // index уже находится в PDA, сохраняем для ясности
        voter_hash: Pubkey, // ИЗМЕНЕНИЕ: Теперь принимаем Pubkey
    ) -> Result<()> {
        let registry = &mut ctx.accounts.voter_registry; 
        let election = &ctx.accounts.election;

        // --- 1. Проверки безопасности ---
        msg!("Performing security checks...");
        
        // 1.1. Только создатель выборов может добавлять избирателей
        require_keys_eq!(
            election.creator,
            ctx.accounts.authority.key(),
            VoteError::NotAuthorized
        );
        
        // 1.2. Выборы еще не начались
        require!(
            election.state == 0, // 0 - Draft
            VoteError::ElectionNotDraft
        );
        msg!("Checks passed. Current count: {}", registry.count);


        // 1.3. Проверка на переполнение чанка 
        let current_count = registry.count as usize;
        require!(
            current_count < MAX_ITEMS_PER_CHUNK,
            VoteError::ChunkFull
        );
        
        // --- 2. Декодирование Base58 в [u8; 32] ---
        // ЭТОТ ШАГ УДАЛЕН, ТАК КАК Pubkey УЖЕ ДЕКОДИРОВАЛ ДАННЫЕ.
        // Мы используем voter_hash напрямую, так как он уже является 32-байтовым объектом.
        
        // --- 3. Запись данных (БЕЗ REALLOC) ---

        // Устанавливаем эти поля только при первой инициализации аккаунта
        if registry.count == 0 {
            registry.election = election.key(); 
            registry.chunk_index = _chunk_index; 
        }

        // Прямая запись в фиксированный массив по текущему счетчику
        let index_to_write = current_count;
        
        // ПРИМЕЧАНИЕ: Здесь мы предполагаем, что в state.rs вы изменили 
        // VoterRegistry::voter_hashes на массив Pubkey.
        registry.voter_hashes[index_to_write] = voter_hash; 
        
        // Инкрементируем счетчик
        registry.count = registry.count.checked_add(1).ok_or(VoteError::ChunkFull)?;

        // Сохранение bump
        registry.bump = ctx.bumps.voter_registry;
        msg!("Voter hash recorded at index {}. New count: {}", index_to_write, registry.count);

        Ok(())
    }
}
// ...
// #[derive(Accounts)] pub struct RegisterVoters { ... } // (Нужно вынести сюда, или импортировать)
// ...