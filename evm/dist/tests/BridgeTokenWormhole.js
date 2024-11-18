"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const withArgs_1 = require("@nomicfoundation/hardhat-chai-matchers/withArgs");
const chai_1 = require("chai");
const hardhat_1 = require("hardhat");
const kdf_1 = require("./helpers/kdf");
const signatures_1 = require("./helpers/signatures");
describe("BridgeTokenWormhole", () => {
    const wrappedNearId = "wrap.testnet";
    const consistencyLevel = 3;
    let user1;
    let adminAccount;
    let BridgeTokenInstance;
    let BridgeTokenFactory;
    let TestWormhole;
    beforeEach(async () => {
        ;
        [adminAccount] = await hardhat_1.ethers.getSigners();
        user1 = await hardhat_1.ethers.getImpersonatedSigner("0x3A445243376C32fAba679F63586e236F77EA601e");
        await fundAddress(await user1.getAddress(), "1");
        const bridgeTokenFactory = await hardhat_1.ethers.getContractFactory("BridgeToken");
        const bridgeToken = await bridgeTokenFactory.deploy();
        await bridgeToken.waitForDeployment();
        const testWormholeFactory = await hardhat_1.ethers.getContractFactory("TestWormhole");
        TestWormhole = await testWormholeFactory.deploy();
        await TestWormhole.waitForDeployment();
        const nearBridgeDeriveAddress = await (0, kdf_1.deriveEthereumAddress)("omni-locker.testnet", "bridge-1");
        const omniBridgeChainId = 0;
        const bridgeTokenFactoryWormhole_factory = await hardhat_1.ethers.getContractFactory("BridgeTokenFactoryWormhole");
        const proxyContract = await hardhat_1.upgrades.deployProxy(bridgeTokenFactoryWormhole_factory, [
            await bridgeToken.getAddress(),
            nearBridgeDeriveAddress,
            omniBridgeChainId,
            await TestWormhole.getAddress(),
            consistencyLevel,
        ], { initializer: "initializeWormhole" });
        await proxyContract.waitForDeployment();
        BridgeTokenInstance = bridgeTokenFactory.attach(await bridgeToken.getAddress());
    });
    async function fundAddress(address, amount) {
        const tx = await adminAccount.sendTransaction({
            to: address,
            value: hardhat_1.ethers.parseEther(amount),
        });
        await tx.wait();
    }
    async function createToken(tokenId) {
        const { signature, payload } = (0, signatures_1.metadataSignature)(tokenId);
        await BridgeTokenFactory.deployToken(signature, payload);
        const tokenProxyAddress = await BridgeTokenFactory.nearToEthToken(tokenId);
        const token = BridgeTokenInstance.attach(tokenProxyAddress);
        return { tokenProxyAddress, token };
    }
    it("deploy token", async () => {
        const { signature, payload } = (0, signatures_1.metadataSignature)(wrappedNearId);
        await (0, chai_1.expect)(await BridgeTokenFactory.deployToken(signature, payload))
            .to.emit(TestWormhole, "MessagePublished")
            .withArgs(0, withArgs_1.anyValue, consistencyLevel);
    });
    it("deposit token", async () => {
        const { token } = await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, await user1.getAddress());
        const expectedPayload = hardhat_1.ethers.AbiCoder.defaultAbiCoder().encode(["uint8", "string", "uint256", "string", "uint128"], [1, wrappedNearId, payload.amount, payload.feeRecipient, payload.destinationNonce]);
        await (0, chai_1.expect)(BridgeTokenFactory.finTransfer(signature, payload))
            .to.emit(TestWormhole, "MessagePublished")
            .withArgs(1, expectedPayload, consistencyLevel);
        (0, chai_1.expect)((await token.balanceOf(payload.recipient)).toString()).to.be.equal(payload.amount.toString());
    });
    it("withdraw token", async () => {
        const { token } = await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, await user1.getAddress());
        await BridgeTokenFactory.finTransfer(signature, payload);
        const recipient = "testrecipient.near";
        const fee = 0;
        const nativeFee = 0;
        const nonce = 1;
        const message = "";
        const expectedPayload = hardhat_1.ethers.AbiCoder.defaultAbiCoder().encode(["uint8", "uint128", "string", "uint128", "uint128", "uint128", "string", "address"], [
            0,
            nonce,
            wrappedNearId,
            payload.amount,
            fee,
            nativeFee,
            recipient,
            await user1.getAddress(),
        ]);
        await (0, chai_1.expect)(BridgeTokenFactory.connect(user1).initTransfer(wrappedNearId, payload.amount, fee, nativeFee, recipient, message))
            .to.emit(TestWormhole, "MessagePublished")
            .withArgs(2, expectedPayload, consistencyLevel);
        (0, chai_1.expect)((await token.balanceOf(await user1.getAddress())).toString()).to.be.equal("0");
    });
});
