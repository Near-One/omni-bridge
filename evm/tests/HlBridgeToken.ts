import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import { expect } from "chai"
import { ethers, upgrades } from "hardhat"
import type {
  HyperliquedBridgeToken,
  OmniBridgeWormholeDeferred,
  TestWormhole,
} from "../typechain-types"
import { testWallet } from "./helpers/signatures"

const ACTION_TRANSFER = 0
const ACTION_INIT_TRANSFER = 1
const WORMHOLE_FEE = 10000n
const CONSISTENCY_LEVEL = 0

describe("HyperliquedBridgeToken", () => {
  let adminAccount: HardhatEthersSigner
  let user1: HardhatEthersSigner
  let user2: HardhatEthersSigner
  let systemSigner: HardhatEthersSigner

  let omniBridge: OmniBridgeWormholeDeferred
  let omniBridgeAddress: string
  let testWormhole: TestWormhole

  const SYSTEM_ADDRESS = "0x2222000000000000000000000000000000000000"
  const NEAR_TOKEN_ID = "hl.testnet"

  beforeEach(async () => {
    ;[adminAccount, user1, user2] = await ethers.getSigners()
    systemSigner = await ethers.getImpersonatedSigner(SYSTEM_ADDRESS)
    await adminAccount.sendTransaction({
      to: SYSTEM_ADDRESS,
      value: ethers.parseEther("1"),
    })

    const BridgeToken_factory = await ethers.getContractFactory("BridgeToken")
    const bridgeTokenImpl = await BridgeToken_factory.deploy()
    await bridgeTokenImpl.waitForDeployment()

    const testWormhole_factory = await ethers.getContractFactory("TestWormhole")
    testWormhole = await testWormhole_factory.deploy()
    await testWormhole.waitForDeployment()

    // The HyperEVM deployment is the deferred variant: HyperCore-originated
    // transfers are committed by the token and submitted in a second transaction.
    const factory = await ethers.getContractFactory("OmniBridgeWormholeDeferred")
    const proxy = await upgrades.deployProxy(
      factory,
      [
        await bridgeTokenImpl.getAddress(),
        testWallet.address,
        0,
        await testWormhole.getAddress(),
        CONSISTENCY_LEVEL,
      ],
      { initializer: "initializeWormhole" },
    )
    omniBridge = (await proxy.waitForDeployment()) as unknown as OmniBridgeWormholeDeferred
    omniBridgeAddress = await omniBridge.getAddress()
  })

  async function deployHlToken(): Promise<{
    token: HyperliquedBridgeToken
    address: string
  }> {
    const HlFactory = await ethers.getContractFactory("HyperliquedBridgeToken")
    const deployed = await upgrades.deployProxy(
      HlFactory,
      ["Wrapped HL", "wHL", 18, SYSTEM_ADDRESS, adminAccount.address],
      { initializer: "initialize(string,string,uint8,address,address)", kind: "uups" },
    )
    const token = (await deployed.waitForDeployment()) as unknown as HyperliquedBridgeToken
    return { token, address: await token.getAddress() }
  }

  // `addCustomToken` with `customMinter = address(0)` marks the token as a bridge
  // token, which both authorizes `queueInitTransfer` and selects the burn path.
  async function registerHlOnBridge(tokenAddress: string) {
    // addCustomToken publishes a LogMetadata message, so it carries the fee too.
    await omniBridge.addCustomToken(NEAR_TOKEN_ID, tokenAddress, ethers.ZeroAddress, 18, {
      value: WORMHOLE_FEE,
    })
  }

  describe("3-arg mint (HyperCore path)", () => {
    let token: HyperliquedBridgeToken

    beforeEach(async () => {
      ;({ token } = await deployHlToken())
    })

    it("mints to account then routes balance to system address", async () => {
      await token.connect(adminAccount)["mint(address,uint256,bytes)"](user1.address, 1000, "0x")
      expect(await token.balanceOf(user1.address)).to.equal(0n)
      expect(await token.balanceOf(SYSTEM_ADDRESS)).to.equal(1000n)
    })

    it("rejects non-owner callers", async () => {
      await expect(
        token.connect(user1)["mint(address,uint256,bytes)"](user1.address, 1000, "0x"),
      ).to.be.revertedWithCustomError(token, "OwnableUnauthorizedAccount")
    })
  })

  describe("coreReceiveWithData authorization & dispatch", () => {
    let token: HyperliquedBridgeToken

    beforeEach(async () => {
      ;({ token } = await deployHlToken())
    })

    it("reverts when caller is not the system address", async () => {
      await expect(
        token.connect(user1).coreReceiveWithData(user1.address, ethers.ZeroHash, 0, 100, 0, "0x00"),
      ).to.be.revertedWithCustomError(token, "NotSystemAddress")
    })

    it("reverts on empty data", async () => {
      await expect(
        token
          .connect(systemSigner)
          .coreReceiveWithData(user1.address, ethers.ZeroHash, 0, 100, 0, "0x"),
      ).to.be.revertedWithCustomError(token, "EmptyActionData")
    })

    it("reverts on unknown action tag", async () => {
      await expect(
        token
          .connect(systemSigner)
          .coreReceiveWithData(user1.address, ethers.ZeroHash, 0, 100, 0, "0x99"),
      )
        .to.be.revertedWithCustomError(token, "UnknownAction")
        .withArgs(0x99)
    })
  })

  describe("ACTION_TRANSFER (0x00)", () => {
    const AMOUNT = 500n
    let token: HyperliquedBridgeToken
    let tokenAddress: string

    beforeEach(async () => {
      ;({ token, address: tokenAddress } = await deployHlToken())
      await token.connect(adminAccount)["mint(address,uint256)"](SYSTEM_ADDRESS, AMOUNT)
    })

    function transferData(to: string) {
      return ethers.concat(["0x00", ethers.AbiCoder.defaultAbiCoder().encode(["address"], [to])])
    }

    it("releases tokens from the system-address pool to recipient", async () => {
      const data = transferData(user2.address)
      await expect(
        token
          .connect(systemSigner)
          .coreReceiveWithData(user1.address, ethers.ZeroHash, 0, AMOUNT, 0, data),
      )
        .to.emit(token, "CoreReceived")
        .withArgs(user1.address, ACTION_TRANSFER, 0, AMOUNT, data)

      expect(await token.balanceOf(user2.address)).to.equal(AMOUNT)
      expect(await token.balanceOf(SYSTEM_ADDRESS)).to.equal(0n)
      expect(await token.balanceOf(tokenAddress)).to.equal(0n)
    })

    it("rejects the zero address, which _update would treat as a burn", async () => {
      await expect(
        token
          .connect(systemSigner)
          .coreReceiveWithData(
            user1.address,
            ethers.ZeroHash,
            0,
            AMOUNT,
            0,
            transferData(ethers.ZeroAddress),
          ),
      ).to.be.revertedWithCustomError(token, "InvalidRecipient")
      expect(await token.totalSupply()).to.equal(AMOUNT)
    })

    it("reverts if the system-address pool is insufficient", async () => {
      await expect(
        token
          .connect(systemSigner)
          .coreReceiveWithData(
            user1.address,
            ethers.ZeroHash,
            0,
            AMOUNT + 1n,
            0,
            transferData(user2.address),
          ),
      ).to.be.revertedWithCustomError(token, "ERC20InsufficientBalance")
    })
  })

  describe("ACTION_INIT_TRANSFER (0x01) — deferred via OmniBridge", () => {
    const AMOUNT = 1000n
    const FEE = 10n
    const RECIPIENT = "near:alice.near"
    const MESSAGE = "ref=hypercore"
    const CORE_NONCE = 7n
    const ORIGIN_NONCE = 1n
    const PAUSED_INIT_TRANSFER = 1
    let token: HyperliquedBridgeToken
    let tokenAddress: string

    beforeEach(async () => {
      ;({ token, address: tokenAddress } = await deployHlToken())
      // Seed the standing pool at _systemAddress while we're still the owner.
      await token.connect(adminAccount)["mint(address,uint256)"](SYSTEM_ADDRESS, AMOUNT)
      // The bridge must own the token to burn from it at submission time.
      await token.transferOwnership(omniBridgeAddress)
      await omniBridge.acceptTokenOwnership(tokenAddress)
      await registerHlOnBridge(tokenAddress)
    })

    function encodeData(fee: bigint = FEE) {
      return ethers.concat([
        "0x01",
        ethers.AbiCoder.defaultAbiCoder().encode(
          ["uint128", "string", "string"],
          [fee, RECIPIENT, MESSAGE],
        ),
      ])
    }

    // Mirrors _initTransferCommitment on the bridge.
    function commitment(
      tokenAddr: string,
      sender: string,
      amount: bigint,
      fee: bigint,
      recipient: string,
      message: string,
    ) {
      return ethers.keccak256(
        ethers.AbiCoder.defaultAbiCoder().encode(
          ["address", "address", "uint128", "uint128", "string", "string"],
          [tokenAddr, sender, amount, fee, recipient, message],
        ),
      )
    }

    function queue(
      coreNonce: bigint = CORE_NONCE,
      amount: bigint = AMOUNT,
      from: string = user1.address,
    ) {
      return token
        .connect(systemSigner)
        .coreReceiveWithData(from, ethers.ZeroHash, 0, amount, coreNonce, encodeData())
    }

    function trigger(
      overrides: Partial<{
        originNonce: bigint
        tokenAddress: string
        sender: string
        amount: bigint
        fee: bigint
        recipient: string
        message: string
        value: bigint
      }> = {},
    ) {
      const a = {
        originNonce: ORIGIN_NONCE,
        tokenAddress,
        sender: user1.address,
        amount: AMOUNT,
        fee: FEE,
        recipient: RECIPIENT,
        message: MESSAGE,
        value: WORMHOLE_FEE,
        ...overrides,
      }
      // Permissionless on purpose: any third party may submit a stuck transfer.
      return omniBridge
        .connect(user2)
        .triggerPendingInitTransfer(
          a.originNonce,
          a.tokenAddress,
          a.sender,
          a.amount,
          a.fee,
          a.recipient,
          a.message,
          { value: a.value },
        )
    }

    it("commits on the bridge instead of publishing inline", async () => {
      const data = encodeData()
      const tx = token
        .connect(systemSigner)
        .coreReceiveWithData(user1.address, ethers.ZeroHash, 0, AMOUNT, CORE_NONCE, data)

      await expect(tx)
        .to.emit(omniBridge, "PreInitTransfer")
        .withArgs(
          ORIGIN_NONCE,
          tokenAddress,
          user1.address,
          CORE_NONCE,
          AMOUNT,
          FEE,
          RECIPIENT,
          MESSAGE,
        )

      await expect(tx)
        .to.emit(token, "CoreReceived")
        .withArgs(user1.address, ACTION_INIT_TRANSFER, CORE_NONCE, AMOUNT, data)

      // Nothing observable by bloom-filtered watchers happens in this tx.
      await expect(tx).to.not.emit(omniBridge, "InitTransfer")
      await expect(tx).to.not.emit(testWormhole, "MessagePublished")

      expect(await omniBridge.pendingInitTransfers(ORIGIN_NONCE)).to.equal(
        commitment(tokenAddress, user1.address, AMOUNT, FEE, RECIPIENT, MESSAGE),
      )
      // The cursor a relayer polls instead of relying on logs.
      expect(await omniBridge.currentOriginNonce()).to.equal(ORIGIN_NONCE)
      // Tokens are parked on the token contract; nothing burned yet.
      expect(await token.balanceOf(tokenAddress)).to.equal(AMOUNT)
      expect(await token.totalSupply()).to.equal(AMOUNT)
    })

    it("submits the committed transfer, burning and publishing", async () => {
      await queue()

      const tx = trigger()
      await expect(tx)
        .to.emit(omniBridge, "InitTransfer")
        .withArgs(tokenAddress, tokenAddress, ORIGIN_NONCE, AMOUNT, FEE, 0n, RECIPIENT, MESSAGE)
      await expect(tx).to.emit(testWormhole, "MessagePublished")

      expect(await omniBridge.pendingInitTransfers(ORIGIN_NONCE)).to.equal(ethers.ZeroHash)
      expect(await token.balanceOf(tokenAddress)).to.equal(0n)
      expect(await token.totalSupply()).to.equal(0n)
    })

    it("requires the Wormhole message fee", async () => {
      await queue()
      await expect(trigger({ value: 0n })).to.be.revertedWith("invalid fee")
      // Still retryable once the fee is supplied.
      await expect(trigger()).to.emit(testWormhole, "MessagePublished")
    })

    it("cannot be submitted twice", async () => {
      await queue()
      await trigger()

      await expect(trigger())
        .to.be.revertedWithCustomError(omniBridge, "NothingPending")
        .withArgs(ORIGIN_NONCE)
    })

    it("rejects a payload that does not hash to the commitment", async () => {
      await queue()

      for (const bad of [
        { recipient: "near:mallory.near" },
        { amount: AMOUNT + 1n },
        { fee: FEE + 1n },
        { sender: user2.address },
        { message: "tampered" },
      ]) {
        await expect(trigger(bad))
          .to.be.revertedWithCustomError(omniBridge, "PayloadMismatch")
          .withArgs(ORIGIN_NONCE)
      }
    })

    it("reverts when nothing is committed under the nonce", async () => {
      await expect(trigger({ originNonce: 777n }))
        .to.be.revertedWithCustomError(omniBridge, "NothingPending")
        .withArgs(777n)
    })

    it("only a registered bridge token may queue", async () => {
      await expect(
        omniBridge
          .connect(user1)
          .queueInitTransfer(user1.address, CORE_NONCE, AMOUNT, FEE, RECIPIENT, MESSAGE),
      )
        .to.be.revertedWithCustomError(omniBridge, "NotBridgeToken")
        .withArgs(user1.address)
    })

    // coreNonce is sequenced per HyperCore sender, so two senders can present the
    // same value; the bridge's own originNonce keeps the commitments distinct.
    it("assigns a distinct originNonce per delivery", async () => {
      const half = AMOUNT / 2n
      await queue(CORE_NONCE, half, user1.address)
      await queue(CORE_NONCE, half, user2.address)

      expect(await omniBridge.currentOriginNonce()).to.equal(2n)
      expect(await omniBridge.pendingInitTransfers(1n)).to.equal(
        commitment(tokenAddress, user1.address, half, FEE, RECIPIENT, MESSAGE),
      )
      expect(await omniBridge.pendingInitTransfers(2n)).to.equal(
        commitment(tokenAddress, user2.address, half, FEE, RECIPIENT, MESSAGE),
      )

      await trigger({ originNonce: 1n, sender: user1.address, amount: half })
      expect(await omniBridge.pendingInitTransfers(1n)).to.equal(ethers.ZeroHash)
      expect(await omniBridge.pendingInitTransfers(2n)).to.equal(
        commitment(tokenAddress, user2.address, half, FEE, RECIPIENT, MESSAGE),
      )
    })

    it("keeps accepting HyperCore deposits while init-transfer is paused", async () => {
      await omniBridge.pause(PAUSED_INIT_TRANSFER)

      // Queueing must not revert: a revert here strands the tokens on HyperCore,
      // which does not roll back with this transaction.
      await expect(queue()).to.emit(omniBridge, "PreInitTransfer")

      // Submission is what the pause blocks, and it stays retryable.
      await expect(trigger()).to.be.revertedWith("Pausable: paused")
      expect(await omniBridge.pendingInitTransfers(ORIGIN_NONCE)).to.equal(
        commitment(tokenAddress, user1.address, AMOUNT, FEE, RECIPIENT, MESSAGE),
      )

      await omniBridge.pause(0)
      await expect(trigger()).to.emit(testWormhole, "MessagePublished")
      expect(await omniBridge.pendingInitTransfers(ORIGIN_NONCE)).to.equal(ethers.ZeroHash)
    })

    it("reverts when amount overflows uint128 (SafeCast)", async () => {
      const tooBig = 2n ** 128n
      await expect(queue(CORE_NONCE, tooBig))
        .to.be.revertedWithCustomError(token, "SafeCastOverflowedUintDowncast")
        .withArgs(128, tooBig)
    })
  })
})
