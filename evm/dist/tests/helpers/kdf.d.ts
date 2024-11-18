declare function najPublicKeyStrToUncompressedHexPoint(): string;
declare function deriveChildPublicKey(parentUncompressedPublicKeyHex: string, signerId: string, path?: string): Promise<string>;
declare function uncompressedHexPointToEvmAddress(uncompressedHexPoint: string): string;
declare function uncompressedHexPointToBtcAddress(publicKeyHex: string, network: string): Promise<string>;
declare function deriveEthereumAddress(accountId: string, derivation_path: string): Promise<string>;
export { deriveChildPublicKey, najPublicKeyStrToUncompressedHexPoint, uncompressedHexPointToEvmAddress, uncompressedHexPointToBtcAddress, deriveEthereumAddress, };
