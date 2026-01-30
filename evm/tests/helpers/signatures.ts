import * as borsh from "borsh"
import { type BigNumberish, Wallet } from "ethers"
import { ethers } from "hardhat"
import type { BridgeTypes } from "../../typechain-types/src/omni-bridge/contracts/OmniBridge"

interface SignatureData<T> {
  payload: T
  signature: string
}

// Constants
const TEST_PRIVATE_KEY = "0x1234567890123456789012345678901234567890123456789012345678901234"
export const testWallet = new Wallet(TEST_PRIVATE_KEY)

// Message Classes
class MetadataMessage {
  static schema = {
    struct: {
      payloadType: "u8",
      token: "string",
      name: "string",
      symbol: "string",
      decimals: "u8",
    },
  }

  constructor(
    public payloadType: number,
    public token: string,
    public name: string,
    public symbol: string,
    public decimals: BigNumberish,
  ) {}

  static serialize(msg: MetadataMessage): Uint8Array {
    return borsh.serialize(MetadataMessage.schema, msg)
  }
}

class TransferMessage {
  static schema = {
    struct: {
      payloadType: "u8",
      destinationNonce: "u64",
      originChain: "u8",
      originNonce: "u64",
      omniBridgeChainId: "u8",
      tokenAddress: { array: { type: "u8", len: 20 } },
      amount: "u128",
      recipientChainId: "u8",
      recipient: { array: { type: "u8", len: 20 } },
      feeRecipient: { option: "string" },
    },
  }

  constructor(
    public payloadType: number,
    public destinationNonce: bigint,
    public originChain: BigNumberish,
    public originNonce: bigint,
    public omniBridgeChainId: number,
    public tokenAddress: Uint8Array,
    public amount: bigint,
    public recipientChainId: number,
    public recipient: Uint8Array,
    public feeRecipient: string | null,
    public message: Uint8Array,
  ) {}

  static serialize(msg: TransferMessage): Uint8Array {
    return borsh.serialize(TransferMessage.schema, msg)
  }
}

// Utility Functions
function createMessageHash(borshEncoded: Uint8Array): string {
  return ethers.keccak256(borshEncoded)
}

function signMessage(messageHash: string): string {
  return testWallet.signingKey.sign(ethers.getBytes(messageHash)).serialized
}

// Main Functions
export function metadataSignature(
  tokenId: string,
): SignatureData<BridgeTypes.MetadataPayloadStruct> {
  const payload: BridgeTypes.MetadataPayloadStruct = {
    token: tokenId,
    name: "Wrapped NEAR fungible token",
    symbol: "wNEAR",
    decimals: 24,
  }

  const message = new MetadataMessage(
    1,
    payload.token,
    payload.name,
    payload.symbol,
    payload.decimals,
  )
  const borshEncoded = MetadataMessage.serialize(message)
  const messageHash = createMessageHash(borshEncoded)
  const signature = signMessage(messageHash)

  return { payload, signature }
}

export function depositSignature(
  tokenAddress: string,
  recipient: string,
): SignatureData<BridgeTypes.TransferMessagePayloadStruct> {
  const payload: BridgeTypes.TransferMessagePayloadStruct = {
    destinationNonce: 1,
    tokenAddress,
    amount: 1,
    recipient,
    feeRecipient: "",
    originChain: 1,
    originNonce: 1,
    message: "0x",
  }

  if (typeof payload.tokenAddress !== "string" || typeof payload.recipient !== "string") {
    throw new Error("tokenAddress and recipient must be strings")
  }

  const message = new TransferMessage(
    0,
    BigInt(payload.destinationNonce),
    payload.originChain,
    BigInt(payload.originNonce),
    0,
    ethers.getBytes(payload.tokenAddress),
    BigInt(payload.amount),
    0,
    ethers.getBytes(payload.recipient),
    null,
    ethers.getBytes(payload.message),
  )

  const borshEncoded = TransferMessage.serialize(message)
  const messageHash = createMessageHash(borshEncoded)
  const signature = signMessage(messageHash)

  return { payload, signature }
}
