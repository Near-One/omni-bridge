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
      ["Wrapped HL", "wHL", 18, SYSTEM_ADDRESS],
      { initializer: "initialize(string,string,uint8,address)", kind: "uups" },
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
      // Simulate the system address pre-transferring tokens to address(this).
      await token.connect(adminAccount)["mint(address,uint256)"](tokenAddress, AMOUNT)
    })

    it("forwards tokens to recipient encoded in data", async () => {
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
        .withArgs(user1.address, ACTION_TRANSFER, AMOUNT, data)

      expect(await token.balanceOf(user2.address)).to.equal(AMOUNT)
      expect(await token.balanceOf(tokenAddress)).to.equal(0n)
    })

    it("reverts if the contract balance is insufficient", async () => {
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
    let token: HyperliquedBridgeToken
    let tokenAddress: string

    beforeEach(async () => {
      ;({ token, address: tokenAddress } = await deployHlToken())
      // Mint to address(this) while we're still the owner.
      await token.connect(adminAccount)["mint(address,uint256)"](tokenAddress, AMOUNT)
      // Hand ownership to OmniBridge so it can burn from the token contract.
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

    it("emits InitTransfer on the real OmniBridge and burns the bridged amount", async () => {
      const data = encodeData()
      const tx = token
        .connect(systemSigner)
        .coreReceiveWithData(user1.address, ethers.ZeroHash, 0, AMOUNT, 0, data)

      await expect(tx)
        .to.emit(omniBridge, "InitTransfer")
        .withArgs(tokenAddress, tokenAddress, 1n, AMOUNT, FEE, 0n, RECIPIENT, MESSAGE)

      await expect(tx)
        .to.emit(token, "CoreReceived")
        .withArgs(user1.address, ACTION_INIT_TRANSFER, AMOUNT, data)

      expect(await token.balanceOf(tokenAddress)).to.equal(0n)
      expect(await token.totalSupply()).to.equal(0n)
    })

    it("reverts when amount overflows uint128 (SafeCast)", async () => {
      const tooBig = 2n ** 128n
      const data = encodeData()
      await expect(
        token
          .connect(systemSigner)
          .coreReceiveWithData(user1.address, ethers.ZeroHash, 0, tooBig, 0, data),
      )
        .to.be.revertedWithCustomError(token, "SafeCastOverflowedUintDowncast")
        .withArgs(128, tooBig)
    })
  })
})
