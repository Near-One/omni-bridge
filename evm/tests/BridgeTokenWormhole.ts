import { anyValue } from "@nomicfoundation/hardhat-chai-matchers/withArgs"
import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import * as borsh from "borsh"
import { expect } from "chai"
import type { BigNumberish } from "ethers"
import { ethers, upgrades } from "hardhat"
import type { BridgeToken, OmniBridgeWormhole, TestWormhole } from "../typechain-types"
import { depositSignature, metadataSignature, testWallet } from "./helpers/signatures"

const WormholeFee = 10000

class FinTransferWormholeMessage {
  static schema = {
    struct: {
      messageType: "u8",
      originChain: "u8",
      originNonce: "u64",
      omniBridgeChainId: "u8",
      tokenAddress: { array: { type: "u8", len: 20 } },
      amount: "u128",
      feeRecipient: "string",
    },
  }

  constructor(
    public messageType: number,
    public originChain: BigNumberish,
    public originNonce: bigint,
    public omniBridgeChainId: number,
    public tokenAddress: Uint8Array,
    public amount: bigint,
    public feeRecipient: string,
  ) {}

  static serialize(msg: FinTransferWormholeMessage): Uint8Array {
    return borsh.serialize(FinTransferWormholeMessage.schema, msg)
  }
}

class InitTransferWormholeMessage {
  static schema = {
    struct: {
      messageType: "u8",
      originChainId: "u8",
      sender: { array: { type: "u8", len: 20 } },
      destinationChainId: "u8",
      tokenAddress: { array: { type: "u8", len: 20 } },
      originNonce: "u64",
      amount: "u128",
      fee: "u128",
      nativeFee: "u128",
      recipient: "string",
      message: "string",
    },
  }

  constructor(
    public messageType: number,
    public originChainId: number,
    public sender: Uint8Array,
    public destinationChainId: number,
    public tokenAddress: Uint8Array,
    public originNonce: bigint,
    public amount: bigint,
    public fee: bigint,
    public nativeFee: bigint,
    public recipient: string,
    public message: string,
  ) {}

  static serialize(msg: InitTransferWormholeMessage): Uint8Array {
    return borsh.serialize(InitTransferWormholeMessage.schema, msg)
  }
}

