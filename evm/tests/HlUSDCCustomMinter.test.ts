import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import * as borsh from "borsh"
import { expect } from "chai"
import { Wallet } from "ethers"
import { ethers, upgrades } from "hardhat"
import type { HlUSDCCustomMinter, OmniBridge } from "../typechain-types"

// TODO: Fill in the actual addresses on HyperLiquid testnet
const USDC_ADDRESS = "0x2B3370eE501B4a559b57D449569354196457D8Ab" // TODO: USDC (native ERC-20) on HyperEVM testnet
const CORE_DEPOSIT_WALLET_ADDRESS = "0x0B80659a4076E9E93C7DbE0f10675A16a3e5C206" // CoreDepositWallet on HyperEVM testnet
const NEAR_USDC_TOKEN_ID = "usdc.omni-bridge.testnet" // TODO: verify the NEAR token ID for USDC

// Test wallet for signing finTransfer payloads (not a secret, testnet only)
const TEST_PRIVATE_KEY = "0x1234567890123456789012345678901234567890123456789012345678901234"
const testWallet = new Wallet(TEST_PRIVATE_KEY)

// OmniBridge chain ID for HyperLiquid (from hardhat.config.ts)
const OMNI_CHAIN_ID = 9

describe("HlUSDCCustomMinter on HyperLiquid Testnet", function () {
  // Big blocks on HyperEVM are produced every ~60s, deploys can take several minutes
  this.timeout(600_000)

  let deployer: HardhatEthersSigner
  let omniBridge: OmniBridge
  let customMinter: HlUSDCCustomMinter
  let usdc: ReturnType<typeof ethers.getContractAt> extends Promise<infer T> ? T : never

  before(async () => {
    ;[deployer] = await ethers.getSigners()
    console.log("Deployer:", deployer.address)

    // Attach to the existing USDC contract on testnet
    usdc = await ethers.getContractAt("IERC20", USDC_ADDRESS)

    const usdcBalance = await usdc.balanceOf(deployer.address)
    console.log("Deployer USDC balance:", usdcBalance.toString())
    expect(usdcBalance).to.be.gt(0, "Deployer must have USDC on testnet")

    // 1. Deploy BridgeToken implementation (required by OmniBridge.initialize)
    const BridgeToken = await ethers.getContractFactory("BridgeToken")
    const bridgeTokenImpl = await BridgeToken.deploy()
    await bridgeTokenImpl.waitForDeployment()
    console.log("BridgeToken impl:", await bridgeTokenImpl.getAddress())

    // 2. Deploy OmniBridge proxy with testWallet as the "MPC signer"
    //    Requires big blocks enabled on HyperEVM (30M gas limit)
    const OmniBridgeFactory = await ethers.getContractFactory("OmniBridge")
    const omniBridgeProxy = await upgrades.deployProxy(
      OmniBridgeFactory,
      [await bridgeTokenImpl.getAddress(), testWallet.address, OMNI_CHAIN_ID],
      { initializer: "initialize" },
    )
    omniBridge = (await omniBridgeProxy.waitForDeployment()) as unknown as OmniBridge
    console.log("OmniBridge:", await omniBridge.getAddress())

    // 3. Deploy HlUSDCCustomMinter proxy
    const MinterFactory = await ethers.getContractFactory("HlUSDCCustomMinter")
    const minterProxy = await upgrades.deployProxy(
      MinterFactory,
      [USDC_ADDRESS, CORE_DEPOSIT_WALLET_ADDRESS, deployer.address, await omniBridge.getAddress()],
      { initializer: "initialize" },
    )
    customMinter = (await minterProxy.waitForDeployment()) as unknown as HlUSDCCustomMinter
    console.log("HlUSDCCustomMinter:", await customMinter.getAddress())

    // 4. Register USDC as a custom token in OmniBridge
    const tx = await omniBridge.addCustomToken(
      NEAR_USDC_TOKEN_ID,
      USDC_ADDRESS,
      await customMinter.getAddress(),
      6, // USDC decimals
    )
    await tx.wait()
    console.log("addCustomToken done")
  })

  it("setup is correct", async () => {
    expect(await customMinter.usdc()).to.equal(USDC_ADDRESS)
    expect(await customMinter.coreDepositWallet()).to.equal(CORE_DEPOSIT_WALLET_ADDRESS)
    expect(await omniBridge.customMinters(USDC_ADDRESS)).to.equal(await customMinter.getAddress())
    expect(
      await customMinter.hasRole(await customMinter.MINTER_ROLE(), await omniBridge.getAddress()),
    ).to.be.true
  })

  it("initTransfer: locks USDC in custom minter", async () => {
    const amount = 1_000_000n // 1 USDC (6 decimals)

    const minterAddress = await customMinter.getAddress()
    const omniBridgeAddress = await omniBridge.getAddress()

    // Approve OmniBridge to pull USDC from deployer
    const approveTx = await usdc.approve(omniBridgeAddress, amount)
    await approveTx.wait()

    const minterBalanceBefore = await usdc.balanceOf(minterAddress)
    const deployerBalanceBefore = await usdc.balanceOf(deployer.address)

    // initTransfer: deployer sends USDC to NEAR
    const initTx = await omniBridge.initTransfer(
      USDC_ADDRESS,
      amount, // amount
      0, // fee
      0, // nativeFee
      "recipient.near", // NEAR recipient
      "", // message
    )
    const receipt = await initTx.wait()
    console.log("initTransfer tx:", receipt?.hash)

    // USDC should now be held by the custom minter (burn is no-op)
    const minterBalanceAfter = await usdc.balanceOf(minterAddress)
    const deployerBalanceAfter = await usdc.balanceOf(deployer.address)

    expect(minterBalanceAfter - minterBalanceBefore).to.equal(amount)
    expect(deployerBalanceBefore - deployerBalanceAfter).to.equal(amount)
  })

  it("finTransfer: sends all USDC from minter via CoreDepositWallet", async () => {
    const recipient = deployer.address
    const minterAddress = await customMinter.getAddress()

    // The minter should hold USDC from the initTransfer above
    const minterBalance = await usdc.balanceOf(minterAddress)
    console.log("Minter USDC balance before finTransfer:", minterBalance.toString())
    expect(minterBalance).to.be.gt(0, "Minter must have USDC from initTransfer")

    // Build and sign finTransfer for the entire minter balance
    const { signature, payload } = buildFinTransferSignature(
      USDC_ADDRESS,
      recipient,
      minterBalance,
      1n,
    )

    const finTx = await omniBridge.finTransfer(signature, payload)
    const receipt = await finTx.wait()
    console.log("finTransfer tx:", receipt?.hash)

    // All USDC should have left the minter (sent to CoreDepositWallet → HyperCore)
    const minterBalanceAfter = await usdc.balanceOf(minterAddress)
    expect(minterBalanceAfter).to.equal(0)
  })
})

