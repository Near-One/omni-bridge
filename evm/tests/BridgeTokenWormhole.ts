import { anyValue } from "@nomicfoundation/hardhat-chai-matchers/withArgs"
import * as borsh from "borsh"
import { expect } from "chai"
import type { BigNumberish, Signer } from "ethers"
import { ethers, upgrades } from "hardhat"
import type { BridgeToken, OmniBridgeWormhole, TestWormhole } from "../typechain-types"
import { depositSignature, metadataSignature, testWallet } from "./helpers/signatures"

class FinTransferWormholeMessage {
	static schema = {
		struct: {
			messageType: "u8",
			originChain: "u8",
			originNonce: "u128",
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
			originNonce: "u128",
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

	let user1: Signer
	let adminAccount: Signer
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

		const OmniBridgeWormhole_factory = await ethers.getContractFactory(
			"OmniBridgeWormhole",
		)
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

		await OmniBridgeWormhole.deployToken(signature, payload)
		const tokenProxyAddress = await OmniBridgeWormhole.nearToEthToken(tokenId)
		const token = OmniBridgeInstance.attach(tokenProxyAddress) as BridgeToken
		return { tokenProxyAddress, token }
	}

	it("deploy token", async () => {
		const { signature, payload } = metadataSignature(wrappedNearId)

		await expect(await OmniBridgeWormhole.deployToken(signature, payload))
			.to.emit(TestWormhole, "MessagePublished")
			.withArgs(0, anyValue, consistencyLevel)
	})

	it("deposit token", async () => {
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

		await expect(OmniBridgeWormhole.finTransfer(signature, payload))
			.to.emit(TestWormhole, "MessagePublished")
			.withArgs(1, messagePayload, consistencyLevel)

		expect((await token.balanceOf(payload.recipient)).toString()).to.be.equal(
			payload.amount.toString(),
		)
	})

	it("withdraw token", async () => {
		const { token } = await createToken(wrappedNearId)
		const tokenProxyAddress = await token.getAddress()
		const { signature, payload } = depositSignature(tokenProxyAddress, await user1.getAddress())
		await OmniBridgeWormhole.finTransfer(signature, payload)

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
			),
		)
			.to.emit(TestWormhole, "MessagePublished")
			.withArgs(2, expectedWormholeMessage, consistencyLevel)

		expect((await token.balanceOf(await user1.getAddress())).toString()).to.be.equal("0")
	})
})
