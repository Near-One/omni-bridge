interface MetadataPayload {
    token: string;
    name: string;
    symbol: string;
    decimals: number;
}
interface DepositPayload {
    destinationNonce: number;
    originChain: number;
    originNonce: number;
    tokenAddress: string;
    amount: number;
    recipient: string;
    feeRecipient: string;
}
interface SignatureData<T> {
    payload: T;
    signature: string;
}
declare function metadataSignature(tokenId: string): SignatureData<MetadataPayload>;
declare function depositSignature(tokenId: string, recipient: string): SignatureData<DepositPayload>;
export { metadataSignature, depositSignature };
