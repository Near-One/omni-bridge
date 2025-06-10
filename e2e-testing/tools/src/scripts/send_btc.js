const bitcoin = require('bitcoinjs-lib');
const ECPairFactory = require('ecpair').ECPairFactory;
const tinysecp = require('tiny-secp256k1');
const axios = require('axios');

const ECPair = ECPairFactory(tinysecp);
const network = bitcoin.networks.testnet;

const path = require('path');

require('dotenv').config({
    path: path.resolve(__dirname, '../../.env')
});
const args = process.argv.slice(2);

// ====== Your data here ======
const senderWIF = process.env.BTC_PRIVATE_KEY;          // Sender's private key (WIF)
const senderAddress = process.env.BTC_ACCOUNT_ID;     // Sender's address (P2WPKH)
const recipientAddress = args[0];  // Recipient's address
const sendAmount = Number(args[1]);           // Amount to send (satoshis)
const fee = 1000;                   // Fee (satoshis)
// ============================

// Fetch UTXOs from Blockstream API
async function fetchUtxos(address) {
    const url = `https://blockstream.info/testnet/api/address/${address}/utxo`;
    const res = await axios.get(url);
    return res.data;
}

// Build and send the transaction
async function sendTransaction() {
    const keyPair = ECPair.fromWIF(senderWIF, network);
    const utxos = await fetchUtxos(senderAddress);

    // Select enough UTXOs to cover the amount + fee
    let inputSum = 0;
    let selectedUtxos = [];
    for (const utxo of utxos) {
        selectedUtxos.push(utxo);
        inputSum += utxo.value;
        if (inputSum >= sendAmount + fee) break;
    }

    if (inputSum < sendAmount + fee) {
        console.error('Not enough balance!');
        return;
    }

    const psbt = new bitcoin.Psbt({ network });

    for (const utxo of selectedUtxos) {
        psbt.addInput({
            hash: utxo.txid,
            index: utxo.vout,
            witnessUtxo: {
                script: bitcoin.payments.p2wpkh({ pubkey: Buffer.from(keyPair.publicKey), network }).output,
                value: utxo.value,
            },
        });
    }

    console.log(psbt.txInputs);

    psbt.addOutput({
        address: recipientAddress,
        value: sendAmount,
    });

    const change = inputSum - sendAmount - fee;
    if (change > 0) {
        psbt.addOutput({
            address: senderAddress, // Change goes back to sender
            value: change,
        });
    }

    const signer = {
        publicKey: Buffer.from(keyPair.publicKey),
        sign: (hash) => Buffer.from(keyPair.sign(hash)), // wrap in Buffer
        signSchnorr: undefined, // for compatibility
    };

    psbt.signAllInputs(signer);
    psbt.finalizeAllInputs();

    const txHex = psbt.extractTransaction().toHex();
    console.log('Raw TX:', txHex);

    // Broadcast transaction
    const res = await axios.post('https://blockstream.info/testnet/api/tx', txHex, {
        headers: { 'Content-Type': 'text/plain' }
    });
    console.log('TXID:', res.data);
}

// Run script
sendTransaction().catch(console.error);

//node send_btc.js
