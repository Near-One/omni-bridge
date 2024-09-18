import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { BridgeTokenFactory } from "../target/types/bridge_token_factory";

const TOKEN_2022_PROGRAM_ID = new anchor.web3.PublicKey(
  "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
);

describe("bridge_token_factory", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.BridgeTokenFactory as Program<BridgeTokenFactory>;
  const signerKeypair = anchor.web3.Keypair.generate();

  before(async () => {
    const airdropSignature = await provider.connection.requestAirdrop(
      signerKeypair.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    
    await provider.connection.confirmTransaction(airdropSignature);
  });

  async function createToken() {
    const deployTokenDataExample = {
      metadata: {
        token: "wrap.testnet",
        name: "Wrapped NEAR fungible token",
        symbol: "wNEAR",
        decimals: 24
      },
      signature: '43D447B8FF105D740FA7B68D506163D33D8AB2831250DB66A074E45FCF218E0C2EC50105AB9AEDD43556D75C22E790CAEF7F6DC486953D5B266859E23D36C3AB01'
        .match(/.{1,2}/g)
        .map(val => parseInt(val, 16))
    };

    return await program.methods
      .deployToken(deployTokenDataExample)
      .accounts({
        signer: signerKeypair.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .signers([signerKeypair])
      .rpc();
  }

  async function mintTokens() {
    const recipient = new anchor.web3.PublicKey("8PP83wXMJmz2WxRccNcmL4PwLMTTfwU98xDyizoXXX1f");
    const finalizeDepositExample = {
      payload: {
        nonce: new anchor.BN(67),
        token: "wrap.testnet",
        amount: new anchor.BN(10),
        recipient,
        feeRecipient: null,
      },
      signature: '4CEABE9A3520895AFAC6FF182721FF4E0E1ABC443EFCD0B210EB0720F2BD53D7145BE252C35843C44D4242C728ABE448F4DF2DD563E4915B4E6BCD7780E1C70300'
        .match(/.{1,2}/g)
        .map(val => parseInt(val, 16))
    };

    return await program.methods
      .finalizeDeposit(finalizeDepositExample)
      .accounts({
        signer: signerKeypair.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        recipient,
      })
      .signers([signerKeypair])
      .rpc();
  }

  it("should create token", async () => {
    try {
      await createToken();
    } catch (error) {
      console.log("Error", error);
      throw error;
    }
  });

  it("should mint tokens", async () => {
    try {
      await mintTokens();
    } catch (error) {
      console.log("Error", error);
      throw error;
    }
  });
});
