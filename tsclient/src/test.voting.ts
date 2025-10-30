import {
    Connection,
    Keypair,
    PublicKey,
    LAMPORTS_PER_SOL,
    clusterApiUrl
} from '@solana/web3.js';
import { CSVPVoteClient } from '../client/src/voteClient';
import { Cluster } from '@arcium/sdk';

describe('CSVP Voting System with Real Arcium Integration', () => {
    let connection: Connection;
    let programId: PublicKey;
    let voteClient: CSVPVoteClient;
    let creator: Keypair;
    let voter: Keypair;

    const ELECTION_ID = 1;
    const ARCIUM_CLUSTER: Cluster = 'mainnet-beta'; // или 'devnet', 'testnet'

    beforeAll(async () => {
        connection = new Connection(clusterApiUrl('devnet'), 'confirmed');
        programId = new PublicKey('CSVP111111111111111111111111111111111111111');

        creator = Keypair.generate();
        voter = Keypair.generate();

        // Фундируем аккаунты
        const airdropSignature1 = await connection.requestAirdrop(creator.publicKey, LAMPORTS_PER_SOL);
        await connection.confirmTransaction(airdropSignature1);

        const airdropSignature2 = await connection.requestAirdrop(voter.publicKey, LAMPORTS_PER_SOL);
        await connection.confirmTransaction(airdropSignature2);

        voteClient = new CSVPVoteClient(connection, programId, ARCIUM_CLUSTER, creator);
    });

    test('Check Arcium cluster health', async () => {
        const clusterInfo = await voteClient.getClusterInfo();
        console.log('Arcium Cluster Info:', clusterInfo);
        expect(clusterInfo).toBeDefined();
    });

    test('Initialize election with real Arcium cluster', async () => {
        const result = await voteClient.initializeElection(
            creator,
            ELECTION_ID,
            'Test Election with Real Arcium',
            'Test election with real Arcium MPC integration',
            Date.now(),
            Date.now() + 24 * 60 * 60 * 1000 // 24 hours
        );

        expect(result.electionPda).toBeDefined();
        expect(result.signature).toBeDefined();
        console.log('Election initialized with PDA:', result.electionPda.toString());
    });

    test('Register voters in chunks', async () => {
        const [electionPda] = await PublicKey.findProgramAddress(
            [
                Buffer.from('election'),
                Buffer.from(ELECTION_ID.toString()),
                creator.publicKey.toBuffer()
            ],
            programId
        );

        const globalSalt = new Uint8Array(32);

        // Регистрируем избирателей в нескольких чанках
        const voterHashes1 = [
            await voteClient.computeVoterHash(new TextEncoder().encode('voter1'), globalSalt),
            await voteClient.computeVoterHash(new TextEncoder().encode('voter2'), globalSalt)
        ];

        const signature1 = await voteClient.registerVoters(creator, electionPda, voterHashes1, 0);
        expect(signature1).toBeDefined();

        const voterHashes2 = [
            await voteClient.computeVoterHash(new TextEncoder().encode('voter3'), globalSalt),
            await voteClient.computeVoterHash(new TextEncoder().encode('voter4'), globalSalt)
        ];

        const signature2 = await voteClient.registerVoters(creator, electionPda, voterHashes2, 1);
        expect(signature2).toBeDefined();

        console.log('Voters registered in multiple chunks');
    });

    test('Cast vote with real Arcium encryption', async () => {
        const [electionPda] = await PublicKey.findProgramAddress(
            [
                Buffer.from('election'),
                Buffer.from(ELECTION_ID.toString()),
                creator.publicKey.toBuffer()
            ],
            programId
        );

        const globalSalt = new Uint8Array(32);
        const voterHash = await voteClient.computeVoterHash(
            new TextEncoder().encode('voter1'),
            globalSalt
        );

        const result = await voteClient.castVote(
            voter,
            electionPda,
            voterHash,
            1, // candidate 1
            `test-election-${ELECTION_ID}`
        );

        expect(result.receiptId).toBeDefined();
        expect(result.signature).toBeDefined();
        console.log('Vote cast with real Arcium encryption');
    });

    test('Tally election with real Arcium MPC', async () => {
        const [electionPda] = await PublicKey.findProgramAddress(
            [
                Buffer.from('election'),
                Buffer.from(ELECTION_ID.toString()),
                creator.publicKey.toBuffer()
            ],
            programId
        );

        const tallyResult = await voteClient.tallyElection(electionPda);

        expect(tallyResult.results).toBeDefined();
        expect(tallyResult.totalVotes).toBeGreaterThan(0);
        expect(tallyResult.proof).toBeDefined();

        console.log('Election tally completed with real Arcium MPC');
        console.log('Results:', Object.fromEntries(tallyResult.results));
        console.log('Total votes:', tallyResult.totalVotes);
    });

    test('Verify vote receipt', async () => {
        const [electionPda] = await PublicKey.findProgramAddress(
            [
                Buffer.from('election'),
                Buffer.from(ELECTION_ID.toString()),
                creator.publicKey.toBuffer()
            ],
            programId
        );

        const globalSalt = new Uint8Array(32);
        const voterHash = await voteClient.computeVoterHash(
            new TextEncoder().encode('voter1'),
            globalSalt
        );
        const receiptSeed = crypto.getRandomValues(new Uint8Array(32));
        const receiptId = await voteClient.computeReceiptId(receiptSeed, electionPda);

        const isVerified = await voteClient.verifyVoteReceipt(electionPda, receiptId);
        expect(isVerified).toBe(true);
        console.log('Vote receipt verified successfully');
    });
});

// Тестирование ошибок
describe('Arcium Error Handling', () => {
    let connection: Connection;
    let programId: PublicKey;
    let voteClient: CSVPVoteClient;

    beforeAll(() => {
        connection = new Connection(clusterApiUrl('devnet'), 'confirmed');
        programId = new PublicKey('CSVP111111111111111111111111111111111111111');
        voteClient = new CSVPVoteClient(connection, programId, 'mainnet-beta');
    });

    test('Handle Arcium encryption errors', async () => {
        const invalidVote = {
            candidateId: NaN, // Invalid data
            electionId: '',
            voterId: '',
            timestamp: -1
        };

        await expect(voteClient.castVote(
            Keypair.generate(),
            PublicKey.unique(),
            new Uint8Array(32),
            NaN,
            ''
        )).rejects.toThrow();
    });
});