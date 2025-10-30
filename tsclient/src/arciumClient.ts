import * as borsh from 'borsh';
import { BN } from '@coral-xyz/anchor';
import { RescueCipher, x25519 } from "@arcium-hq/client";
import { Connection, Keypair, PublicKey } from '@solana/web3.js';
// import {
//   awaitComputationFinalization,
//   getArciumEnv,
//   getCompDefAccOffset,
//   getArciumAccountBaseSeed,
//   getArciumProgAddress,
//   uploadCircuit,
//   buildFinalizeCompDefTx,
//   RescueCipher,
//   deserializeLE,
//   getMXEAccAddress,
//   getMempoolAccAddress,
//   getCompDefAccAddress,
//   getExecutingPoolAccAddress,
//   x25519,
//   getComputationAccAddress,
//   getMXEPublicKey,
// } from "@arcium-hq/client";
const UserVoteSchema = new Map([
    [Object, {
        kind: 'struct',
        fields: [
            ['candidate_index', 'u64'] // Индекс кандидата (0 до N-1)
        ]
    }]
]);

export interface Vote {
    candidateId: number;
    electionId: string;
    voterId: string;
    timestamp: number;
}

export interface EncryptedBallot {
    data: Uint8Array;
    proof: Uint8Array;
}

export interface TallyResult {
    results: Map<number, number>;
    proof: Uint8Array;
    totalVotes: number;
}

export class ArciumVoteClient {
    private arcium: Arcium;
    private cluster: Cluster;
    private connection: Connection;

    constructor(
        cluster: Cluster,
        connection: Connection,
        wallet?: Keypair
    ) {
        this.connection = connection;
        this.cluster = cluster;

        // Инициализация Arcium SDK на основе официального примера
        this.arcium = new Arcium({
            cluster: this.cluster,
            connection: this.connection,
            wallet: wallet, // Опционально для подписывания транзакций
        });
    }

    /**
     * Шифрование голоса используя реальный Arcium SDK
     * На основе: https://github.com/arcium-hq/examples/blob/main/voting/src/components/Voting.tsx
     */
  /**
 * Шифрует выбор кандидата для MPC
 * @param candidateIndex - Индекс кандидата (0 до MAX_CANDIDATES - 1)
 * @param nonce - Криптографический nonce из Election Account
 * @param mxePub - Публичный ключ MXE
 * @returns 32-байтовый шифротекст (Enc<Shared, UserVote>)
 */
export async function encryptVote(
    candidateIndex: number, 
    nonce: string, 
    mxePub: Uint8Array
): Promise<Uint8Array> {
    // 1. Создание общего секрета
    const clientEphemeral = x25519.generateKeyPair();
    const sharedSecret = x25519.getSharedSecret(clientEphemeral.secretKey, mxePub);

    // 2. Сериализация голоса (UserVote)
    // BN нужен для корректной сериализации 'u64'
    const voteStruct = { candidate_index: new BN(candidateIndex) };
    const userVoteBorsh = Buffer.from(borsh.serialize(UserVoteSchema, voteStruct));
    
    // 3. Шифрование (используя nonce для инициализации RescueCipher)
    const nonceBuffer = Buffer.from(nonce, 'hex'); // nonce должен быть 128-битным (16 байт)
    const cipher = new RescueCipher(sharedSecret, nonceBuffer);
    
    // MPC ожидает 32-байтовый шифротекст.
    const encryptedVote = cipher.encrypt(userVoteBorsh).slice(0, 32); 
    
    return encryptedVote;
}


}