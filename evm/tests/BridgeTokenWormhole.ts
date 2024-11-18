import { anyValue } from "@nomicfoundation/hardhat-chai-matchers/withArgs"
import { expect } from "chai"
import type { Signer } from "ethers"
import { ethers, upgrades } from "hardhat"
import type { BridgeToken, BridgeTokenFactory, BridgeTokenFactoryWormhole, TestWormhole } from "../typechain-types"
import { deriveEthereumAddress } from "./helpers/kdf"
import { depositSignature, metadataSignature } from "./helpers/signatures"

describe("BridgeTokenWormhole", () => {
	const wrappedNearId = "wrap.testnet"
	const consistencyLevel = 3

	let user1: Signer
	let adminAccount: Signer
	let BridgeTokenInstance: BridgeToken
	let BridgeTokenFactory: BridgeTokenFactory
	let TestWormhole: TestWormhole

	beforeEach(async () => {
		;[adminAccount] = await ethers.getSigners()
		user1 = await ethers.getImpersonatedSigner("0x3A445243376C32fAba679F63586e236F77EA601e")
		await fundAddress(await user1.getAddress(), "1")

		const bridgeTokenFactory = await ethers.getContractFactory("BridgeToken")
		BridgeTokenInstance = await bridgeTokenFactory.deploy()
		await BridgeTokenInstance.waitForDeployment()

		const testWormholeFactory = await ethers.getContractFactory("TestWormhole")
		TestWormhole = await testWormholeFactory.deploy()
		await TestWormhole.waitForDeployment()

		const nearBridgeDeriveAddress = await deriveEthereumAddress("omni-locker.testnet", "bridge-1")
		const omniBridgeChainId = 0

		const bridgeTokenFactoryWormhole_factory = await ethers.getContractFactory(
			"BridgeTokenFactoryWormhole",
		)
		const proxyContract = await upgrades.deployProxy(
			bridgeTokenFactoryWormhole_factory,
			[
				await BridgeTokenInstance.getAddress(),
				nearBridgeDeriveAddress,
				omniBridgeChainId,
				await TestWormhole.getAddress(),
				consistencyLevel,
			],
			{ initializer: "initializeWormhole" },
		)
		await proxyContract.waitForDeployment()

		BridgeTokenFactory = bridgeTokenFactory.attach(await BridgeTokenInstance.getAddress()) as BridgeTokenFactoryWormhole
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

		await BridgeTokenFactory.deployToken(signature, payload)
		const tokenProxyAddress = await BridgeTokenFactory.nearToEthToken(tokenId)
		const token = BridgeTokenInstance.attach(tokenProxyAddress) as BridgeToken
		return { tokenProxyAddress, token }
	}

	it("deploy token", async () => {
		const { signature, payload } = metadataSignature(wrappedNearId)

		await expect(await BridgeTokenFactory.deployToken(signature, payload))
			.to.emit(TestWormhole, "MessagePublished")
			.withArgs(0, anyValue, consistencyLevel)
	})

	it("deposit token", async () => {
		const { token } = await createToken(wrappedNearId)
		const { signature, payload } = depositSignature(wrappedNearId, await user1.getAddress())

		const expectedPayload = ethers.AbiCoder.defaultAbiCoder().encode(
			["uint8", "string", "uint256", "string", "uint128"],
			[1, wrappedNearId, payload.amount, payload.feeRecipient, payload.destinationNonce],
		)

		await expect(BridgeTokenFactory.finTransfer(signature, payload))
			.to.emit(TestWormhole, "MessagePublished")
			.withArgs(1, expectedPayload, consistencyLevel)

		expect((await token.balanceOf(payload.recipient)).toString()).to.be.equal(
			payload.amount.toString(),
		)
	})

	it("withdraw token", async () => {
		const { token } = await createToken(wrappedNearId)
		const { signature, payload } = depositSignature(wrappedNearId, await user1.getAddress())
		await BridgeTokenFactory.finTransfer(signature, payload)

		const recipient = "testrecipient.near"
		const fee = 0
		const nativeFee = 0
		const nonce = 1
		const message = ""
		const expectedPayload = ethers.AbiCoder.defaultAbiCoder().encode(
			["uint8", "uint128", "string", "uint128", "uint128", "uint128", "string", "address"],
			[
				0,
				nonce,
				wrappedNearId,
				payload.amount,
				fee,
				nativeFee,
				recipient,
				await user1.getAddress(),
			],
		)

		await expect(
			BridgeTokenFactory.connect(user1).initTransfer(
				wrappedNearId,
				payload.amount,
				fee,
				nativeFee,
				recipient,
				message,
			),
		)
			.to.emit(TestWormhole, "MessagePublished")
			.withArgs(2, expectedPayload, consistencyLevel)

		expect((await token.balanceOf(await user1.getAddress())).toString()).to.be.equal("0")
	})
})
