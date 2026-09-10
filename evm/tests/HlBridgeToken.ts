import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import { expect } from "chai"
import { ethers, upgrades } from "hardhat"
import type { HyperliquedBridgeToken, OmniBridge } from "../typechain-types"
import { testWallet } from "./helpers/signatures"

const ACTION_TRANSFER = 0
const ACTION_INIT_TRANSFER = 1

describe("HyperliquedBridgeToken", () => {
  let adminAccount: HardhatEthersSigner
  let user1: HardhatEthersSigner
  let user2: HardhatEthersSigner
  let systemSigner: HardhatEthersSigner

  let omniBridge: OmniBridge
  let omniBridgeAddress: string

  const SYSTEM_ADDRESS = "0x2222000000000000000000000000000000000000"
  const NEAR_TOKEN_ID = "hl.testnet"

  beforeEach(async () => {
    ;[adminAccount, user1, user2] = await ethers.getSigners()
    systemSigner = await ethers.getImpersonatedSigner(SYSTEM_ADDRESS)
    await adminAccount.sendTransaction({
      to: SYSTEM_ADDRESS,
      value: ethers.parseEther("1"),
    })

    // Deploy OmniBridge with a generic BridgeToken impl — we register an
    // externally-deployed HlBridgeToken via addCustomToken, so the implementation
    // address here is unused for our flows.
    const BridgeToken_factory = await ethers.getContractFactory("BridgeToken")
    const bridgeTokenImpl = await BridgeToken_factory.deploy()
    await bridgeTokenImpl.waitForDeployment()

    const OmniBridge_factory = await ethers.getContractFactory("OmniBridge")
    const omniBridgeProxy = await upgrades.deployProxy(
      OmniBridge_factory,
      [await bridgeTokenImpl.getAddress(), testWallet.address, 0],
      { initializer: "initialize" },
    )
    omniBridge = (await omniBridgeProxy.waitForDeployment()) as unknown as OmniBridge
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

  // `addCustomToken` with `customMinter = address(0)` registers the token so that
  // `OmniBridge.initTransfer` falls into the `isBridgeToken` branch and calls
  // `BridgeToken.burn(msg.sender, amount)` — exactly the path we want.
  async function registerHlOnBridge(tokenAddress: string) {
    await omniBridge.addCustomToken(NEAR_TOKEN_ID, tokenAddress, ethers.ZeroAddress, 18)
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
      // Seed the system-address pool the way a prior 3-arg mint would.
      await token.connect(adminAccount)["mint(address,uint256)"](SYSTEM_ADDRESS, AMOUNT)
    })

    it("releases tokens from the system-address pool to recipient", async () => {
      const data = ethers.concat([
        "0x00",
        ethers.AbiCoder.defaultAbiCoder().encode(["address"], [user2.address]),
      ])

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
      const data = ethers.concat([
        "0x00",
        ethers.AbiCoder.defaultAbiCoder().encode(["address"], [ethers.ZeroAddress]),
      ])
      await expect(
        token
          .connect(systemSigner)
          .coreReceiveWithData(user1.address, ethers.ZeroHash, 0, AMOUNT, 0, data),
      ).to.be.revertedWithCustomError(token, "InvalidRecipient")
      expect(await token.totalSupply()).to.equal(AMOUNT)
    })

    it("reverts if the system-address pool is insufficient", async () => {
      const data = ethers.concat([
        "0x00",
        ethers.AbiCoder.defaultAbiCoder().encode(["address"], [user2.address]),
      ])
      await expect(
        token
          .connect(systemSigner)
          .coreReceiveWithData(user1.address, ethers.ZeroHash, 0, AMOUNT + 1n, 0, data),
      ).to.be.revertedWithCustomError(token, "ERC20InsufficientBalance")
    })
  })

  describe("ACTION_INIT_TRANSFER (0x01) via real OmniBridge", () => {
    const AMOUNT = 1000n
    const FEE = 10n
    const RECIPIENT = "near:alice.near"
    const MESSAGE = "ref=hypercore"
    const CORE_NONCE = 7n
    const PAUSED_INIT_TRANSFER = 1
    let token: HyperliquedBridgeToken
    let tokenAddress: string

    beforeEach(async () => {
      ;({ token, address: tokenAddress } = await deployHlToken())
      // Seed the standing pool at _systemAddress while we're still the owner.
      await token.connect(adminAccount)["mint(address,uint256)"](SYSTEM_ADDRESS, AMOUNT)
      // Hand ownership to OmniBridge so it can burn from the token contract once
      // we've moved the bridged amount from the pool to address(this) inside
      // coreReceiveWithData.
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

    // Mirrors _initTransferCommitment in the contract.
    function commitment(
      sender: string,
      coreNonce: bigint,
      amount: bigint,
      fee: bigint,
      recipient: string,
      message: string,
    ) {
      return ethers.keccak256(
        ethers.AbiCoder.defaultAbiCoder().encode(
          ["address", "uint64", "uint128", "uint128", "string", "string"],
          [sender, coreNonce, amount, fee, recipient, message],
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
        id: bigint
        sender: string
        coreNonce: bigint
        amount: bigint
        fee: bigint
        recipient: string
        message: string
      }> = {},
    ) {
      const a = {
        id: 0n,
        sender: user1.address,
        coreNonce: CORE_NONCE,
        amount: AMOUNT,
        fee: FEE,
        recipient: RECIPIENT,
        message: MESSAGE,
        ...overrides,
      }
      // Permissionless on purpose: any third party may submit a stuck transfer.
      return token
        .connect(user2)
        .triggerPendingInitTransfer(
          a.id,
          a.sender,
          a.coreNonce,
          a.amount,
          a.fee,
          a.recipient,
          a.message,
        )
    }

    it("commits the transfer instead of bridging inline", async () => {
      const data = encodeData()
      const tx = token
        .connect(systemSigner)
        .coreReceiveWithData(user1.address, ethers.ZeroHash, 0, AMOUNT, CORE_NONCE, data)

      await expect(tx)
        .to.emit(token, "PreInitTransfer")
        .withArgs(0n, user1.address, CORE_NONCE, AMOUNT, FEE, RECIPIENT, MESSAGE)

      await expect(tx)
        .to.emit(token, "CoreReceived")
        .withArgs(user1.address, ACTION_INIT_TRANSFER, CORE_NONCE, AMOUNT, data)

      // Nothing reaches the bridge in this tx — that is the whole point of the split,
      // since these logs are invisible to bloom-filtered watchers.
      await expect(tx).to.not.emit(omniBridge, "InitTransfer")

      expect(await token.pendingInitTransfers(0n)).to.equal(
        commitment(user1.address, CORE_NONCE, AMOUNT, FEE, RECIPIENT, MESSAGE),
      )
      // The cursor a relayer polls instead of relying on logs.
      expect(await token.nextPendingInitTransferId()).to.equal(1n)
      // Tokens are parked on the token contract until the second step burns them.
      expect(await token.balanceOf(tokenAddress)).to.equal(AMOUNT)
      expect(await token.balanceOf(SYSTEM_ADDRESS)).to.equal(0n)
    })

    it("submits the committed transfer and clears the commitment", async () => {
      await queue()

      await expect(trigger())
        .to.emit(omniBridge, "InitTransfer")
        .withArgs(tokenAddress, tokenAddress, 1n, AMOUNT, FEE, 0n, RECIPIENT, MESSAGE)

      expect(await token.pendingInitTransfers(0n)).to.equal(ethers.ZeroHash)
      expect(await token.balanceOf(tokenAddress)).to.equal(0n)
      expect(await token.totalSupply()).to.equal(0n)
    })

    it("cannot be submitted twice", async () => {
      await queue()
      await trigger()

      await expect(trigger())
        .to.be.revertedWithCustomError(token, "PendingInitTransferNotFound")
        .withArgs(0n)
    })

    it("rejects a payload that does not hash to the commitment", async () => {
      await queue()

      for (const bad of [
        { recipient: "near:mallory.near" },
        { amount: AMOUNT + 1n },
        { fee: FEE + 1n },
        { sender: user2.address },
        { coreNonce: CORE_NONCE + 1n },
        { message: "tampered" },
      ]) {
        await expect(trigger(bad))
          .to.be.revertedWithCustomError(token, "PayloadMismatch")
          .withArgs(0n)
      }
    })

    it("reverts when nothing is committed under the id", async () => {
      await expect(trigger({ id: 777n }))
        .to.be.revertedWithCustomError(token, "PendingInitTransferNotFound")
        .withArgs(777n)
    })

    // coreNonce is sequenced per HyperCore sender, so two senders can present the
    // same value; ids must stay distinct regardless.
    it("assigns a fresh id per delivery, including a repeated sender/nonce", async () => {
      const half = AMOUNT / 2n
      await queue(CORE_NONCE, half, user1.address)
      await queue(CORE_NONCE, half, user2.address)

      expect(await token.nextPendingInitTransferId()).to.equal(2n)
      expect(await token.pendingInitTransfers(0n)).to.equal(
        commitment(user1.address, CORE_NONCE, half, FEE, RECIPIENT, MESSAGE),
      )
      expect(await token.pendingInitTransfers(1n)).to.equal(
        commitment(user2.address, CORE_NONCE, half, FEE, RECIPIENT, MESSAGE),
      )

      // Submitting one must leave the other untouched.
      await trigger({ id: 0n, sender: user1.address, amount: half })
      expect(await token.pendingInitTransfers(0n)).to.equal(ethers.ZeroHash)
      expect(await token.pendingInitTransfers(1n)).to.equal(
        commitment(user2.address, CORE_NONCE, half, FEE, RECIPIENT, MESSAGE),
      )
    })

    it("keeps the commitment retryable when the bridge call reverts", async () => {
      await queue()
      await omniBridge.pause(PAUSED_INIT_TRANSFER)

      await expect(trigger()).to.be.reverted

      // The delete rolled back together with the failed call, so a transient bridge
      // failure does not burn the transfer.
      expect(await token.pendingInitTransfers(0n)).to.equal(
        commitment(user1.address, CORE_NONCE, AMOUNT, FEE, RECIPIENT, MESSAGE),
      )

      await omniBridge.pause(0)
      await expect(trigger()).to.emit(omniBridge, "InitTransfer")
      expect(await token.pendingInitTransfers(0n)).to.equal(ethers.ZeroHash)
    })

    it("reverts when amount overflows uint128 (SafeCast)", async () => {
      const tooBig = 2n ** 128n
      await expect(queue(CORE_NONCE, tooBig))
        .to.be.revertedWithCustomError(token, "SafeCastOverflowedUintDowncast")
        .withArgs(128, tooBig)
    })
  })
})
