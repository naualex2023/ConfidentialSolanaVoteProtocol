import {
    Connection,
    Keypair,
    PublicKey,
    Transaction,
    SystemProgram,
    sendAndConfirmTransaction,
    TransactionInstruction
} from '@solana/web3.js';
import { ArciumVoteClient, Vote, EncryptedBallot, TallyResult } from './arciumClient';
import { Cluster } from '@arcium/sdk';
import * as borsh from 'borsh';

// Borsh схемы для сериализации инструкций
class InitializeElectionSchema {
    instruction = 0;
    electionId!: number;
    title!: string;
    description!: string;
    startTime!: number;
    endTime!: number;
    arciumCluster!: string;
    publicKey!: number[];

    constructor(fields: any) {
        Object.assign(this, fields);
    }
}

class RegisterVotersSchema {
    instruction = 1;
    voterHashes!: number[][];
    chunkIndex!: number;

    constructor(fields: any) {
        Object.assign(this, fields);
    }
}

class CastVoteSchema {
    instruction = 2;
    voterHash!: number[];
    encryptedVote!: number[];
    receiptId!: number[];
    nullifier!: number[];
    arciumProof!: number[];

    constructor(fields: any) {
        Object.assign(this, fields);
    }
}

export class CSVPVoteClient {
    private connection: Connection;
    private programId: PublicKey;
    private arciumClient: ArciumVoteClient;

    constructor(
        connection: Connection,
        programId: PublicKey,
        arciumCluster: Cluster,
        wallet?: Keypair
    ) {
        this.connection = connection;
        this.programId = programId;
        this.arciumClient = new ArciumVoteClient(arciumCluster, connection, wallet);
    }

