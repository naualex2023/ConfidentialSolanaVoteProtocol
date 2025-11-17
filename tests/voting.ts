import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import { CsvpProtocol } from "../target/types/csvp_protocol"; // <-- Ensure the type name is correct
import { Registration } from "../target/types/registration"; 
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
  findVoterProofPda,
  findNullifierPda,
  findElectionVoteSignPda,
  findFalseVoterProofPda
} from "./helpers";

// @ts-ignore
const SECONDS = 1000;

describe("CsvpProtocol", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());
  const program = anchor.workspace.CsvpProtocol as Program<CsvpProtocol>;
  const Registrationprogram = anchor.workspace.Registration as Program<Registration>;
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

  it("runs a complete confidential voting process!", async () => {
    
    // --- 1. SETUP ---
    const owner = await getKeypairFromFile(`${os.homedir()}/.config/solana/id.json`);
    const voter = owner; // Using the owner as the voter for simplicity
    
    console.log("Owner (Authority/Voter):", owner.publicKey.toBase58());

    const { privateKey, publicKey, sharedSecret } = await makeClientSideKeys(provider as anchor.AnchorProvider, program.programId);
    
    const cipher = new RescueCipher(sharedSecret);
    
    // --- Test Parameters ---
    const ELECTION_ID = new anchor.BN(123); // u64
    const VOTER_CHUNK_INDEX = 0; // u32
    const CHOICE_INDEX = 2; // Voting for the 3rd candidate (index 2)
    const TALLY_CHECK_PERIOD_MS = 5000; // 5 секунд
    
    // Generate fake hashes for registration and nullifier
    // In a real application, these would be cryptographically generated
    //const voterHash = Array.from(randomBytes(32));
    const rawNullifierHashBytes = randomBytes(32);
    
    const rawVoterHashBytes = randomBytes(32);
        
    // 2. Convert the 32-byte hash into a PublicKey object.
    // This is required because the Rust instruction expects a Pubkey.
    // The Anchor SDK automatically serializes this object.
    const voterHashKey = new anchor.web3.PublicKey(rawVoterHashBytes);
    console.log("Generated voter_hash is: ", voterHashKey.toBase58());
    const nullifierHashKey = new anchor.web3.PublicKey(rawNullifierHashBytes);
    
    // --- Calculate all necessary PDAs ---
    // const [electionPda, _electionBump] = findElectionPda(program.programId, owner.publicKey, ELECTION_ID);
    // const [signPda, _signBump] = findSignPda(program.programId, electionPda);
    //const [voterChunkPda, _voterBump] = findVoterChunkPda(program.programId, VOTER_CHUNK_INDEX);
    // Новый PDA зависит от хэша
const [voterProofPDA] = findVoterProofPda(
  voterHashKey // Ваш 32-байтовый Pubkey, переданный как аргумент инструкции
);
    const [falsevoterProofPDA] = findFalseVoterProofPda(program.programId,
  voterHashKey // Ваш 32-байтовый Pubkey, переданный как аргумент инструкции
);
    // --- Calculate all necessary PDAs ---
    const [electionPda, _electionBump] = findElectionPda(program.programId, owner.publicKey, ELECTION_ID);
    const [signPda, _signBump] = findSignPda(program.programId, electionPda);
    const [electionVoteSignPda, _evsBump] = findElectionVoteSignPda(program.programId, electionPda);
    
    console.log("Election PDA:", electionPda.toBase58());
    console.log("Signer PDA:", signPda.toBase58());
    console.log("Voter proof PDA:", voterProofPDA.toBase58());
    console.log("FALSE Voter proof PDA:", falsevoterProofPDA.toBase58());
    console.log("Election Vote Sign PDA:", electionVoteSignPda.toBase58());
    

    // --- 2. INITIALIZE MPC SCHEMAS ---
    console.log("Initializing vote stats computation definition");
    const initVoteStatsSig = await initVoteStatsCompDef(program, owner, false, false);
    console.log("... Vote stats comp def initialized:", initVoteStatsSig);

    console.log("Initializing voting computation definition");
    const initVoteSig = await initVoteCompDef(program, owner, false, false);
    console.log("... Vote comp def initialized:", initVoteSig);

    console.log("Initializing reveal result computation definition");
    const initRRSig = await initRevealResultCompDef(program, owner, false, false);
    console.log("... Reveal result comp def initialized:", initRRSig);
    
    
    // --- 3. CREATE ELECTION (initialize_election) ---
    console.log(`\n🆕 Creating election (ID: ${ELECTION_ID.toString()})...`);
    
    const mxeAccountPda = getMXEAccAddress(program.programId);
    
    // Time (start_time, end_time)
    const now = new anchor.BN(Math.floor(Date.now() / 1000));
    const startTime = now.sub(new anchor.BN(60)); // Started one minute ago
    const endTime = now.add(new anchor.BN(10)); // Ends in 10 seconds
    
    const electionNonce = randomBytes(16);

    const electionComputationOffset = getRandomBigNumber();
    console.log("... Arcium cluster:", arciumEnv.arciumClusterPubkey.toString());

    const initSig = await program.methods
      .initElection(
        electionComputationOffset,
        anchor.BN(ELECTION_ID), 
        'Galactic President Election',
        anchor.BN(startTime),
        anchor.BN(endTime),
        new anchor.BN(deserializeLE(electionNonce).toString())
      )
      .accountsPartial({
        // Arcium accounts
        mxeAccount: getMXEAccAddress(program.programId),
        mempoolAccount: getMempoolAccAddress(program.programId),
        executingPool: getExecutingPoolAccAddress(program.programId),
        computationAccount: getComputationAccAddress(program.programId, electionComputationOffset),
        compDefAccount: getCompDefAccAddress(
          program.programId,
          Buffer.from(getCompDefAccOffset("init_vote_stats")).readUInt32LE()
        ),
        clusterAccount: arciumEnv.arciumClusterPubkey,
        //signPdaAccount: signPda,
      })
      .rpc({ skipPreflight: true, commitment: "confirmed" });

    console.log("... Election creation transaction sent:", initSig);
    
    const finalizeInitSig = await awaitComputationFinalization(
      provider as anchor.AnchorProvider,
      electionComputationOffset,
      program.programId,
      "confirmed"
    );
    console.log("... Election finalized (MPC init_vote_stats executed):", finalizeInitSig);
    
    const electionAccount = await program.account.election.fetch(electionPda);
    console.log("... Election account data:", electionAccount);

    // --- 4. REGISTER VOTER (register_voters) ---
    console.log(`\n📝 Registering voter ...`);
    
    // const registerSig = await program.methods
    //   .registerVoter(
    //     Number(VOTER_CHUNK_INDEX),
    //     voterHashKey // Pass the voter hash (as Pubkey)
    //   )
    //   .accountsPartial({
    //     authority: owner.publicKey,
    //     election: electionPda,
    //     voterRegistry: voterChunkPda,
    //     systemProgram: SystemProgram.programId,
    //   }).signers([owner])
    //   .rpc({ skipPreflight: true, commitment: "confirmed" });
   const registerSig = await Registrationprogram.methods
  .registerVoter(
    0, // chunk_index (теперь игнорируется, но нужен для сигнатуры)
    voterHashKey 
  )
  .accountsPartial({
    authority: owner.publicKey,
    // ✅ Передаем новый PDA
    voterProof: voterProofPDA, 
    systemProgram: SystemProgram.programId,
  }).signers([owner])
  .rpc({ skipPreflight: true, commitment: "confirmed" });
    console.log("... Voter registered:", registerSig);
    const registryAccount = await Registrationprogram.account.voterProof.fetch(voterProofPDA);
    console.log("... VoterProof account data:", registryAccount);

  // Проверяем, существует ли аккаунт, чтобы избежать ошибки "already initialized"
  const accountInfo = await provider.connection.getAccountInfo(signPda);
  
  if (!accountInfo) {
    console.log("Initializing Signer PDA...");
    
    const tx = await program.methods
      .initSignerPda()
      .accounts({
        authority: owner.publicKey, // Предполагаем, что owner - это плательщик
        signPdaAccount: signPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([owner])
      .rpc({ skipPreflight: true, commitment: "confirmed" });
      
    await provider.connection.confirmTransaction(tx, "confirmed");
    console.log("Signer PDA initialized:", tx);
  } else {
    console.log("Signer PDA already exists.");
  }

    // --- 5. CAST VOTE (cast_vote) ---
    console.log(`\n🗳️  Casting vote for candidate index ${CHOICE_INDEX}...`);
    
    const [nullifierPda, _nullifierBump] = findNullifierPda(program.programId, electionPda, nullifierHashKey);
    console.log("Nullifier PDA:", nullifierPda.toBase58());
    console.log("Nullifier hashkey:", nullifierHashKey.toBase58());

// 1. Вычисляем PDA Debug-аккаунта
const [debugPda, debugBump] = PublicKey.findProgramAddressSync(
    [Buffer.from("debug"), voter.publicKey.toBuffer()],
    program.programId
);

const sig = await program.methods
    .debugPdaCheck(nullifierHashKey)// Передаем nullifier_hash как Pubkey
    .accounts({
        payer: voter.publicKey,
        debugPdaAccount: debugPda,
        electionAccount: electionPda,
        nullifierHashAccount: nullifierHashKey, // Передаем тот же хэш как Pubkey
        systemProgram: SystemProgram.programId,
    })
    .signers([voter])
    .rpc();

console.log("Debug PDA Check transaction signature:", sig);

// 4. Считываем данные из Debug-аккаунта
const debugAccountData = await program.account.debugPda.fetch(debugPda);

const programNullifierPda = debugAccountData.pdaValue;

console.log("Program recorded Nullifier PDA:", programNullifierPda.toBase58());

const voterProofData = await Registrationprogram.account.voterProof.fetch(
    voterProofPDA
);

// 3. Выводим записанный хэш
const recordedVoterHash = voterProofData.voterHash.toBase58();

console.log("Recorded voter_hash is: ", recordedVoterHash);

    const voteCompOffset = getRandomBigNumber();
    
    // Encrypt our vote (candidate index)
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
        nullifierHashKey, // nullifier_hash
        voterHashKey, // voter_hash
      )
      .accountsPartial({
        // Accounts from the Rust `CastVote` struct
        voter: voter.publicKey,
        electionAccount: electionPda,
        voterProofAccount: voterProofPDA,
        //nullifierAccount: nullifierPda,
        //signPdaAccount: signPda,
        // systemProgram: SystemProgram.programId,
        // arciumProgram: getArciumProgAddress(),
        // Arcium accounts
        mxeAccount: mxeAccountPda,
        mempoolAccount: getMempoolAccAddress(program.programId),
        executingPool: getExecutingPoolAccAddress(program.programId),
        computationAccount: getComputationAccAddress(program.programId, voteCompOffset),
        compDefAccount: getCompDefAccAddress( // <-- IMPORTANT: use 'vote' offset
          program.programId,
          Buffer.from(getCompDefAccOffset("vote")).readUInt32LE()
        ),
        clusterAccount: arciumEnv.arciumClusterPubkey,
      })
      .signers([voter]) // 'voter' must sign
      .rpc({ skipPreflight: true, commitment: "confirmed" });

    console.log("... Voting transaction sent:", voteSig);

    const voteEventPromise = awaitEvent("voteEvent");
    
    const finalizeVoteSig = await awaitComputationFinalization(
      provider as anchor.AnchorProvider,
      voteCompOffset,
      program.programId,
      "confirmed"
    );
    console.log("... Vote finalized (MPC vote executed):", finalizeVoteSig);

    const voteEvent = await voteEventPromise;
    console.log(
      `... 'VoteEvent' received! Timestamp:`,
      voteEvent.timestamp.toString()
    );
    
    // --- 6. REVEAL RESULTS (reveal_result) ---
    console.log(`\n🏆 Revealing results...`);

    // (In a real app, we would wait for 'endTime')
    
    const revealCompOffset = getRandomBigNumber();
    let success = false;

    let attempts = 0;

    // Цикл ожидания: повторяем, пока не будет успеха или таймаута
    while (!success) {
        attempts++;
        console.log(`--- Попытка #${attempts}  ---`);
        
        // Пауза перед следующей попыткой
        if (attempts > 1) {
            await new Promise(resolve => setTimeout(resolve, TALLY_CHECK_PERIOD_MS));
        }

    try {
    const revealSig = await program.methods
      .revealResult(
      revealCompOffset, // computation_offset
      ELECTION_ID.toNumber(), // id
      )
      .accountsPartial({
        // Accounts from the Rust `RevealResult` struct
        authority: owner.publicKey,
        electionAccount: electionPda,
        //signPdaAccount: signPda,
        // systemProgram: SystemProgram.programId,
        // arciumProgram: getArciumProgAddress(),
        // Arcium accounts
        mxeAccount: mxeAccountPda,
        mempoolAccount: getMempoolAccAddress(program.programId),
        executingPool: getExecutingPoolAccAddress(program.programId),
        computationAccount: getComputationAccAddress(program.programId, revealCompOffset),
        compDefAccount: getCompDefAccAddress( // <-- IMPORTANT: use 'reveal_result' offset
          program.programId,
          Buffer.from(getCompDefAccOffset("reveal_result")).readUInt32LE()
        ),
        clusterAccount: arciumEnv.arciumClusterPubkey,
      })
      .signers([owner]) // 'authority' must sign
      .rpc({ skipPreflight: true, commitment: "confirmed" });
      console.log("... Reveal transaction sent:", revealSig);
      success = true;
        } catch (e) {
          console.log(`❌ FAILURE (EXPECTED): Time has not yet run out. We continue to wait.✅`);
        // Ошибка 0x1771 - Constraint (например, end_time < now)
        // if (e.logs && e.logs.some(log => log.includes("SendTransactionError"))) {
        //     console.log(`❌ FAILURE (EXPECTED): Time has not yet run out. We continue to wait.`);
        // } else {
        //     console.error("❌ Unknown error:", e);
        //     throw e; // Выбрасываем другие ошибки
        // }
        success = false;
    }
    
    if (attempts>50) {
             throw new Error("Tally cycle timed out after too many attempts.");
        }
      }
    const finalizeRevealSig = await awaitComputationFinalization(
      provider as anchor.AnchorProvider,
      revealCompOffset,
      program.programId,
      "confirmed"
    );
    console.log("... Reveal finalized (MPC reveal_result executed):", finalizeRevealSig);
    
    // --- 7. VERIFY RESULTS ---
    // Instead of an event, we load the account and check the `finalResult` array
    
    console.log("... Fetching updated election account...");
    const pollAccount = await program.account.election.fetch(electionPda);
    
    const results = pollAccount.finalResult.map(n => n.toString());
    console.log("... Final results (array of [u64; 5]):", results);

    // Verify that our vote (index 2) was counted
    assert.equal(pollAccount.finalResult[CHOICE_INDEX].toNumber(), 1, "Vote for candidate 2 should be 1");
    assert.equal(pollAccount.finalResult[0].toNumber(), 0, "Vote for candidate 0 should be 0");

      console.log("🔍 Fetching all voter accounts for authority:", owner.publicKey.toBase58());
    
      // 2. Получаем ВСЕ аккаунты VoterProof
      // (Anchor позволяет фильтровать по полям, если нужно, но здесь берем все и фильтруем в JS)
      const allVoters = await Registrationprogram.account.voterProof.all([
        {
          memcmp: {
            offset: 8 + 32, // Пропускаем Discriminator (8) + voter_hash (32), попадаем на authority
            bytes: owner.publicKey.toBase58(),
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
          const ix = await Registrationprogram.methods
            .closeVoterProof()
            .accounts({
              authority: owner.publicKey,
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
    
    console.log("\n✅ Test passed successfully!");

  });

  // --- CompDef Initialization Functions (No changes needed) ---

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
