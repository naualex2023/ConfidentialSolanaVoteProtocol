import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Registration } from "../target/types/registration";

async function cleanupVoters() {
  // 1. Настройка
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Registration as Program<Registration>;
  const authority = provider.wallet;

  console.log("🔍 Fetching all voter accounts for authority:", authority.publicKey.toBase58());

  // 2. Получаем ВСЕ аккаунты VoterProof
  // (Anchor позволяет фильтровать по полям, если нужно, но здесь берем все и фильтруем в JS)
  const allVoters = await program.account.voterProof.all([
    {
      memcmp: {
        offset: 8 + 32, // Пропускаем Discriminator (8) + voter_hash (32), попадаем на authority
        bytes: authority.publicKey.toBase58(),
      },
    },
  ]);

  console.log(`found ${allVoters.length} accounts to close.`);

  if (allVoters.length === 0) return;

  // 3. Пакетирование транзакций (Solana вмещает ~20-30 инструкций в одну tx, но для безопасности берем 10)
  const BATCH_SIZE = 10;
  
  for (let i = 0; i < allVoters.length; i += BATCH_SIZE) {
    const batch = allVoters.slice(i, i + BATCH_SIZE);
    const tx = new anchor.web3.Transaction();

    console.log(`Processing batch ${i / BATCH_SIZE + 1}...`);

    for (const acc of batch) {
      // Добавляем инструкцию закрытия для каждого аккаунта
      const ix = await program.methods
        .closeVoterProof()
        .accounts({
          authority: authority.publicKey,
          voterProof: acc.publicKey,
        })
        .instruction();
      
      tx.add(ix);
    }

    try {
      const sig = await provider.sendAndConfirm(tx, [], { skipPreflight: true });
      console.log(`✅ Batch closed. Tx: ${sig}`);
    } catch (e) {
      console.error(`❌ Error closing batch:`, e);
    }
  }

  console.log("🎉 All accounts closed. Rent recovered!");
}

cleanupVoters();