describe("BridgeTokenWormhole", () => {
  const wrappedNearId = "wrap.testnet"
  const consistencyLevel = 3

  let user1: HardhatEthersSigner
  let adminAccount: HardhatEthersSigner
  let OmniBridgeInstance: BridgeToken
  let OmniBridgeWormhole: OmniBridgeWormhole
  let TestWormhole: TestWormhole

  beforeEach(async () => {
    ;[adminAccount] = await ethers.getSigners()
    user1 = await ethers.getImpersonatedSigner("0x3A445243376C32fAba679F63586e236F77EA601e")
    await fundAddress(await user1.getAddress(), "1")

    const bridgeToken_factory = await ethers.getContractFactory("BridgeToken")
    OmniBridgeInstance = await bridgeToken_factory.deploy()
    await OmniBridgeInstance.waitForDeployment()

    const testWormhole_factory = await ethers.getContractFactory("TestWormhole")
    TestWormhole = await testWormhole_factory.deploy()
    await TestWormhole.waitForDeployment()

    const nearBridgeDeriveAddress = testWallet.address
    const omniBridgeChainId = 0

    const OmniBridgeWormhole_factory = await ethers.getContractFactory("OmniBridgeWormhole")
    OmniBridgeWormhole = (await upgrades.deployProxy(
      OmniBridgeWormhole_factory,
      [
        await OmniBridgeInstance.getAddress(),
        nearBridgeDeriveAddress,
        omniBridgeChainId,
        await TestWormhole.getAddress(),
        consistencyLevel,
      ],
      { initializer: "initializeWormhole" },
    )) as unknown as OmniBridgeWormhole
    await OmniBridgeWormhole.waitForDeployment()
  })

  async function fundAddress(address: string, amount: string): Promise<void> {
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

    await OmniBridgeWormhole.deployToken(signature, payload, { value: WormholeFee })
    const tokenProxyAddress = await OmniBridgeWormhole.nearToEthToken(tokenId)
    const token = OmniBridgeInstance.attach(tokenProxyAddress) as BridgeToken
    return { tokenProxyAddress, token }
  }

  it("deploy token", async () => {
    const { signature, payload } = metadataSignature(wrappedNearId)

    await expect(await OmniBridgeWormhole.deployToken(signature, payload, { value: WormholeFee }))
      .to.emit(TestWormhole, "MessagePublished")
      .withArgs(0, anyValue, consistencyLevel)
  })

  it("fin transfer", async () => {
    const { token } = await createToken(wrappedNearId)
    const tokenProxyAddress = await token.getAddress()
    const { signature, payload } = depositSignature(tokenProxyAddress, await user1.getAddress())

    // Serialize the payload using borsh
    const messagePayload = FinTransferWormholeMessage.serialize({
      messageType: 1,
      originChain: payload.originChain,
      originNonce: BigInt(payload.originNonce),
      omniBridgeChainId: 0,
      tokenAddress: ethers.getBytes(tokenProxyAddress),
      amount: BigInt(payload.amount),
      feeRecipient: payload.feeRecipient,
    })

    await expect(OmniBridgeWormhole.finTransfer(signature, payload, { value: WormholeFee }))
      .to.emit(TestWormhole, "MessagePublished")
      .withArgs(1, messagePayload, consistencyLevel)

    expect((await token.balanceOf(payload.recipient)).toString()).to.be.equal(
      payload.amount.toString(),
    )
  })

  it("init transfer", async () => {
    const { token } = await createToken(wrappedNearId)
    const tokenProxyAddress = await token.getAddress()
    const { signature, payload } = depositSignature(tokenProxyAddress, await user1.getAddress())
    await OmniBridgeWormhole.finTransfer(signature, payload, { value: WormholeFee })

    const recipient = "testrecipient.near"
    const fee = 0
    const nativeFee = 0
    const nonce = 1
    const message = ""

    const expectedWormholeMessage = InitTransferWormholeMessage.serialize({
      messageType: 0,
      originChainId: 0,
      sender: ethers.getBytes(await user1.getAddress()),
      destinationChainId: 0,
      tokenAddress: ethers.getBytes(tokenProxyAddress),
      originNonce: BigInt(nonce),
      amount: BigInt(payload.amount),
      fee: BigInt(fee),
      nativeFee: BigInt(nativeFee),
      recipient: recipient,
      message: message,
    })

    await expect(
      OmniBridgeWormhole.connect(user1).initTransfer(
        tokenProxyAddress,
        payload.amount,
        fee,
        nativeFee,
        recipient,
        message,
        {
          value: WormholeFee,
        },
      ),
    )
      .to.emit(TestWormhole, "MessagePublished")
      .withArgs(2, expectedWormholeMessage, consistencyLevel)

    expect((await token.balanceOf(await user1.getAddress())).toString()).to.be.equal("0")
  })

  describe("wormhole disabled", () => {
    const recipient = "testrecipient.near"

    async function fundUserWithToken(): Promise<string> {
      const { token } = await createToken(wrappedNearId)
      const tokenProxyAddress = await token.getAddress()
      const { signature, payload } = depositSignature(tokenProxyAddress, await user1.getAddress())
      await OmniBridgeWormhole.finTransfer(signature, payload, { value: WormholeFee })
      return tokenProxyAddress
    }

    async function disableWormhole(): Promise<void> {
      await expect(OmniBridgeWormhole.setWormholeAddress(ethers.ZeroAddress, 0))
        .to.emit(OmniBridgeWormhole, "WormholeAddressUpdated")
        .withArgs(ethers.ZeroAddress, 0)
    }

    it("only admin can change the wormhole address", async () => {
      await expect(
        OmniBridgeWormhole.connect(user1).setWormholeAddress(ethers.ZeroAddress, 0),
      ).to.be.revertedWithCustomError(OmniBridgeWormhole, "AccessControlUnauthorizedAccount")
    })

    it("init transfer emits the event and publishes nothing", async () => {
      const tokenProxyAddress = await fundUserWithToken()
      await disableWormhole()

      const tx = OmniBridgeWormhole.connect(user1).initTransfer(
        tokenProxyAddress,
        1,
        0,
        0,
        recipient,
        "",
      )
      await expect(tx)
        .to.emit(OmniBridgeWormhole, "InitTransfer")
        .withArgs(await user1.getAddress(), tokenProxyAddress, 1, 1, 0, 0, recipient, "")
      await expect(tx).to.not.emit(TestWormhole, "MessagePublished")
    })

    it("init transfer accepts exactly the native fee", async () => {
      const tokenProxyAddress = await fundUserWithToken()
      await disableWormhole()

      const nativeFee = 100
      await expect(
        OmniBridgeWormhole.connect(user1).initTransfer(
          tokenProxyAddress,
          1,
          0,
          nativeFee,
          recipient,
          "",
          { value: nativeFee },
        ),
      ).to.emit(OmniBridgeWormhole, "InitTransfer")
    })

    it("native transfer carries only amount and native fee", async () => {
      await disableWormhole()
      const amount = 1000n
      const nativeFee = 100n

      await expect(
        OmniBridgeWormhole.connect(user1).initTransfer(
          ethers.ZeroAddress,
          amount,
          0,
          nativeFee,
          recipient,
          "",
          { value: amount + nativeFee },
        ),
      )
        .to.emit(OmniBridgeWormhole, "InitTransfer")
        .withArgs(
          await user1.getAddress(),
          ethers.ZeroAddress,
          1,
          amount,
          0,
          nativeFee,
          recipient,
          "",
        )
      expect(await ethers.provider.getBalance(await OmniBridgeWormhole.getAddress())).to.equal(
        amount + nativeFee,
      )

      await expect(
        OmniBridgeWormhole.connect(user1).initTransfer(
          ethers.ZeroAddress,
          amount,
          0,
          nativeFee,
          recipient,
          "",
          { value: amount + nativeFee + BigInt(WormholeFee) },
        ),
      ).to.be.revertedWithCustomError(OmniBridgeWormhole, "InvalidValue")
    })

    it("init transfer rejects a wormhole fee", async () => {
      const tokenProxyAddress = await fundUserWithToken()
      await disableWormhole()

      await expect(
        OmniBridgeWormhole.connect(user1).initTransfer(tokenProxyAddress, 1, 0, 0, recipient, "", {
          value: WormholeFee,
        }),
      ).to.be.revertedWithCustomError(OmniBridgeWormhole, "InvalidValue")
    })

    it("deploy token works without value and rejects value", async () => {
      await disableWormhole()
      const { signature, payload } = metadataSignature(wrappedNearId)

      await expect(
        OmniBridgeWormhole.deployToken(signature, payload, { value: WormholeFee }),
      ).to.be.revertedWithCustomError(OmniBridgeWormhole, "InvalidValue")

      const tx = OmniBridgeWormhole.deployToken(signature, payload)
      await expect(tx).to.emit(OmniBridgeWormhole, "DeployToken")
      await expect(tx).to.not.emit(TestWormhole, "MessagePublished")
    })

    it("fin transfer works without value and rejects value", async () => {
      const { token } = await createToken(wrappedNearId)
      const tokenProxyAddress = await token.getAddress()
      await disableWormhole()
      const { signature, payload } = depositSignature(tokenProxyAddress, await user1.getAddress())

      await expect(
        OmniBridgeWormhole.finTransfer(signature, payload, { value: WormholeFee }),
      ).to.be.revertedWithCustomError(OmniBridgeWormhole, "InvalidValue")

      const tx = OmniBridgeWormhole.finTransfer(signature, payload)
      await expect(tx).to.emit(OmniBridgeWormhole, "FinTransfer")
      await expect(tx).to.not.emit(TestWormhole, "MessagePublished")
      expect(await token.balanceOf(payload.recipient)).to.equal(payload.amount)
    })

    it("log metadata works without value and rejects value", async () => {
      // logMetadata is for tokens the bridge did not deploy.
      const testToken_factory = await ethers.getContractFactory("TestBridgeToken")
      const testToken = await upgrades.deployProxy(testToken_factory, ["Test Token", "TT", 18], {
        initializer: "initialize",
      })
      await testToken.waitForDeployment()
      const tokenAddress = await testToken.getAddress()
      await disableWormhole()

      await expect(
        OmniBridgeWormhole.logMetadata(tokenAddress, { value: WormholeFee }),
      ).to.be.revertedWithCustomError(OmniBridgeWormhole, "InvalidValue")

      const tx = OmniBridgeWormhole.logMetadata(tokenAddress)
      await expect(tx).to.emit(OmniBridgeWormhole, "LogMetadata")
      await expect(tx).to.not.emit(TestWormhole, "MessagePublished")
    })

    it("re-enabling resumes publishing with the next nonce", async () => {
      const tokenProxyAddress = await fundUserWithToken()
      await disableWormhole()
      await OmniBridgeWormhole.connect(user1).initTransfer(
        tokenProxyAddress,
        1,
        0,
        0,
        recipient,
        "",
      )

      await expect(
        OmniBridgeWormhole.setWormholeAddress(await TestWormhole.getAddress(), consistencyLevel),
      )
        .to.emit(OmniBridgeWormhole, "WormholeAddressUpdated")
        .withArgs(await TestWormhole.getAddress(), consistencyLevel)

      // Nonces 0 and 1 were used while enabled; the disabled init transfer used none.
      const { signature, payload } = metadataSignature("other.testnet")
      await expect(OmniBridgeWormhole.deployToken(signature, payload, { value: WormholeFee }))
        .to.emit(TestWormhole, "MessagePublished")
        .withArgs(2, anyValue, consistencyLevel)
    })
  })

  it("can't init transfer without enough value", async () => {
    const { token } = await createToken(wrappedNearId)
    const tokenProxyAddress = await token.getAddress()
    const { signature, payload } = depositSignature(tokenProxyAddress, await user1.getAddress())
    await OmniBridgeWormhole.finTransfer(signature, payload, { value: WormholeFee })

    await expect(
      OmniBridgeWormhole.connect(user1).initTransfer(
        tokenProxyAddress,
        payload.amount,
        0,
        0,
        "testrecipient.near",
        "",
      ),
    ).to.be.revertedWith("invalid fee")
  })
})
