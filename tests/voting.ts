import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import { CsvpProtocol } from "../target/types/csvp_protocol"; // <-- Убедитесь, что имя типа верное
import { randomBytes } from "crypto";
import {
  awaitComputationFinalization,
  getArciumEnv,
  getCompDefAccOffset,
  getArciumAccountBaseSeed,
  getArciumProgAddress,
  uploadCircuit,
  buildFinalizeCompDefTx,
  RescueCipher,
  deserializeLE,
  getMXEAccAddress,
  getMempoolAccAddress,
  getCompDefAccAddress,
  getExecutingPoolAccAddress,
  getComputationAccAddress,
  //getFeePoolAccAddress,
  //getClockAccAddress,
  getClusterAccAddress,
} from "@arcium-hq/client";
import * as fs from "fs/promises";
import * as os from "os";
import { getKeypairFromFile } from "@solana-developers/helpers"
import { describe, it } from "node:test";
import assert from "node:assert";
import { 
  getRandomBigNumber, 
  makeClientSideKeys,
  findElectionPda,
  findSignPda,
  findVoterChunkPda,
  findNullifierPda,
} from "./helpers";

// @ts-ignore
const SECONDS = 1000;

describe("CsvpProtocol", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());
  const program = anchor.workspace.CsvpProtocol as Program<CsvpProtocol>;
  const provider = anchor.getProvider();

  type Event = anchor.IdlEvents<(typeof program)["idl"]>;
  const awaitEvent = async <E extends keyof Event>(eventName: E) => {
    let listenerId: number| null = null;
    const event = await new Promise<Event[E]>((resolve) => {
      listenerId = program.addEventListener(eventName, (event: Event[E]) => {
        resolve(event);
      });
    });
    if (listenerId !== null) await program.removeEventListener(listenerId);

    return event;
  };

  const arciumEnv = getArciumEnv();

  it("проводит полное конфиденциальное голосование!", async () => {
    
    // --- 1. НАСТРОЙКА ---
    const owner = await getKeypairFromFile(`${os.homedir()}/.config/solana/id.json`);
    const voter = owner; // Будем использовать owner как голосующего
    
    console.log("Владелец (Authority/Voter):", owner.publicKey.toBase58());

    const { privateKey, publicKey, sharedSecret } = await makeClientSideKeys(provider as anchor.AnchorProvider, program.programId);
    
    const cipher = new RescueCipher(sharedSecret);
    
    // --- Параметры нашего теста ---
    const ELECTION_ID = new anchor.BN(123); // u64
    const VOTER_CHUNK_INDEX = 0; // u32
    const CHOICE_INDEX = 2; // Голосуем за 3-го кандидата (индекс 2)
    
    // Генерируем фейковые хеши для регистрации и нуллификатора
    // В реальном приложении они бы генерировались криптографически
    const voterHash = Array.from(randomBytes(32));
    const nullifierHash = Array.from(randomBytes(32));
    
    // --- Вычисляем все необходимые PDA ---
    const [electionPda, _electionBump] = findElectionPda(program.programId, owner.publicKey, ELECTION_ID);
    const [signPda, _signBump] = findSignPda(program.programId, electionPda);
    const [voterChunkPda, _voterBump] = findVoterChunkPda(program.programId, electionPda, VOTER_CHUNK_INDEX);
    const [nullifierPda, _nullifierBump] = findNullifierPda(program.programId, electionPda, Buffer.from(nullifierHash));

    console.log("Election PDA:", electionPda.toBase58());
    console.log("Signer PDA:", signPda.toBase58());
    console.log("Voter Chunk PDA:", voterChunkPda.toBase58());

    // --- 2. ИНИЦИАЛИЗАЦИЯ СХЕМ MPC ---
    // (Этот код выглядит корректно и взят из вашего теста)
    console.log("Initializing vote stats computation definition");
    const initVoteStatsSig = await initVoteStatsCompDef(program, owner, false, false);
    console.log("... Vote stats comp def initialized:", initVoteStatsSig);

    console.log("Initializing voting computation definition");
    const initVoteSig = await initVoteCompDef(program, owner, false, false);
    console.log("... Vote comp def initialized:", initVoteSig);

    console.log("Initializing reveal result computation definition");
    const initRRSig = await initRevealResultCompDef(program, owner, false, false);
    console.log("... Reveal result comp def initialized:", initRRSig);
    
    
    // --- 3. СОЗДАНИЕ ВЫБОРОВ (initialize_election) ---
    console.log(`\n🆕 Создание выборов (ID: ${ELECTION_ID.toString()})...`);
    
    //const initCompOffset = getRandomBigNumber();
    const mxeAccountPda = getMXEAccAddress(program.programId);
    
    // Время (start_time, end_time)
    const now = new anchor.BN(Math.floor(Date.now() / 1000));
    const startTime = now.sub(new anchor.BN(60)); // Начались минуту назад
    const endTime = now.add(new anchor.BN(3600)); // Заканчиваются через час
    
    const electionNonce = randomBytes(16);

    const electionComputationOffset = getRandomBigNumber();

    const initSig = await program.methods
      .initVoteStats(
        electionComputationOffset,
        anchor.BN(ELECTION_ID), 
        'Выборы Президента Галактики',
        anchor.BN(startTime),
        anchor.BN(endTime),
        new anchor.BN(deserializeLE(electionNonce).toString())
        //initCompOffset // Этот аргумент нужен из-за `#[instruction]` на структуре
      )
      .accountsPartial({
        // Аккаунты из Rust-структуры `InitializeElection`
        authority: owner.publicKey,
        electionAccount: electionPda,
        signPdaAccount: signPda,
        systemProgram: SystemProgram.programId,
        arciumProgram: getArciumProgAddress(),
        // Arcium аккаунты
        mxeAccount: getMXEAccAddress(program.programId),
        mempoolAccount: getMempoolAccAddress(program.programId),
        executingPool: getExecutingPoolAccAddress(program.programId),
        computationAccount: getComputationAccAddress(program.programId, electionComputationOffset),
        compDefAccount: getCompDefAccAddress(
          program.programId,
          Buffer.from(getCompDefAccOffset("init_vote_stats")).readUInt32LE()
        ),
        clusterAccount: arciumEnv.arciumClusterPubkey,
        // poolAccount: getArciumFeePoolAccAddress(),
        // clockAccount: getArciumClockAccAddress(),
        // poolAccount: getFeePoolAccAddress(), // 👈 --- 2. РАСКОММЕНТИРУЙТЕ ЭТО
        // clockAccount: getClockAccAddress(),
      })
      .rpc({ skipPreflight: true, commitment: "confirmed" });

    console.log("... Транзакция создания выборов отправлена:", initSig);
    
    const finalizeInitSig = await awaitComputationFinalization(
      provider as anchor.AnchorProvider,
      electionComputationOffset,
      program.programId,
      "confirmed"
    );
    console.log("... Выборы финализированы (MPC init_vote_stats выполнен):", finalizeInitSig);
    
    
    // --- 4. РЕГИСТРАЦИЯ ИЗБИРАТЕЛЯ (register_voters) ---
    console.log(`\n📝 Регистрация избирателя в чанке ${VOTER_CHUNK_INDEX}...`);
    
    const registerSig = await program.methods
      .registerVoters(
        VOTER_CHUNK_INDEX,
        [voterHash] // Передаем хеш нашего избирателя
      )
      .accountsPartial({
        authority: owner.publicKey,
        election: electionPda,
        voterRegistry: voterChunkPda,
        systemProgram: SystemProgram.programId,
      })
      .rpc({ skipPreflight: true, commitment: "confirmed" });

    console.log("... Избиратель зарегистрирован:", registerSig);
    

    // --- 5. ГОЛОСОВАНИЕ (cast_vote) ---
    console.log(`\n🗳️  Голосование за кандидата с индексом ${CHOICE_INDEX}...`);
    
    const voteCompOffset = getRandomBigNumber();
    
    // Шифруем наш голос (индекс кандидата)
    const plaintext = [BigInt(CHOICE_INDEX)]; // [u64]
    const voteNonce = randomBytes(16);
    const ciphertext = cipher.encrypt(plaintext, voteNonce); // [u8; 32]
    
    const voteSig = await program.methods
      .castVote(
        voteCompOffset,  
        0, // voter_chunk_index      
        Array.from(ciphertext[0]), // vote_ciphertext
        Array.from(publicKey), // vote_encryption_pubkey
        new anchor.BN(deserializeLE(voteNonce).toString()), // vote_nonce
        nullifierHash, // nullifier_hash
        voterHash, // voter_hash
       // voteCompOffset // computation_offset
      )
      .accountsPartial({
        // Аккаунты из Rust-структуры `CastVote`
        voter: voter.publicKey,
        electionAccount: electionPda,
        voterRegistry: voterChunkPda,
        nullifierAccount: nullifierPda,
        signPdaAccount: signPda,
        systemProgram: SystemProgram.programId,
        arciumProgram: getArciumProgAddress(),
        // Arcium аккаунты
        mxeAccount: mxeAccountPda,
        mempoolAccount: getMempoolAccAddress(program.programId),
        executingPool: getExecutingPoolAccAddress(program.programId),
        computationAccount: getComputationAccAddress(program.programId, voteCompOffset),
        compDefAccount: getCompDefAccAddress( // <-- ВАЖНО: используем 'vote' offset
          program.programId,
          Buffer.from(getCompDefAccOffset("vote")).readUInt32LE()
        ),
        clusterAccount: arciumEnv.arciumClusterPubkey,
        // poolAccount: getFeePoolAccAddress(),
        // clockAccount: getClockAccAddress(),
      })
      .signers([voter]) // 'voter' должен подписать
      .rpc({ skipPreflight: true, commitment: "confirmed" });

    console.log("... Транзакция голосования отправлена:", voteSig);

    const voteEventPromise = awaitEvent("voteEvent"); // <-- Исправлен регистр
    
    const finalizeVoteSig = await awaitComputationFinalization(
      provider as anchor.AnchorProvider,
      voteCompOffset,
      program.programId,
      "confirmed"
    );
    console.log("... Голосование финализировано (MPC vote выполнен):", finalizeVoteSig);

    const voteEvent = await voteEventPromise;
    console.log(
      `... Событие 'VoteEvent' получено! Временная метка:`,
      voteEvent.timestamp.toString()
    );
    
    // --- 6. РАСКРЫТИЕ РЕЗУЛЬТАТОВ (reveal_result) ---
    console.log(`\n🏆 Раскрытие результатов...`);

    // (В реальном приложении мы бы дождались `endTime`)
    
    const revealCompOffset = getRandomBigNumber();
    
    const revealSig = await program.methods
      .revealResult(
      revealCompOffset, // computation_offset
      ELECTION_ID.toNumber(), // id
      )
      .accountsPartial({
        // Аккаунты из Rust-структуры `RevealResult`
        authority: owner.publicKey,
        electionAccount: electionPda,
        signPdaAccount: signPda,
        systemProgram: SystemProgram.programId,
        arciumProgram: getArciumProgAddress(),
        // Arcium аккаунты
        mxeAccount: mxeAccountPda,
        mempoolAccount: getMempoolAccAddress(program.programId),
        executingPool: getExecutingPoolAccAddress(program.programId),
        computationAccount: getComputationAccAddress(program.programId, revealCompOffset),
        compDefAccount: getCompDefAccAddress( // <-- ВАЖНО: используем 'reveal_result' offset
          program.programId,
          Buffer.from(getCompDefAccOffset("reveal_result")).readUInt32LE()
        ),
        clusterAccount: arciumEnv.arciumClusterPubkey,
        // poolAccount: getFeePoolAccAddress(),
        // clockAccount: getClockAccAddress(),
      })
      .signers([owner]) // 'authority' должен подписать
      .rpc({ skipPreflight: true, commitment: "confirmed" });

    console.log("... Транзакция раскрытия отправлена:", revealSig);
    
    const finalizeRevealSig = await awaitComputationFinalization(
      provider as anchor.AnchorProvider,
      revealCompOffset,
      program.programId,
      "confirmed"
    );
    console.log("... Раскрытие финализировано (MPC reveal_result выполнен):", finalizeRevealSig);
    
    // --- 7. ПРОВЕРКА РЕЗУЛЬТАТОВ ---
    // Вместо события, мы загружаем аккаунт и проверяем массив `finalResult`
    
    console.log("... Загрузка обновленного аккаунта выборов...");
    const pollAccount = await program.account.election.fetch(electionPda);
    
    const results = pollAccount.finalResult.map(n => n.toString());
    console.log("... Финальные результаты (массив [u64; 5]):", results);

    // Проверяем, что наш голос (индекс 2) был учтен
    assert.equal(pollAccount.finalResult[CHOICE_INDEX].toNumber(), 1, "Голос за кандидата 2 должен быть 1");
    assert.equal(pollAccount.finalResult[0].toNumber(), 0, "Голос за кандидата 0 должен быть 0");
    
    console.log("\n✅ Тест успешно пройден!");

  });

  // --- Функции инициализации CompDef (без изменений) ---

  async function initVoteStatsCompDef(
    program: Program<CsvpProtocol>,
    owner: anchor.web3.Keypair,
    uploadRawCircuit: boolean,
    offchainSource: boolean
  ): Promise<string> {
    const baseSeedCompDefAcc = getArciumAccountBaseSeed(
      "ComputationDefinitionAccount"
    );
    const offset = getCompDefAccOffset("init_vote_stats");

    const compDefPDA = PublicKey.findProgramAddressSync(
      [baseSeedCompDefAcc, program.programId.toBuffer(), offset],
      getArciumProgAddress()
    )[0];

    const sig = await program.methods
      .initVoteStatsCompDef()
      .accounts({
        compDefAccount: compDefPDA,
        payer: owner.publicKey,
        mxeAccount: getMXEAccAddress(program.programId),
      })
      .signers([owner])
      .rpc({
        commitment: "confirmed",
        preflightCommitment: "confirmed",
      });

 if (uploadRawCircuit) {
      const rawCircuit = await fs.readFile("build/init_vote_stats.arcis");

      await uploadCircuit(
        provider as anchor.AnchorProvider,
        "init_vote_stats",
        program.programId,
        rawCircuit,
        true
      );
    } else if (!offchainSource) {
      const finalizeTx = await buildFinalizeCompDefTx(
        provider as anchor.AnchorProvider,
        Buffer.from(offset).readUInt32LE(),
        program.programId
      );

      const latestBlockhash = await provider.connection.getLatestBlockhash();
      finalizeTx.recentBlockhash = latestBlockhash.blockhash;
      finalizeTx.lastValidBlockHeight = latestBlockhash.lastValidBlockHeight;
      finalizeTx.sign(owner);
      if (!provider.sendAndConfirm) {
        throw new Error("Provider sendAndConfirm method is undefined");
      }
      await provider.sendAndConfirm(finalizeTx);
    }
    return sig;
  }

  async function initVoteCompDef(
    program: Program<CsvpProtocol>,
    owner: anchor.web3.Keypair,
    uploadRawCircuit: boolean,
    offchainSource: boolean
  ): Promise<string> {
    const baseSeedCompDefAcc = getArciumAccountBaseSeed(
      "ComputationDefinitionAccount"
    );
    const offset = getCompDefAccOffset("vote");

    const compDefPDA = PublicKey.findProgramAddressSync(
      [baseSeedCompDefAcc, program.programId.toBuffer(), offset],
      getArciumProgAddress()
    )[0];

    const sig = await program.methods
      .initVoteCompDef()
      .accounts({
        compDefAccount: compDefPDA,
        payer: owner.publicKey,
        mxeAccount: getMXEAccAddress(program.programId),
      })
      .signers([owner])
      .rpc({
        commitment: "confirmed",
        preflightCommitment: "confirmed",
      });

    if (uploadRawCircuit) {
      const rawCircuit = await fs.readFile("build/vote.arcis");

      await uploadCircuit(
        provider as anchor.AnchorProvider,
        "vote",
        program.programId,
        rawCircuit,
        true
      );
    } else if (!offchainSource) {
      const finalizeTx = await buildFinalizeCompDefTx(
        provider as anchor.AnchorProvider,
        Buffer.from(offset).readUInt32LE(),
        program.programId
      );

      const latestBlockhash = await provider.connection.getLatestBlockhash();
      finalizeTx.recentBlockhash = latestBlockhash.blockhash;
      finalizeTx.lastValidBlockHeight = latestBlockhash.lastValidBlockHeight;

      finalizeTx.sign(owner);

      if (!provider.sendAndConfirm) {
        throw new Error("Provider sendAndConfirm method is undefined");
      }
      await provider.sendAndConfirm(finalizeTx);
    }
    return sig;
  }

  async function initRevealResultCompDef(
    program: Program<CsvpProtocol>,
    owner: anchor.web3.Keypair,
    uploadRawCircuit: boolean,
    offchainSource: boolean
  ): Promise<string> {
    const baseSeedCompDefAcc = getArciumAccountBaseSeed(
      "ComputationDefinitionAccount"
    );
    const offset = getCompDefAccOffset("reveal_result");

    const compDefPDA = PublicKey.findProgramAddressSync(
      [baseSeedCompDefAcc, program.programId.toBuffer(), offset],
      getArciumProgAddress()
    )[0];
    
    const sig = await program.methods
      .initRevealResultCompDef()
      .accounts({
        compDefAccount: compDefPDA,
        payer: owner.publicKey,
        mxeAccount: getMXEAccAddress(program.programId),
      })
      .signers([owner])
      .rpc({
        commitment: "confirmed",
        preflightCommitment: "confirmed",
      });

    if (uploadRawCircuit) {
      const rawCircuit = await fs.readFile("build/reveal_result.arcis");

      await uploadCircuit(
        provider as anchor.AnchorProvider,
        "reveal_result",
        program.programId,
        rawCircuit,
        true
      );
    } else if (!offchainSource) {
      const finalizeTx = await buildFinalizeCompDefTx(
        provider as anchor.AnchorProvider,
        Buffer.from(offset).readUInt32LE(),
        program.programId
      );

      const latestBlockhash = await provider.connection.getLatestBlockhash();
      finalizeTx.recentBlockhash = latestBlockhash.blockhash;
      finalizeTx.lastValidBlockHeight = latestBlockhash.lastValidBlockHeight;

      finalizeTx.sign(owner);

      if (!provider.sendAndConfirm) {
        throw new Error("Provider sendAndConfirm method is undefined");
      }
      await provider.sendAndConfirm(finalizeTx);
    }
    return sig;
  }
});