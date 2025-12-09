import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import { expect } from "chai"
import type { BigNumberish } from "ethers"
import { ethers, upgrades } from "hardhat"
import type { BridgeToken, OmniBridge, TestERC1155 } from "../typechain-types"
import { depositSignature, testWallet } from "./helpers/signatures"

type OmniBridge1155 = OmniBridge & {
  exposedGetOrCreateDeterministicAddress(
    tokenAddress: string,
    tokenId: BigNumberish,
  ): Promise<string>
  forceSetMultiToken(
    deterministic: string,
    tokenAddress: string,
    tokenId: BigNumberish,
  ): Promise<void>
}

describe("OmniBridge ERC1155", () => {
  const tokenId = 7n
  const secondaryTokenId = 8n
  const mintedAmount = 5n

  let admin: HardhatEthersSigner
  let user: HardhatEthersSigner
  let recipient: HardhatEthersSigner
  let bridgeTokenImpl: BridgeToken
  let bridge: OmniBridge1155
  let erc1155: TestERC1155

  beforeEach(async () => {
    ;[admin, user, recipient] = await ethers.getSigners()

    const bridgeTokenFactory = await ethers.getContractFactory("BridgeToken")
    bridgeTokenImpl = (await bridgeTokenFactory.deploy()) as BridgeToken
    await bridgeTokenImpl.waitForDeployment()

    const bridgeFactory = await ethers.getContractFactory("OmniBridge1155Harness")
    const deployedBridge = await upgrades.deployProxy(
      bridgeFactory,
      [await bridgeTokenImpl.getAddress(), testWallet.address, 0],
      { initializer: "initialize" },
    )
    bridge = (await deployedBridge.waitForDeployment()) as unknown as OmniBridge1155

    const erc1155Factory = await ethers.getContractFactory("TestERC1155")
    erc1155 = await erc1155Factory.deploy()
    await erc1155.waitForDeployment()

    await erc1155.mint(await user.getAddress(), tokenId, mintedAmount)
    await erc1155.mint(await user.getAddress(), secondaryTokenId, 2)
    await erc1155.connect(user).setApprovalForAll(await bridge.getAddress(), true)
  })

  function manualDeterministicAddress(tokenAddress: string, id: BigNumberish): string {
    const addr = BigInt(tokenAddress)
    const prefix = (addr >> 128n) & ((1n << 32n) - 1n)
    const hash = ethers.solidityPackedKeccak256(["address", "uint256"], [tokenAddress, id])
    const suffix = (BigInt(hash) >> 128n) & ((1n << 128n) - 1n)
    const combined = (prefix << 128n) | suffix
    return ethers.getAddress(`0x${combined.toString(16).padStart(40, "0")}`)
  }

  it("initiates ERC1155 transfer and records mapping", async () => {
    const deterministic = await bridge.deriveDeterministicAddress(
      await erc1155.getAddress(),
      tokenId,
    )
    const amount = 2n
    const fee = 0
    const nativeFee = 0
    const recipientOnNear = "recipient.near"
    const memo = "erc1155-init"

    await expect(
      bridge
        .connect(user)
        .initTransfer1155(
          await erc1155.getAddress(),
          tokenId,
          amount,
          fee,
          nativeFee,
          recipientOnNear,
          memo,
        ),
    )
      .to.emit(bridge, "InitTransfer")
      .withArgs(
        await user.getAddress(),
        deterministic,
        1,
        amount,
        fee,
        nativeFee,
        recipientOnNear,
        memo,
      )

    expect(await erc1155.balanceOf(await bridge.getAddress(), tokenId)).to.equal(amount)
    expect(await erc1155.balanceOf(await user.getAddress(), tokenId)).to.equal(
      mintedAmount - amount,
    )

    const storedMapping = await bridge.multiTokens(deterministic)
    expect(storedMapping.tokenAddress).to.equal(await erc1155.getAddress())
    expect(storedMapping.tokenId).to.equal(tokenId)
  })

  it("finalizes ERC1155 transfer using deterministic address", async () => {
    const deterministic = await bridge.deriveDeterministicAddress(
      await erc1155.getAddress(),
      tokenId,
    )
    await bridge
      .connect(user)
      .initTransfer1155(
        await erc1155.getAddress(),
        tokenId,
        1,
        0,
        0,
        "recipient.near",
        "",
      )

    const { signature, payload } = depositSignature(
      deterministic,
      await recipient.getAddress(),
    )

    await expect(bridge.finTransfer(signature, payload))
      .to.emit(bridge, "FinTransfer")
      .withArgs(
        payload.originChain,
        payload.originNonce,
        deterministic,
        payload.amount,
        payload.recipient,
        payload.feeRecipient,
      )

    expect(await erc1155.balanceOf(await bridge.getAddress(), tokenId)).to.equal(0)
    expect(await erc1155.balanceOf(await recipient.getAddress(), tokenId)).to.equal(1n)
  })

  it("logs metadata for ERC1155 tokens and sets mapping", async () => {
    const tokenAddress = await erc1155.getAddress()
    const deterministic = await bridge.deriveDeterministicAddress(tokenAddress, tokenId)

    await expect(bridge.logMetadata1155(tokenAddress, tokenId))
      .to.emit(bridge, "LogMetadata")
      .withArgs(deterministic, "", "", 0)

    const storedMapping = await bridge.multiTokens(deterministic)
    expect(storedMapping.tokenAddress).to.equal(tokenAddress)
    expect(storedMapping.tokenId).to.equal(tokenId)

    // Calling again should reuse mapping without reverting
    await expect(bridge.logMetadata1155(tokenAddress, tokenId))
      .to.emit(bridge, "LogMetadata")
      .withArgs(deterministic, "", "", 0)
  })

  it("derives deterministic addresses consistently and rejects collisions", async () => {
    const tokenAddress = await erc1155.getAddress()
    const derived = await bridge.deriveDeterministicAddress(tokenAddress, tokenId)
    expect(derived).to.equal(manualDeterministicAddress(tokenAddress, tokenId))

    await bridge.exposedGetOrCreateDeterministicAddress(tokenAddress, tokenId)
    const mapping = await bridge.multiTokens(derived)
    expect(mapping.tokenAddress).to.equal(tokenAddress)
    expect(mapping.tokenId).to.equal(tokenId)

    const fakeToken = ethers.Wallet.createRandom().address
    await bridge.forceSetMultiToken(derived, fakeToken, tokenId + 1n)
    await expect(
      bridge.exposedGetOrCreateDeterministicAddress(tokenAddress, tokenId),
    ).to.be.revertedWithCustomError(bridge, "ERC1155MappingMismatch")
  })

  it("validates ERC1155 receiver hooks", async () => {
    const bridgeAddress = await bridge.getAddress()

    await expect(
      erc1155
        .connect(user)
        .safeTransferFrom(await user.getAddress(), bridgeAddress, tokenId, 1, "0x"),
    ).to.be.revertedWithCustomError(bridge, "ERC1155DirectSendNotAllowed")

    await expect(
      erc1155
        .connect(user)
        .safeBatchTransferFrom(
          await user.getAddress(),
          bridgeAddress,
          [tokenId, secondaryTokenId],
          [1, 1],
          "0x",
        ),
    ).to.be.revertedWithCustomError(bridge, "ERC1155BatchNotSupported")

    await bridge
      .connect(user)
      .initTransfer1155(await erc1155.getAddress(), tokenId, 1, 0, 0, "receiver.near", "")
    expect(await erc1155.balanceOf(bridgeAddress, tokenId)).to.equal(1n)
  })

  it("maintains multiTokens mapping across repeated operations", async () => {
    const tokenAddress = await erc1155.getAddress()
    const deterministic = await bridge.deriveDeterministicAddress(tokenAddress, tokenId)

    await bridge.logMetadata1155(tokenAddress, tokenId)
    await bridge
      .connect(user)
      .initTransfer1155(tokenAddress, tokenId, 1, 0, 0, "repeat.near", "")

    const storedMapping = await bridge.multiTokens(deterministic)
    expect(storedMapping.tokenAddress).to.equal(tokenAddress)
    expect(storedMapping.tokenId).to.equal(tokenId)
  })
})
