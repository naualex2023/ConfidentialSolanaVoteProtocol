import { PublicKey } from '@solana/web3.js';
import { Arcium } from '@arcium/sdk';
export class ArciumVoteClient {
    arcium;
    cluster;
    connection;
    constructor(cluster, connection, wallet) {
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
    async encryptVote(vote) {
        try {
            // Сериализуем голос в Uint8Array
            const voteData = this.serializeVote(vote);
            // Шифруем используя Arcium SDK
            const encryptionResult = await this.arcium.encrypt(voteData, {
                cluster: this.cluster,
            });
            if (!encryptionResult.ciphertext || !encryptionResult.proof) {
                throw new Error('Arcium encryption failed: missing ciphertext or proof');
            }
            return {
                data: encryptionResult.ciphertext,
                proof: encryptionResult.proof,
            };
        }
        catch (error) {
            console.error('Error encrypting vote with Arcium:', error);
            throw new Error(`Failed to encrypt vote: ${error.message}`);
        }
    }
    /**
     * Подсчет результатов используя Arcium MPC
     * На основе официального примера tallying
     */
    async tallyElection(encryptedBallots) {
        try {
            console.log(`Starting MPC tally for ${encryptedBallots.length} ballots on cluster:`, this.cluster);
            // Подготавливаем данные для MPC подсчета
            const tallyInputs = encryptedBallots.map(ballot => ({
                ciphertext: ballot.data,
                proof: ballot.proof,
            }));
            // Вызываем MPC tally через Arcium SDK
            const tallyResult = await this.arcium.tally(tallyInputs, {
                cluster: this.cluster,
                circuit: 'vote_tally', // Используем схему для подсчета голосов
            });
            if (!tallyResult.results || !tallyResult.proof) {
                throw new Error('Arcium tally failed: missing results or proof');
            }
            // Парсим результаты
            const results = this.parseTallyResults(tallyResult.results);
            return {
                results,
                proof: tallyResult.proof,
                totalVotes: encryptedBallots.length,
            };
        }
        catch (error) {
            console.error('Error tallying election with Arcium:', error);
            throw new Error(`Failed to tally election: ${error.message}`);
        }
    }
    /**
     * Верификация результатов подсчета
     */
    async verifyTally(encryptedBallots, tallyResult) {
        try {
            const tallyInputs = encryptedBallots.map(ballot => ({
                ciphertext: ballot.data,
                proof: ballot.proof,
            }));
            const verificationResult = await this.arcium.verifyTally(tallyInputs, Array.from(tallyResult.results.entries()), tallyResult.proof, {
                cluster: this.cluster,
                circuit: 'vote_tally',
            });
            return verificationResult.isValid;
        }
        catch (error) {
            console.error('Error verifying tally with Arcium:', error);
            return false;
        }
    }
    /**
     * Получение публичного ключа кластера для шифрования
     */
    async getClusterPublicKey() {
        try {
            const clusterInfo = await this.arcium.getCluster(this.cluster);
            return clusterInfo.publicKey;
        }
        catch (error) {
            console.error('Error getting cluster public key:', error);
            throw new Error(`Failed to get cluster public key: ${error.message}`);
        }
    }
    /**
     * Проверка доступности кластера Arcium
     */
    async checkClusterHealth() {
        try {
            const health = await this.arcium.getClusterHealth(this.cluster);
            return health.status === 'healthy';
        }
        catch (error) {
            console.error('Error checking cluster health:', error);
            return false;
        }
    }
    /**
     * Получение информации о кластере
     */
    async getClusterInfo() {
        return await this.arcium.getCluster(this.cluster);
    }
    // Приватные вспомогательные методы
    serializeVote(vote) {
        const voteData = {
            c: vote.candidateId, // candidateId
            e: vote.electionId, // electionId  
            v: vote.voterId, // voterId
            t: vote.timestamp, // timestamp
        };
        const jsonString = JSON.stringify(voteData);
        return new TextEncoder().encode(jsonString);
    }
    parseTallyResults(results) {
        const resultMap = new Map();
        try {
            // В зависимости от формата результатов Arcium
            if (Array.isArray(results)) {
                results.forEach((count, candidateId) => {
                    resultMap.set(candidateId, Number(count));
                });
            }
            else if (typeof results === 'object') {
                Object.entries(results).forEach(([candidateId, count]) => {
                    resultMap.set(Number(candidateId), Number(count));
                });
            }
        }
        catch (error) {
            console.error('Error parsing tally results:', error);
        }
        return resultMap;
    }
    /**
     * Получение всех зашифрованных бюллетеней из блокчейна Solana
     */
    async getEncryptedBallotsFromChain(electionPda, programId) {
        const ballots = [];
        let chunkIndex = 0;
        try {
            while (true) {
                const [chunkPda] = await PublicKey.findProgramAddress([
                    Buffer.from('ballot_chunk'),
                    electionPda.toBuffer(),
                    Buffer.from(chunkIndex.toString())
                ], programId);
                const accountInfo = await this.connection.getAccountInfo(chunkPda);
                if (!accountInfo)
                    break;
                // Десериализуем данные чанка
                const chunkData = this.deserializeBallotChunk(accountInfo.data);
                // Добавляем бюллетени из этого чанка
                ballots.push(...chunkData.ballots);
                // Проверяем есть ли следующий чанк
                if (!chunkData.nextChunk)
                    break;
                chunkIndex++;
            }
        }
        catch (error) {
            console.error('Error fetching ballots from chain:', error);
        }
        return ballots;
    }
    deserializeBallotChunk(data) {
        // Упрощенная десериализация - в реальности использовать borsh
        try {
            // Предполагаем, что данные содержат массив бюллетеней
            // Каждый бюллетень: [encrypted_data, proof]
            const ballots = [];
            // Это упрощенная реализация - в production нужна полная десериализация borsh
            let offset = 0;
            // Читаем количество бюллетеней (первые 4 байта)
            const ballotCount = new DataView(data.buffer).getUint32(offset, true);
            offset += 4;
            for (let i = 0; i < ballotCount; i++) {
                // Читаем размер encrypted_data
                const dataSize = new DataView(data.buffer).getUint32(offset, true);
                offset += 4;
                // Читаем encrypted_data
                const encryptedData = data.slice(offset, offset + dataSize);
                offset += dataSize;
                // Читаем размер proof
                const proofSize = new DataView(data.buffer).getUint32(offset, true);
                offset += 4;
                // Читаем proof
                const proof = data.slice(offset, offset + proofSize);
                offset += proofSize;
                ballots.push({ data: encryptedData, proof });
            }
            return { ballots, nextChunk: offset < data.length };
        }
        catch (error) {
            console.error('Error deserializing ballot chunk:', error);
            return { ballots: [], nextChunk: false };
        }
    }
}
//# sourceMappingURL=arciumClient.js.map