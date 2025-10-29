import { getMXEPublicKey, x25519 } from "@arcium-hq/client";
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { setTimeout } from "timers/promises";
import { randomBytes } from "crypto";

// --- Константы из state.rs ---
export const ELECTION_SEED = Buffer.from("election");
export const SIGN_PDA_SEED = Buffer.from("sign_pda"); // Убедитесь, что это совпадает с lib.rs
export const VOTER_REGISTRY_SEED = Buffer.from("voter_registry");
export const NULLIFIER_SEED = Buffer.from("nullifier");

/**
 * Находит PDA для аккаунта выборов (Election)
 */
export const findElectionPda = (
  programId: PublicKey,
  authority: PublicKey,
  electionId: anchor.BN
): [PublicKey, number] => {
  return PublicKey.findProgramAddressSync(
    [
      ELECTION_SEED,
      authority.toBuffer(),
      electionId.toBuffer("le", 8), // u64, 8 байт, Little Endian
    ],
    programId
  );
};

/**
 * Находит PDA для аккаунта-подписанта Arcium (SignerAccount)
 */
export const findSignPda = (
  programId: PublicKey,
  electionPda: PublicKey
): [PublicKey, number] => {
  return PublicKey.findProgramAddressSync(
    [
      SIGN_PDA_SEED,
      electionPda.toBuffer(),
    ],
    programId
  );
};

/**
 * Находит PDA для чанка избирателей (VoterChunk)
 */
export const findVoterChunkPda = (
  programId: PublicKey,
  electionPda: PublicKey,
  chunkIndex: number
): [PublicKey, number] => {
  const chunkIndexBuffer = Buffer.alloc(4); // u32, 4 байта
  chunkIndexBuffer.writeUInt32LE(chunkIndex, 0);

  return PublicKey.findProgramAddressSync(
    [
      VOTER_REGISTRY_SEED,
      electionPda.toBuffer(),
      chunkIndexBuffer,
    ],
    programId
  );
};

/**
 * Находит PDA для нуллификатора (NullifierAccount)
 */
export const findNullifierPda = (
  programId: PublicKey,
  electionPda: PublicKey,
  nullifierHash: Buffer | Uint8Array
): [PublicKey, number] => {
  return PublicKey.findProgramAddressSync(
    [
      NULLIFIER_SEED,
      electionPda.toBuffer(),
      Buffer.from(nullifierHash),
    ],
    programId
  );
};


// --- Функции из вашего оригинального helpers.ts ---

export const getMXEPublicKeyWithRetry = async function (
  provider: anchor.AnchorProvider,
  programId: PublicKey,
  maxRetries: number = 10,
  retryDelayMs: number = 500
): Promise<Uint8Array> {
  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      const mxePublicKey = await getMXEPublicKey(provider, programId);
      if (mxePublicKey) {
        return mxePublicKey;
      }
    } catch (error) {
      console.log(`Attempt ${attempt} failed to fetch MXE public key:`, error);
    }

    if (attempt < maxRetries) {
      console.log(
        `Retrying in ${retryDelayMs}ms... (attempt ${attempt}/${maxRetries})`
      );
      await setTimeout(retryDelayMs);
    }
  }

  throw new Error(
    `Failed to fetch MXE public key after ${maxRetries} attempts`
  );
}

export const makeClientSideKeys = async function (provider: anchor.AnchorProvider, programId: PublicKey) {

  const mxePublicKey = await getMXEPublicKeyWithRetry(
    provider,
    programId
  );

  console.log("MXE x25519 pubkey is", mxePublicKey);

  const privateKey = x25519.utils.randomSecretKey();
  const publicKey = x25519.getPublicKey(privateKey);
  const sharedSecret = x25519.getSharedSecret(privateKey, mxePublicKey);

  return { privateKey, publicKey, sharedSecret };
}

export const getRandomBigNumber = () => {
  return new anchor.BN(randomBytes(8), "hex")
}