// ─── Signature helpers ──────────────────────────────────────────────────────

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
    public originChain: number,
    public originNonce: bigint,
    public omniBridgeChainId: number,
    public tokenAddress: Uint8Array,
    public amount: bigint,
    public recipientChainId: number,
    public recipient: Uint8Array,
    public feeRecipient: string | null,
  ) {}

  static serialize(msg: TransferMessage): Uint8Array {
    return borsh.serialize(TransferMessage.schema, msg)
  }
}

function buildFinTransferSignature(
  tokenAddress: string,
  recipient: string,
  amount: bigint,
  destinationNonce: bigint,
) {
  const payload = {
    destinationNonce,
    tokenAddress,
    amount,
    recipient,
    feeRecipient: "",
    originChain: 1,
    originNonce: 1n,
    message: "0x",
  }

  const message = new TransferMessage(
    0, // PayloadType::TransferMessage
    BigInt(payload.destinationNonce),
    payload.originChain,
    BigInt(payload.originNonce),
    OMNI_CHAIN_ID,
    ethers.getBytes(payload.tokenAddress),
    BigInt(payload.amount),
    OMNI_CHAIN_ID,
    ethers.getBytes(payload.recipient),
    null, // feeRecipient = None
  )

  const borshEncoded = TransferMessage.serialize(message)
  const hashed = ethers.keccak256(borshEncoded)
  const signature = testWallet.signingKey.sign(ethers.getBytes(hashed)).serialized

  return { signature, payload }
}
