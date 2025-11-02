/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/registration.json`.
 */
export type Registration = {
  "address": "CGZp3yAZwuL9WQbQYpWRgw3fTyXesExjtoSi7sfC29zu",
  "metadata": {
    "name": "registration",
    "version": "0.2.0",
    "spec": "0.1.0",
    "description": "Register voters for Confidential Solana Vote Protocol"
  },
  "instructions": [
    {
      "name": "registerVoter",
      "discriminator": [
        229,
        124,
        185,
        99,
        118,
        51,
        226,
        6
      ],
      "accounts": [
        {
          "name": "authority",
          "writable": true,
          "signer": true
        },
        {
          "name": "voterProof",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  111,
                  116,
                  101,
                  114,
                  115,
                  95,
                  114,
                  101,
                  103,
                  105,
                  115,
                  116,
                  114,
                  121
                ]
              },
              {
                "kind": "arg",
                "path": "voterHash"
              }
            ]
          }
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "chunkIndex",
          "type": "u32"
        },
        {
          "name": "voterHash",
          "type": "pubkey"
        }
      ]
    }
  ],
  "accounts": [
    {
      "name": "voterProof",
      "discriminator": [
        186,
        224,
        160,
        101,
        106,
        116,
        117,
        177
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "chunkFull",
      "msg": "Chunk is full"
    }
  ],
  "types": [
    {
      "name": "voterProof",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "voterHash",
            "type": "pubkey"
          }
        ]
      }
    }
  ]
};
