"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.metadataSignature = metadataSignature;
exports.depositSignature = depositSignature;
function metadataSignature(tokenId) {
    const signatures = [
        {
            // https://testnet.nearblocks.io/txns/7ptwRrXz5o44RB55Tn82fR6Qvi6wriCzSgy6d4Byb54P#execution
            payload: {
                token: "wrap.testnet",
                name: "Wrapped NEAR fungible token",
                symbol: "wNEAR",
                decimals: 24,
            },
            signature: "0xBA0F62842505C4D93689D5066371F58A81559509CBF177644FBC5045EAA0965C1152C92ADEFF0516C854D6EAECE11DD100F4036CC30922BF873AE4066BA36A811C",
        },
    ];
    const data = signatures.find((s) => s.payload.token === tokenId);
    if (data === undefined)
        throw new Error(`Metadata not found for token ${tokenId}`);
    return data;
}
function depositSignature(tokenId, recipient) {
    const signatures = [
        {
            payload: {
                destinationNonce: 1,
                tokenAddress: "0x856e4424f806D16E8CBC702B3c0F2ede5468eae5",
                amount: 1,
                recipient: "0x3A445243376C32fAba679F63586e236F77EA601e",
                feeRecipient: "",
                originChain: 1, // TODO: Fix this
                originNonce: 1, // TODO: Fix this
            },
            // TODO: Update this signature
            signature: "0xEC712AAF47EADA1542B128231BA1315A4CEDD0578843ABF9E26EC6F9EFF8DFC14BA36FF643ED1EAA46CDE1E80A80126CEAE5FBDA572FE964BB8630C2EA83A7A91C",
        },
    ];
    const data = signatures.find((s) => s.payload.tokenAddress === tokenId &&
        s.payload.recipient.toLowerCase() === recipient.toLowerCase());
    if (data === undefined)
        throw new Error(`Deposit not found for token ${tokenId} and recipient ${recipient}`);
    return data;
}
