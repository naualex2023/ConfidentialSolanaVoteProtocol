import { Connection, Keypair, clusterApiUrl } from '@solana/web3.js';
import { CSVPVoteClient } from './voteClient';
import { Cluster } from '@arcium/sdk';

async function main() {
    console.log('CSVP Voting System Demo with Real Arcium Integration\n');

    // Конфигурация
    const connection = new Connection(clusterApiUrl('devnet'), 'confirmed');
    const programId = new PublicKey('CSVP111111111111111111111111111111111111111');
    const arciumCluster: Cluster = 'mainnet-beta'; // Используем mainnet Arcium

    // Создаем ключи
    const creator = Keypair.generate();
    const voter = Keypair.generate();

    // Инициализируем клиент
    const voteClient = new CSVPVoteClient(connection, programId, arciumCluster, creator);

    try {
        // Шаг 1: Проверяем Arcium кластер
        console.log('1. Checking Arcium cluster...');
        const clusterInfo = await voteClient.getClusterInfo();
        console.log('✅ Arcium cluster is ready:', clusterInfo.name);

        // Шаг 2: Инициализируем выборы
        console.log('\n2. Initializing election...');
        const { electionPda } = await voteClient.initializeElection(
            creator,
            12345,
            'Community Proposal Vote',
            'Vote on the new community center proposal',
            Date.now(),
            Date.now() + 7 * 24 * 60 * 60 * 1000 // 7 days
        );
        console.log('✅ Election initialized:', electionPda.toString());

        // Шаг 3: Регистрируем избирателей
        console.log('\n3. Registering voters...');
        const globalSalt = new Uint8Array(32);
        const voterHashes = [
            await voteClient.computeVoterHash(new TextEncoder().encode('alice-fingerprint'), globalSalt),
            await voteClient.computeVoterHash(new TextEncoder().encode('bob-fingerprint'), globalSalt),
        ];

        await voteClient.registerVoters(creator, electionPda, voterHashes, 0);
        console.log('✅ Voters registered');

        // Шаг 4: Голосование
        console.log('\n4. Casting votes...');
        const voterHash = await voteClient.computeVoterHash(
            new TextEncoder().encode('alice-fingerprint'),
            globalSalt
        );

        const voteResult = await voteClient.castVote(
            voter,
            electionPda,
            voterHash,
            1, // За предложение
            'community-proposal-12345'
        );
        console.log('✅ Vote cast with receipt:', Buffer.from(voteResult.receiptId).toString('hex'));

        // Шаг 5: Подсчет результатов
        console.log('\n5. Tallying election with Arcium MPC...');
        const tallyResult = await voteClient.tallyElection(electionPda);

        console.log('✅ Election results:');
        console.log('   Total votes:', tallyResult.totalVotes);
        console.log('   Results:', Object.fromEntries(tallyResult.results));
        console.log('   Tally proof length:', tallyResult.proof.length, 'bytes');

        // Шаг 6: Верификация
        console.log('\n6. Verifying vote receipt...');
        const isVerified = await voteClient.verifyVoteReceipt(electionPda, voteResult.receiptId);
        console.log('✅ Vote receipt verified:', isVerified);

        console.log('\n🎉 Demo completed successfully!');

    } catch (error) {
        console.error('❌ Demo failed:', error);
    }
}

main().catch(console.error);