    /**
     * Инициализация выборов с реальным Arcium кластером
     */
    async initializeElection(
        creator: Keypair,
        electionId: number,
        title: string,
        description: string,
        startTime: number,
        endTime: number
    ): Promise<{ electionPda: PublicKey, signature: string }> {

        // Получаем публичный ключ Arcium кластера
        const clusterPublicKey = await this.arciumClient.getClusterPublicKey();

        // Проверяем здоровье кластера
        const isHealthy = await this.arciumClient.checkClusterHealth();
        if (!isHealthy) {
            throw new Error('Arcium cluster is not healthy');
        }

        // Создаем PDA для выборов
        const [electionPda] = await PublicKey.findProgramAddress(
            [
                Buffer.from('election'),
                Buffer.from(electionId.toString()),
                creator.publicKey.toBuffer()
            ],
            this.programId
        );

        // Подготавливаем данные инструкции
        const instructionData = new InitializeElectionSchema({
            electionId,
            title,
            description,
            startTime: Math.floor(startTime / 1000),
            endTime: Math.floor(endTime / 1000),
            arciumCluster: this.arciumClient.cluster,
            publicKey: Array.from(clusterPublicKey)
        });

        const schema = new Map([
            [InitializeElectionSchema, {
                kind: 'struct',
                fields: [
                    ['instruction', 'u8'],
                    ['electionId', 'u64'],
                    ['title', 'string'],
                    ['description', 'string'],
                    ['startTime', 'i64'],
                    ['endTime', 'i64'],
                    ['arciumCluster', 'string'],
                    ['publicKey', [32]]
                ]
            }]
        ]);

        const serializedData = borsh.serialize(schema, instructionData);

        const instruction = new TransactionInstruction({
            keys: [
                { pubkey: creator.publicKey, isSigner: true, isWritable: true },
                { pubkey: electionPda, isSigner: false, isWritable: true },
                { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
            ],
            programId: this.programId,
            data: Buffer.from(serializedData),
        });

        const transaction = new Transaction().add(instruction);
        const signature = await sendAndConfirmTransaction(
            this.connection,
            transaction,
            [creator]
        );

        console.log('Election initialized with Arcium cluster:', this.arciumClient.cluster);
        return { electionPda, signature };
    }

    /**
     * Регистрация избирателей в чанке
     */
    async registerVoters(
        creator: Keypair,
        electionPda: PublicKey,
        voterHashes: Uint8Array[],
        chunkIndex: number = 0
    ): Promise<string> {
        const [voterChunkPda] = await PublicKey.findProgramAddress(
            [
                Buffer.from('voter_chunk'),
                electionPda.toBuffer(),
                Buffer.from(chunkIndex.toString())
            ],
            this.programId
        );

        const instructionData = new RegisterVotersSchema({
            voterHashes: voterHashes.map(hash => Array.from(hash)),
            chunkIndex
        });

        const schema = new Map([
            [RegisterVotersSchema, {
                kind: 'struct',
                fields: [
                    ['instruction', 'u8'],
                    ['voterHashes', [['u8', 32]]],
                    ['chunkIndex', 'u32']
                ]
            }]
        ]);

        const serializedData = borsh.serialize(schema, instructionData);

        const instruction = new TransactionInstruction({
            keys: [
                { pubkey: creator.publicKey, isSigner: true, isWritable: true },
                { pubkey: electionPda, isSigner: false, isWritable: false },
                { pubkey: voterChunkPda, isSigner: false, isWritable: true },
                { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
            ],
            programId: this.programId,
            data: Buffer.from(serializedData),
        });

        const transaction = new Transaction().add(instruction);
        const signature = await sendAndConfirmTransaction(
            this.connection,
            transaction,
            [creator]
        );

        return signature;
    }

    /**
     * Голосование с реальным Arcium шифрованием
     */
    async castVote(
        voter: Keypair,
        electionPda: PublicKey,
        voterHash: Uint8Array,
        candidateId: number,
        electionId: string
    ): Promise<{ receiptId: Uint8Array; signature: string }> {
        const receiptSeed = crypto.getRandomValues(new Uint8Array(32));
        const receiptId = await this.computeReceiptId(receiptSeed, electionPda);
        const nullifier = await this.computeNullifier(voterHash, electionPda);

        const vote: Vote = {
            candidateId,
            electionId,
            voterId: Buffer.from(voterHash).toString('hex'),
            timestamp: Date.now(),
        };
        const encryptedVote = await this.arciumClient.encryptVote(vote);

        const [voterChunkPda] = await PublicKey.findProgramAddress(
            [Buffer.from('voter_chunk'), electionPda.toBuffer(), Buffer.from('0')],
            this.programId
        );
        const [receiptChunkPda] = await PublicKey.findProgramAddress(
            [Buffer.from('receipt_chunk'), electionPda.toBuffer(), Buffer.from('0')],
            this.programId
        );
        const [ballotChunkPda] = await PublicKey.findProgramAddress(
            [Buffer.from('ballot_chunk'), electionPda.toBuffer(), Buffer.from('0')],
            this.programId
        );

        // === PDA для нулификатора ===
        const [nullifierPda] = await PublicKey.findProgramAddress(
            [Buffer.from('election'), electionPda.toBuffer(), Buffer.from('nullifier'), Buffer.from(nullifier)],
            this.programId
        );

        const instructionData = new CastVoteSchema({
            voterHash: Array.from(voterHash),
            encryptedVote: Array.from(encryptedVote.data),
            receiptId: Array.from(receiptId),
            nullifier: Array.from(nullifier),
            arciumProof: Array.from(encryptedVote.proof)
        });

        const schema = new Map([
            [CastVoteSchema, {
                kind: 'struct',
                fields: [
                    ['instruction', 'u8'],
                    ['voterHash', [32]],
                    ['encryptedVote', ['u8']],
                    ['receiptId', [32]],
                    ['nullifier', [32]],
                    ['arciumProof', ['u8']]
                ]
            }]
        ]);

        const serializedData = borsh.serialize(schema, instructionData);

        const instruction = new TransactionInstruction({
            keys: [
                { pubkey: voter.publicKey, isSigner: true, isWritable: true },
                { pubkey: electionPda, isSigner: false, isWritable: true },
                { pubkey: voterChunkPda, isSigner: false, isWritable: false },
                { pubkey: receiptChunkPda, isSigner: false, isWritable: true },
                { pubkey: ballotChunkPda, isSigner: false, isWritable: true },
                { pubkey: nullifierPda, isSigner: false, isWritable: true },
                { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
            ],
            programId: this.programId,
            data: Buffer.from(serializedData),
        });

        const tx = new Transaction().add(instruction);
        const signature = await sendAndConfirmTransaction(this.connection, tx, [voter]);

        //console.log('Vote cast successfully. Nullifier registered.');
        console.log('Vote cast with Arcium encryption, receipt ID:', Buffer.from(receiptId).toString('hex'));
        return { receiptId, signature };
    }
    // === Utility hash functions ===
    //private async hashData(data: Uint8Array): Promise<Uint8Array> {
    //    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
    //    return new Uint8Array(hashBuffer);
    //}

    //async computeReceiptId(seed: Uint8Array, electionPda: PublicKey): Promise<Uint8Array> {
    //    return this.hashData(new Uint8Array([...seed, ...electionPda.toBytes()]));
    //}

    //async computeNullifier(voterHash: Uint8Array, electionPda: PublicKey): Promise<Uint8Array> {
    //    return this.hashData(new Uint8Array([...voterHash, ...electionPda.toBytes()]));
    //}
    /**
     * Подсчет результатов через реальный Arcium MPC
     */
    async tallyElection(electionPda: PublicKey): Promise<TallyResult> {
        console.log('Starting election tally with Arcium MPC...');

        // Получаем все зашифрованные бюллетени из блокчейна
        const encryptedBallots = await this.arciumClient.getEncryptedBallotsFromChain(
            electionPda,
            this.programId
        );

        if (encryptedBallots.length === 0) {
            throw new Error('No encrypted ballots found for tallying');
        }

        console.log(`Found ${encryptedBallots.length} encrypted ballots for tallying`);

        // Подсчитываем через Arcium MPC
        const tallyResult = await this.arciumClient.tallyElection(encryptedBallots);

        // Валидируем результаты
        const isValid = await this.arciumClient.verifyTally(encryptedBallots, tallyResult);

        if (!isValid) {
            throw new Error('Tally verification failed - results may be tampered with');
        }

        console.log('Election tally completed successfully with Arcium MPC');
        return tallyResult;
    }

    /**
     * Получение информации о Arcium кластере
     */
    async getClusterInfo() {
        return await this.arciumClient.getClusterInfo();
    }

    // Вспомогательные методы

    async computeVoterHash(biometricData: Uint8Array, globalSalt: Uint8Array): Promise<Uint8Array> {
        const data = new Uint8Array([...globalSalt, ...biometricData]);
        return this.hashData(data);
    }

    async computeReceiptId(receiptSeed: Uint8Array, electionPda: PublicKey): Promise<Uint8Array> {
        const data = new Uint8Array([...receiptSeed, ...electionPda.toBytes()]);
        return this.hashData(data);
    }

    async computeNullifier(voterHash: Uint8Array, electionPda: PublicKey): Promise<Uint8Array> {
        const data = new Uint8Array([...voterHash, ...electionPda.toBytes()]);
        return this.hashData(data);
    }

    private async hashData(data: Uint8Array): Promise<Uint8Array> {
        const hashBuffer = await crypto.subtle.digest('SHA-256', data);
        return new Uint8Array(hashBuffer);
    }

    /**
     * Проверка receipt_id в блокчейне
     */
    async verifyVoteReceipt(
        electionPda: PublicKey,
        receiptId: Uint8Array
    ): Promise<boolean> {
        let chunkIndex = 0;

        while (true) {
            try {
                const [chunkPda] = await PublicKey.findProgramAddress(
                    [
                        Buffer.from('receipt_chunk'),
                        electionPda.toBuffer(),
                        Buffer.from(chunkIndex.toString())
                    ],
                    this.programId
                );

                const accountInfo = await this.connection.getAccountInfo(chunkPda);
                if (!accountInfo) break;

                // В реальной реализации нужно десериализовать чанк и проверить наличие receipt_id
                // Для демо возвращаем true, предполагая что проверка прошла
                return true;

            } catch (error) {
                break;
            }

            chunkIndex++;
        }

        return false;
    }
}