import bs58 from "bs58"
import { ec as EC } from "elliptic"
import { ethers } from "ethers"
import { sha3_256 } from "js-sha3"

// sources:
// https://github.com/near-examples/near-multichain/blob/main/src/services/kdf.js
// https://docs.near.org/build/chain-abstraction/chain-signatures/#1-deriving-the-foreign-address

const mpcRootPublicKeys = {
  testnet: {
    accountId: "v1.signer-prod.testnet",
    key: "secp256k1:4NfTiv3UsGahebgTaHyD9vF8KYKMBnfd6kh94mK6xv8fGBiJB8TBtFMP5WWXz6B89Ac1fbpzPwAvoyQebemHFwx3",
  },
  mainnet: {
    accountId: "v1.signer",
    key: "secp256k1:3tFRbMqmoa6AAALMrEFAYCEoHcqKxeW38YptwowBVBtXK1vo36HDbUWuR6EZmoK4JcH6HDkNMGGqP1ouV7VZUWya",
  },
}

function najPublicKeyStrToUncompressedHexPoint(rootPublicKey: string): string {
  const res = `04${Buffer.from(bs58.decode(rootPublicKey.split(":")[1])).toString("hex")}`
  return res
}

async function deriveChildPublicKey(
  parentUncompressedPublicKeyHex: string,
  signerId: string,
  path = "",
): Promise<string> {
  const ec = new EC("secp256k1")
  const scalarHex = sha3_256(`near-mpc-recovery v0.1.0 epsilon derivation:${signerId},${path}`)

  const x = parentUncompressedPublicKeyHex.substring(2, 66)
  const y = parentUncompressedPublicKeyHex.substring(66)

  // Create a point object from X and Y coordinates
  const oldPublicKeyPoint = ec.curve.point(x, y)

  // Multiply the scalar by the generator point G
  const scalarTimesG = ec.g.mul(scalarHex)

  // Add the result to the old public key point
  const newPublicKeyPoint = oldPublicKeyPoint.add(scalarTimesG)
  const newX = newPublicKeyPoint.getX().toString("hex").padStart(64, "0")
  const newY = newPublicKeyPoint.getY().toString("hex").padStart(64, "0")
  return `04${newX}${newY}`
}

function uncompressedHexPointToEvmAddress(uncompressedHexPoint: string): string {
  const addressHash = ethers.keccak256(`0x${uncompressedHexPoint.slice(2)}`)

  // Ethereum address is last 20 bytes of hash (40 characters), prefixed with 0x
  return `0x${addressHash.substring(addressHash.length - 40)}`
}

async function deriveEVMAddress(
  accountId: string,
  derivation_path: string,
  rootPublicKey: string,
): Promise<string> {
  const publicKey = await deriveChildPublicKey(
    najPublicKeyStrToUncompressedHexPoint(rootPublicKey),
    accountId,
    derivation_path,
  )
  return uncompressedHexPointToEvmAddress(publicKey)
}

export { deriveEVMAddress, deriveChildPublicKey, mpcRootPublicKeys }
