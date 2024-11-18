"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.deriveChildPublicKey = deriveChildPublicKey;
exports.najPublicKeyStrToUncompressedHexPoint = najPublicKeyStrToUncompressedHexPoint;
exports.uncompressedHexPointToEvmAddress = uncompressedHexPointToEvmAddress;
exports.uncompressedHexPointToBtcAddress = uncompressedHexPointToBtcAddress;
exports.deriveEthereumAddress = deriveEthereumAddress;
const bs58check_1 = __importDefault(require("bs58check"));
const elliptic_1 = require("elliptic");
const hardhat_1 = require("hardhat");
const hash_js_1 = __importDefault(require("hash.js"));
const js_sha3_1 = require("js-sha3");
const serialize_1 = require("near-api-js/lib/utils/serialize");
const rootPublicKey = "secp256k1:4NfTiv3UsGahebgTaHyD9vF8KYKMBnfd6kh94mK6xv8fGBiJB8TBtFMP5WWXz6B89Ac1fbpzPwAvoyQebemHFwx3";
function najPublicKeyStrToUncompressedHexPoint() {
    const res = `04${Buffer.from((0, serialize_1.base_decode)(rootPublicKey.split(":")[1])).toString("hex")}`;
    return res;
}
async function deriveChildPublicKey(parentUncompressedPublicKeyHex, signerId, path = "") {
    const ec = new elliptic_1.ec("secp256k1");
    const scalarHex = (0, js_sha3_1.sha3_256)(`near-mpc-recovery v0.1.0 epsilon derivation:${signerId},${path}`);
    const x = parentUncompressedPublicKeyHex.substring(2, 66);
    const y = parentUncompressedPublicKeyHex.substring(66);
    // Create a point object from X and Y coordinates
    const oldPublicKeyPoint = ec.curve.point(x, y);
    // Multiply the scalar by the generator point G
    const scalarTimesG = ec.g.mul(scalarHex);
    // Add the result to the old public key point
    const newPublicKeyPoint = oldPublicKeyPoint.add(scalarTimesG);
    const newX = newPublicKeyPoint.getX().toString("hex").padStart(64, "0");
    const newY = newPublicKeyPoint.getY().toString("hex").padStart(64, "0");
    return `04${newX}${newY}`;
}
function uncompressedHexPointToEvmAddress(uncompressedHexPoint) {
    const addressHash = hardhat_1.ethers.keccak256(`0x${uncompressedHexPoint.slice(2)}`);
    // Ethereum address is last 20 bytes of hash (40 characters), prefixed with 0x
    return `0x${addressHash.substring(addressHash.length - 40)}`;
}
async function uncompressedHexPointToBtcAddress(publicKeyHex, network) {
    // Step 1: SHA-256 hashing of the public key
    const publicKeyBytes = Uint8Array.from(Buffer.from(publicKeyHex, "hex"));
    const sha256Hash = hash_js_1.default.sha256().update(publicKeyBytes).digest();
    // Step 2: RIPEMD-160 hashing of the SHA-256 hash
    const ripemdHash = hash_js_1.default.ripemd160().update(sha256Hash).digest();
    // Step 3: Add version byte in front (0x00 for mainnet, 0x6f for testnet)
    const versionByte = network === "testnet" ? 0x6f : 0x00;
    const versionedHash = Buffer.concat([Buffer.from([versionByte]), Buffer.from(ripemdHash)]);
    // Step 4: Create checksum and append it
    return bs58check_1.default.encode(versionedHash);
}
async function deriveEthereumAddress(accountId, derivation_path) {
    const uncompressedHexPoint = await deriveChildPublicKey(najPublicKeyStrToUncompressedHexPoint(), accountId, derivation_path);
    return uncompressedHexPointToEvmAddress(uncompressedHexPoint);
}
