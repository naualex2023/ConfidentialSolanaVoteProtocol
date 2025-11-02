import { getMXEPublicKey, x25519 } from "@arcium-hq/client";
import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { setTimeout } from "timers/promises";
import { randomBytes } from "crypto";

// --- Константы из state.rs ---
export const ELECTION_SEED = Buffer.from("election");
export const SIGN_PDA_SEED = Buffer.from("signer_account"); // Убедитесь, что это совпадает с lib.rs
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
      //electionPda.toBuffer(),
    ],
    programId
  );
};

// ID ВАШЕЙ ПРОГРАММЫ РЕГИСТРАЦИИ
const REG_PROGRAM_ID = new PublicKey("CGZp3yAZwuL9WQbQYpWRgw3fTyXesExjtoSi7sfC29zu"); 

export const findVoterProofPda = (
  voterHash: PublicKey // Должен быть Pubkey
): [PublicKey, number] => {
  // Сиды: [b"voters_registry", voter_hash.as_ref()]
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("voters_registry"), // Имя сида должно совпадать с VOTER_REGISTRY_SEED
      voterHash.toBuffer(),            // Pubkey должен быть преобразован в 32-байтовый буфер
    ],
    REG_PROGRAM_ID // ID программы, которая владеет аккаунтом VoterProof
  );
};

/**
 * Находит PDA для нуллификатора (NullifierAccount)
 */
export const findNullifierPda = (
  programId: PublicKey,
  electionPda: PublicKey,
  nullifierHash: PublicKey
): [PublicKey, number] => {
  return PublicKey.findProgramAddressSync(
    [
      NULLIFIER_SEED,
      electionPda.toBuffer(),
      nullifierHash.toBuffer(),
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