import type { BridgeToken, OmniBridge, TestBridgeToken } from "../typechain-types"

import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import { expect } from "chai"
import { ethers, upgrades } from "hardhat"
import { depositSignature, metadataSignature, testWallet } from "./helpers/signatures"

const PauseMode = {
  UnpausedAll: 0,
  PausedInitTransfer: 1 << 0,
  PausedFinTransfer: 1 << 1,
}
const PauseAll = PauseMode.PausedInitTransfer | PauseMode.PausedFinTransfer
const PanicCodeArithmeticOperationOverflowed = "0x11"

describe("BridgeToken", () => {
  const wrappedNearId = "wrap.testnet"

  let OmniBridgeInstance: BridgeToken
  let OmniBridge: OmniBridge
  let adminAccount: HardhatEthersSigner
  let user1: HardhatEthersSigner
  let user2: HardhatEthersSigner

  beforeEach(async () => {
    ;[adminAccount] = await ethers.getSigners()
    user1 = await ethers.getImpersonatedSigner("0x3A445243376C32fAba679F63586e236F77EA601e")
    user2 = await ethers.getImpersonatedSigner("0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265")

    await fundAddress(user1.address, "1")
    await fundAddress(user2.address, "1")

    const BridgeToken_factory = await ethers.getContractFactory("BridgeToken")
    const bridgeToken = await BridgeToken_factory.deploy()
    OmniBridgeInstance = await bridgeToken.waitForDeployment()

    // Use our test wallet's address as the bridge authority
    const nearBridgeDeriveAddress = testWallet.address
    //console.log("nearBridgeDeriveAddress:", nearBridgeDeriveAddress)
    const omniBridgeChainId = 0

    const OmniBridge_factory = await ethers.getContractFactory("OmniBridge")
    const upgradedContract = await upgrades.deployProxy(
      OmniBridge_factory,
      [await bridgeToken.getAddress(), nearBridgeDeriveAddress, omniBridgeChainId],
      { initializer: "initialize" },
    )
    OmniBridge = (await upgradedContract.waitForDeployment()) as unknown as OmniBridge
  })

  async function fundAddress(address: string, amount: string) {
    const tx = await adminAccount.sendTransaction({
      to: address,
      value: ethers.parseEther(amount),
    })
    await tx.wait()
  }

  async function createToken(tokenId: string) {
    const { signature, payload } = await metadataSignature(tokenId)
    await OmniBridge.deployToken(signature, payload)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(tokenId)
    const token = OmniBridgeInstance.attach(tokenProxyAddress) as BridgeToken
    return { token, tokenProxyAddress }
  }

  it("can create a token", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)
    const token = OmniBridgeInstance.attach(tokenProxyAddress) as BridgeToken
    expect(await token.name()).to.be.equal("Wrapped NEAR fungible token")
    expect(await token.symbol()).to.be.equal("wNEAR")
    expect((await token.decimals()).toString()).to.be.equal("18")
  })

  it("can't create token if token already exists", async () => {
    await createToken(wrappedNearId)
    await expect(createToken(wrappedNearId)).to.be.revertedWith("ERR_TOKEN_EXIST")
  })

  it("can update token's metadata", async () => {
    const { token } = await createToken(wrappedNearId)

    await OmniBridge.setMetadata(wrappedNearId, "Circle USDC Bridged", "USDC.E")
    expect(await token.name()).to.equal("Circle USDC Bridged")
    expect(await token.symbol()).to.equal("USDC.E")
  })

  it("can't update metadata of non-existent token", async () => {
    await createToken(wrappedNearId)

    await expect(OmniBridge.setMetadata("non-existing", "Circle USDC", "USDC")).to.be.revertedWith(
      "ERR_NOT_BRIDGE_TOKEN",
    )
  })

  it("can't update metadata as a normal user", async () => {
    await createToken(wrappedNearId)

    await expect(
      OmniBridge.connect(user1).setMetadata(wrappedNearId, "Circle USDC", "USDC"),
    ).to.be.revertedWithCustomError(OmniBridge, "AccessControlUnauthorizedAccount")
  })

  it("can fin transfer", async () => {
    const { token } = await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = await depositSignature(tokenProxyAddress, user1.address)

    await expect(OmniBridge.finTransfer(signature, payload))
      .to.emit(OmniBridge, "FinTransfer")
      .withArgs(
        payload.destinationNonce,
        payload.originChain,
        tokenProxyAddress,
        1,
        payload.recipient,
        payload.feeRecipient,
      )

    expect((await token.balanceOf(payload.recipient)).toString()).to.be.equal(
      payload.amount.toString(),
    )
  })

  it("can't fin transfer if the contract is paused", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    await expect(OmniBridge.pause(PauseMode.PausedFinTransfer))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer)

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)

    await expect(OmniBridge.finTransfer(signature, payload)).to.be.revertedWith("Pausable: paused")
  })

  it("can't fin transfer twice with the same signature", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)
    await OmniBridge.finTransfer(signature, payload)

    await expect(OmniBridge.finTransfer(signature, payload)).to.be.revertedWithCustomError(
      OmniBridge,
      "NonceAlreadyUsed",
    )
  })

  it("can't fin transfer with invalid amount", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)
    payload.amount = 100000

    await expect(OmniBridge.finTransfer(signature, payload)).to.be.revertedWithCustomError(
      OmniBridge,
      "InvalidSignature",
    )
  })

  it("can't fin transfer with invalid nonce", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)
    payload.destinationNonce = 99

    await expect(OmniBridge.finTransfer(signature, payload)).to.be.revertedWithCustomError(
      OmniBridge,
      "InvalidSignature",
    )
  })

  it("can't fin transfer with invalid token", async () => {
    await createToken(wrappedNearId)
    const wrappedNearTokenAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = depositSignature(wrappedNearTokenAddress, user1.address)
    const tokenProxyAddress = await OmniBridge.nearToEthToken("test-token.testnet")
    payload.tokenAddress = tokenProxyAddress

    await expect(OmniBridge.finTransfer(signature, payload)).to.be.revertedWithCustomError(
      OmniBridge,
      "InvalidSignature",
    )
  })

  it("can't fin transfer with invalid recipient", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)
    payload.recipient = user2.address

    await expect(OmniBridge.finTransfer(signature, payload)).to.be.revertedWithCustomError(
      OmniBridge,
      "InvalidSignature",
    )
  })

  it("can't fin transfer with invalid relayer", async () => {
    await createToken(wrappedNearId)
    const wrappedNearTokenAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = depositSignature(wrappedNearTokenAddress, user1.address)
    payload.feeRecipient = "testrecipient.near"

    await expect(OmniBridge.finTransfer(signature, payload)).to.be.revertedWithCustomError(
      OmniBridge,
      "InvalidSignature",
    )
  })

  it("can init transfer", async () => {
    const { token } = await createToken(wrappedNearId)
    const tokenProxyAddress = await token.getAddress()

    const { signature, payload } = await depositSignature(tokenProxyAddress, user1.address)
    await OmniBridge.finTransfer(signature, payload)

    const recipient = "testrecipient.near"
    const fee = 0
    const nativeFee = 0

    await expect(
      OmniBridge.connect(user1).initTransfer(
        tokenProxyAddress,
        payload.amount,
        fee,
        nativeFee,
        recipient,
        "",
      ),
    )
      .to.emit(OmniBridge, "InitTransfer")
      .withArgs(user1.address, tokenProxyAddress, 1, payload.amount, fee, nativeFee, recipient, "")

    expect((await token.balanceOf(user1.address)).toString()).to.be.equal("0")
  })

  it("can't init transfer token when paused", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)
    await OmniBridge.finTransfer(signature, payload)

    const fee = 0
    const nativeFee = 100
    const message = ""
    await expect(OmniBridge.pause(PauseMode.PausedInitTransfer))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.PausedInitTransfer)
    await expect(
      OmniBridge.initTransfer(
        tokenProxyAddress,
        payload.amount,
        fee,
        nativeFee,
        "testrecipient.near",
        message,
        {
          value: 100,
        },
      ),
    ).to.be.revertedWith("Pausable: paused")
  })

  it("can't init transfer when value is too low", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)
    await OmniBridge.finTransfer(signature, payload)

    const fee = 0
    const nativeFee = 100
    const message = ""

    await expect(
      OmniBridge.initTransfer(
        tokenProxyAddress,
        payload.amount,
        fee,
        nativeFee,
        "testrecipient.near",
        message,
      ),
    ).to.be.revertedWithPanic(PanicCodeArithmeticOperationOverflowed)
  })

  it("can't init transfer when value is too high", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)
    await OmniBridge.finTransfer(signature, payload)

    const fee = 0
    const nativeFee = 100
    const message = ""

    await expect(
      OmniBridge.connect(user1).initTransfer(
        tokenProxyAddress,
        payload.amount,
        fee,
        nativeFee,
        "testrecipient.near",
        message,
        {
          value: 200,
        },
      ),
    ).to.be.revertedWithCustomError(OmniBridge, "InvalidValue")
  })

  it("allows admin to rescue accidentally received ETH", async () => {
    const bridgeAddress = await OmniBridge.getAddress()
    const recipient = user2.address
    const amount = ethers.parseEther("0.1")

    await adminAccount.sendTransaction({
      to: bridgeAddress,
      value: amount,
    })

    const bridgeBalanceBefore = await ethers.provider.getBalance(bridgeAddress)
    const recipientBalanceBefore = await ethers.provider.getBalance(recipient)
    expect(bridgeBalanceBefore).to.be.equal(amount)

    const tx = await OmniBridge.connect(adminAccount).rescueEther(recipient, amount)
    const receipt = await tx.wait()
    expect(receipt).to.not.be.undefined

    const bridgeBalanceAfter = await ethers.provider.getBalance(bridgeAddress)
    const recipientBalanceAfter = await ethers.provider.getBalance(recipient)

    expect(bridgeBalanceAfter).to.be.equal(0n)
    expect(recipientBalanceAfter - recipientBalanceBefore).to.be.equal(amount)
  })

  it("rejects ETH rescue from non-admin accounts", async () => {
    const bridgeAddress = await OmniBridge.getAddress()
    const amount = ethers.parseEther("0.1")

    await adminAccount.sendTransaction({
      to: bridgeAddress,
      value: amount,
    })

    await expect(OmniBridge.connect(user1).rescueEther(user1.address, amount)).to.be.revertedWithCustomError(
      OmniBridge,
      "AccessControlUnauthorizedAccount",
    )
  })

  it("can fin and init transfer after unpausing", async () => {
    const { token } = await createToken(wrappedNearId)
    const tokenProxyAddress = await token.getAddress()

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)
    await OmniBridge.finTransfer(signature, payload)

    await expect(OmniBridge.pause(PauseMode.PausedInitTransfer))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.PausedInitTransfer)

    await expect(OmniBridge.pause(PauseMode.UnpausedAll))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.UnpausedAll)

    const recipient = "testrecipient.near"
    const fee = 0
    const nativeFee = 0
    const message = ""
    await OmniBridge.connect(user1).initTransfer(
      tokenProxyAddress,
      payload.amount,
      fee,
      nativeFee,
      recipient,
      message,
    )

    expect((await token.balanceOf(user1.address)).toString()).to.be.equal("0")
  })

  it("upgrade token contract", async () => {
    const { tokenProxyAddress } = await createToken(wrappedNearId)

    const BridgeTokenV2Instance = await ethers.getContractFactory("TestBridgeToken")
    const BridgeTokenV2 = await BridgeTokenV2Instance.deploy()
    await BridgeTokenV2.waitForDeployment()

    await OmniBridge.upgradeToken(tokenProxyAddress, await BridgeTokenV2.getAddress())
    const BridgeTokenV2Proxied = BridgeTokenV2Instance.attach(tokenProxyAddress) as TestBridgeToken
    expect(await BridgeTokenV2Proxied.returnTestString()).to.equal("test")
    expect(await BridgeTokenV2Proxied.name()).to.equal("Wrapped NEAR fungible token")
    expect(await BridgeTokenV2Proxied.symbol()).to.equal("wNEAR")
    expect((await BridgeTokenV2Proxied.decimals()).toString()).to.equal("18")
  })

  it("user can't upgrade token contract", async () => {
    await createToken(wrappedNearId)
    const tokenProxyAddress = await OmniBridge.nearToEthToken(wrappedNearId)

    const BridgeTokenV2Instance = await ethers.getContractFactory("TestBridgeToken")
    const BridgeTokenV2 = await BridgeTokenV2Instance.deploy()
    await BridgeTokenV2.waitForDeployment()

    await expect(
      OmniBridge.connect(user1).upgradeToken(tokenProxyAddress, await BridgeTokenV2.getAddress()),
    ).to.be.revertedWithCustomError(OmniBridge, "AccessControlUnauthorizedAccount")
  })

  it("Test selective pause", async () => {
    // Pause withdraw
    await expect(OmniBridge.pause(PauseMode.PausedInitTransfer))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.PausedInitTransfer)
    expect(await OmniBridge.pausedFlags()).to.be.equal(PauseMode.PausedInitTransfer)

    // Pause withdraw again
    await expect(OmniBridge.pause(PauseMode.PausedInitTransfer))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.PausedInitTransfer)
    expect(await OmniBridge.pausedFlags()).to.be.equal(PauseMode.PausedInitTransfer)
    expect(await OmniBridge.paused(PauseMode.PausedFinTransfer)).to.be.equal(false)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.equal(true)

    // Pause deposit
    await expect(OmniBridge.pause(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer)
    expect(await OmniBridge.pausedFlags()).to.be.equal(
      PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer,
    )

    // Pause deposit again
    await expect(OmniBridge.pause(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer)
    expect(await OmniBridge.pausedFlags()).to.be.equal(
      PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer,
    )

    // Pause deposit and withdraw
    await expect(OmniBridge.pause(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer)
    expect(await OmniBridge.pausedFlags()).to.be.equal(
      PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer,
    )
    expect(await OmniBridge.paused(PauseMode.PausedFinTransfer)).to.be.equal(true)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.equal(true)

    // Unpause all
    await expect(OmniBridge.pause(PauseMode.UnpausedAll))
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.UnpausedAll)
    expect(await OmniBridge.pausedFlags()).to.be.equal(PauseMode.UnpausedAll)

    // Pause all
    await expect(OmniBridge.pauseAll())
      .to.emit(OmniBridge, "Paused")
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer)
    expect(await OmniBridge.pausedFlags()).to.be.equal(
      PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer,
    )
    expect(await OmniBridge.paused(PauseMode.PausedFinTransfer)).to.be.equal(true)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.equal(true)
  })

  it("Test grant admin role", async () => {
    await OmniBridge.connect(adminAccount).pause(PauseMode.UnpausedAll)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.false

    await OmniBridge.connect(adminAccount).pauseAll()
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.true

    const signers = await ethers.getSigners()
    const newAdminAccount = signers[2]
    const DEFAULT_ADMIN_ROLE = "0x0000000000000000000000000000000000000000000000000000000000000000"
    await expect(
      OmniBridge.connect(newAdminAccount).pause(PauseMode.UnpausedAll),
    ).to.be.revertedWithCustomError(OmniBridge, "AccessControlUnauthorizedAccount")
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.true

    // Grant DEFAULT_ADMIN_ROLE to newAdminAccount
    await expect(OmniBridge.grantRole(DEFAULT_ADMIN_ROLE, newAdminAccount.address))
      .to.emit(OmniBridge, "RoleGranted")
      .withArgs(DEFAULT_ADMIN_ROLE, newAdminAccount.address, adminAccount.address)
    await OmniBridge.connect(newAdminAccount).pause(PauseMode.UnpausedAll)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.false

    await OmniBridge.connect(newAdminAccount).pause(PauseAll)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.true

    // Revoke DEFAULT_ADMIN_ROLE from adminAccount
    await expect(
      OmniBridge.connect(newAdminAccount).revokeRole(DEFAULT_ADMIN_ROLE, adminAccount.address),
    )
      .to.emit(OmniBridge, "RoleRevoked")
      .withArgs(DEFAULT_ADMIN_ROLE, adminAccount.address, newAdminAccount.address)

    // Check tx reverted on call from revoked adminAccount
    await expect(
      OmniBridge.connect(adminAccount).pause(PauseMode.UnpausedAll),
    ).to.be.revertedWithCustomError(OmniBridge, "AccessControlUnauthorizedAccount")
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.true

    // Check newAdminAccount can perform admin calls
    await OmniBridge.connect(newAdminAccount).pause(PauseMode.UnpausedAll)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.false
    await OmniBridge.connect(newAdminAccount).pause(PauseAll)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.true

    // Check newAdminAccount can grant DEFAULT_ADMIN_ROLE to adminAccount
    await expect(
      OmniBridge.connect(newAdminAccount).grantRole(DEFAULT_ADMIN_ROLE, adminAccount.address),
    )
      .to.emit(OmniBridge, "RoleGranted")
      .withArgs(DEFAULT_ADMIN_ROLE, adminAccount.address, newAdminAccount.address)

    // Check that adminAccount can perform admin calls again
    await OmniBridge.connect(adminAccount).pause(PauseMode.UnpausedAll)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.false
    await OmniBridge.connect(adminAccount).pause(PauseAll)
    expect(await OmniBridge.paused(PauseMode.PausedInitTransfer)).to.be.true
  })
})
