import bs58check from "bs58check"
import { ec as EC } from "elliptic"
import { ethers } from "hardhat"
import hash from "hash.js"
import { sha3_256 } from "js-sha3"
import { base_decode } from "near-api-js/lib/utils/serialize"

const rootPublicKey =
	"secp256k1:4NfTiv3UsGahebgTaHyD9vF8KYKMBnfd6kh94mK6xv8fGBiJB8TBtFMP5WWXz6B89Ac1fbpzPwAvoyQebemHFwx3"

function najPublicKeyStrToUncompressedHexPoint(): string {
	const res = `04${Buffer.from(base_decode(rootPublicKey.split(":")[1])).toString("hex")}`
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

async function uncompressedHexPointToBtcAddress(
	publicKeyHex: string,
	network: string,
): Promise<string> {
	// Step 1: SHA-256 hashing of the public key
	const publicKeyBytes = Uint8Array.from(Buffer.from(publicKeyHex, "hex"))
	const sha256Hash = hash.sha256().update(publicKeyBytes).digest()

	// Step 2: RIPEMD-160 hashing of the SHA-256 hash
	const ripemdHash = hash.ripemd160().update(sha256Hash).digest()

	// Step 3: Add version byte in front (0x00 for mainnet, 0x6f for testnet)
	const versionByte = network === "testnet" ? 0x6f : 0x00
	const versionedHash = Buffer.concat([Buffer.from([versionByte]), Buffer.from(ripemdHash)])

	// Step 4: Create checksum and append it
	return bs58check.encode(versionedHash)
}

async function deriveEthereumAddress(accountId: string, derivation_path: string): Promise<string> {
	const uncompressedHexPoint = await deriveChildPublicKey(
		najPublicKeyStrToUncompressedHexPoint(),
		accountId,
		derivation_path,
	)
	return uncompressedHexPointToEvmAddress(uncompressedHexPoint)
}

export {
	deriveChildPublicKey,
	najPublicKeyStrToUncompressedHexPoint,
	uncompressedHexPointToEvmAddress,
	uncompressedHexPointToBtcAddress,
	deriveEthereumAddress,
}
