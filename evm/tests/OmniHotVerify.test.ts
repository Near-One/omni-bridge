import type { BridgeToken, OmniHotVerify } from "../typechain-types"

import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import { expect } from "chai"
import { ethers, upgrades } from "hardhat"
import { depositSignature, metadataSignature, testWallet } from "./helpers/signatures"

describe("OmniHotVerify", () => {
  const wrappedNearId = "wrap.testnet"

  let OmniHotVerifyInstance: OmniHotVerify
  let BridgeTokenInstance: BridgeToken
  let adminAccount: HardhatEthersSigner
  let user1: HardhatEthersSigner

  beforeEach(async () => {
    ;[adminAccount] = await ethers.getSigners()
    user1 = await ethers.getImpersonatedSigner("0x3A445243376C32fAba679F63586e236F77EA601e")
    await fundAddress(user1.address, "1")

    const bridgeTokenFactory = await ethers.getContractFactory("BridgeToken")
    BridgeTokenInstance = await bridgeTokenFactory.deploy()
    await BridgeTokenInstance.waitForDeployment()

    const nearBridgeDeriveAddress = testWallet.address
    const omniBridgeChainId = 0

    const OmniHotVerifyFactory = await ethers.getContractFactory("OmniHotVerify")
    const deployed = await upgrades.deployProxy(
      OmniHotVerifyFactory,
      [await BridgeTokenInstance.getAddress(), nearBridgeDeriveAddress, omniBridgeChainId],
      { initializer: "initialize" },
    )
    OmniHotVerifyInstance = (await deployed.waitForDeployment()) as OmniHotVerify
  })

  async function fundAddress(address: string, amount: string) {
    const tx = await adminAccount.sendTransaction({
      to: address,
      value: ethers.parseEther(amount),
    })
    await tx.wait()
  }

  async function createToken(
    tokenId: string,
  ): Promise<{ tokenProxyAddress: string; token: BridgeToken }> {
    const { signature, payload } = metadataSignature(tokenId)
    await OmniHotVerifyInstance.deployToken(signature, payload)
    const tokenProxyAddress = await OmniHotVerifyInstance.nearToEthToken(tokenId)
    const token = BridgeTokenInstance.attach(tokenProxyAddress) as BridgeToken
    return { tokenProxyAddress, token }
  }

  it("records initiatedTransfers hash and validates hotVerify", async () => {
    const { token } = await createToken(wrappedNearId)
    const tokenProxyAddress = await token.getAddress()

    const { signature, payload } = depositSignature(tokenProxyAddress, user1.address)
    await OmniHotVerifyInstance.finTransfer(signature, payload)

    const recipient = "testrecipient.near"
    const fee = 0n
    const nativeFee = 0n
    const message = "hot-verify"

    await OmniHotVerifyInstance.connect(user1).initTransfer(
      tokenProxyAddress,
      payload.amount,
      fee,
      nativeFee,
      recipient,
      message,
    )

    const originNonce = await OmniHotVerifyInstance.currentOriginNonce()
    const stored = await OmniHotVerifyInstance.initiatedTransfers(originNonce)
    const chainId = (await ethers.provider.getNetwork()).chainId
    const amount = BigInt(payload.amount)
    const expectedHash = ethers.keccak256(
      ethers.AbiCoder.defaultAbiCoder().encode(
        [
          "uint256",
          "address",
          "address",
          "address",
          "uint64",
          "uint128",
          "uint128",
          "uint128",
          "string",
          "string",
        ],
        [
          chainId,
          await OmniHotVerifyInstance.getAddress(),
          user1.address,
          tokenProxyAddress,
          originNonce,
          amount,
          fee,
          nativeFee,
          recipient,
          message,
        ],
      ),
    )

    expect(stored).to.equal(expectedHash)

    const userPayload = ethers.AbiCoder.defaultAbiCoder().encode(["uint64"], [originNonce])
    expect(await OmniHotVerifyInstance.hotVerify(expectedHash, "0x", userPayload, "0x")).to.equal(
      true,
    )
    expect(await OmniHotVerifyInstance.hotVerify(ethers.ZeroHash, "0x", userPayload, "0x")).to.equal(
      false,
    )

    const wrongNoncePayload = ethers.AbiCoder.defaultAbiCoder().encode(
      ["uint64"],
      [originNonce + 1n],
    )
    expect(
      await OmniHotVerifyInstance.hotVerify(expectedHash, "0x", wrongNoncePayload, "0x"),
    ).to.equal(false)
  })
})
