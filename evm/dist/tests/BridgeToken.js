"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const chai_1 = require("chai");
const hardhat_1 = require("hardhat");
const kdf_1 = require("./helpers/kdf");
const signatures_1 = require("./helpers/signatures");
const PauseMode = {
    UnpausedAll: 0,
    PausedInitTransfer: 1 << 0,
    PausedFinTransfer: 1 << 1,
};
const PauseAll = PauseMode.PausedInitTransfer | PauseMode.PausedFinTransfer;
describe("BridgeToken", () => {
    const wrappedNearId = "wrap.testnet";
    let BridgeTokenInstance;
    let BridgeTokenFactory;
    let adminAccount;
    let user1;
    let user2;
    beforeEach(async () => {
        ;
        [adminAccount] = await hardhat_1.ethers.getSigners();
        user1 = await hardhat_1.ethers.getImpersonatedSigner("0x3A445243376C32fAba679F63586e236F77EA601e");
        user2 = await hardhat_1.ethers.getImpersonatedSigner("0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265");
        await fundAddress(user1.address, "1");
        await fundAddress(user2.address, "1");
        const BridgeToken_factory = await hardhat_1.ethers.getContractFactory("BridgeToken");
        const bridgeToken = await BridgeToken_factory.deploy();
        BridgeTokenInstance = await bridgeToken.waitForDeployment();
        const nearBridgeDeriveAddress = await (0, kdf_1.deriveEthereumAddress)("omni-locker.testnet", "bridge-1");
        //console.log(await deriveChildPublicKey(najPublicKeyStrToUncompressedHexPoint(), 'omni-locker.testnet', 'bridge-1'));
        const omniBridgeChainId = 0;
        const BridgeTokenFactory_factory = await hardhat_1.ethers.getContractFactory("BridgeTokenFactory");
        const upgradedContract = await hardhat_1.upgrades.deployProxy(BridgeTokenFactory_factory, [await bridgeToken.getAddress(), nearBridgeDeriveAddress, omniBridgeChainId], { initializer: "initialize" });
        BridgeTokenFactory =
            (await upgradedContract.waitForDeployment());
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
    it("can create a token", async () => {
        await createToken(wrappedNearId);
        const tokenProxyAddress = await BridgeTokenFactory.nearToEthToken(wrappedNearId);
        const token = BridgeTokenInstance.attach(tokenProxyAddress);
        (0, chai_1.expect)(await token.name()).to.be.equal("Wrapped NEAR fungible token");
        (0, chai_1.expect)(await token.symbol()).to.be.equal("wNEAR");
        (0, chai_1.expect)((await token.decimals()).toString()).to.be.equal("24");
    });
    it("can't create token if token already exists", async () => {
        await createToken(wrappedNearId);
        await (0, chai_1.expect)(createToken(wrappedNearId)).to.be.revertedWith("ERR_TOKEN_EXIST");
    });
    it("can update token's metadata", async () => {
        const { token } = await createToken(wrappedNearId);
        await BridgeTokenFactory.setMetadata(wrappedNearId, "Circle USDC Bridged", "USDC.E");
        (0, chai_1.expect)(await token.name()).to.equal("Circle USDC Bridged");
        (0, chai_1.expect)(await token.symbol()).to.equal("USDC.E");
    });
    it("can't update metadata of non-existent token", async () => {
        await createToken(wrappedNearId);
        await (0, chai_1.expect)(BridgeTokenFactory.setMetadata("non-existing", "Circle USDC", "USDC")).to.be.revertedWith("ERR_NOT_BRIDGE_TOKEN");
    });
    it("can't update metadata as a normal user", async () => {
        await createToken(wrappedNearId);
        await (0, chai_1.expect)(BridgeTokenFactory.connect(user1).setMetadata(wrappedNearId, "Circle USDC", "USDC")).to.be.revertedWithCustomError(BridgeTokenFactory, "AccessControlUnauthorizedAccount");
    });
    it("deposit token", async () => {
        const { token } = await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        await (0, chai_1.expect)(BridgeTokenFactory.finTransfer(signature, payload))
            .to.emit(BridgeTokenFactory, "FinTransfer")
            .withArgs(payload.destinationNonce, wrappedNearId, 1, payload.recipient, payload.feeRecipient);
        (0, chai_1.expect)((await token.balanceOf(payload.recipient)).toString()).to.be.equal(payload.amount.toString());
    });
    it("can't deposit if the contract is paused", async () => {
        await createToken(wrappedNearId);
        const tokenProxyAddress = await BridgeTokenFactory.nearToEthToken(wrappedNearId);
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.PausedFinTransfer))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.PausedFinTransfer);
        const { signature, payload } = (0, signatures_1.depositSignature)(tokenProxyAddress, user1.address);
        await (0, chai_1.expect)(BridgeTokenFactory.finTransfer(signature, payload)).to.be.revertedWith("Pausable: paused");
    });
    it("can't deposit twice with the same signature", async () => {
        await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        await BridgeTokenFactory.finTransfer(signature, payload);
        await (0, chai_1.expect)(BridgeTokenFactory.finTransfer(signature, payload)).to.be.revertedWithCustomError(BridgeTokenFactory, "NonceAlreadyUsed");
    });
    it("can't deposit with invalid amount", async () => {
        await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        payload.amount = 100000;
        await (0, chai_1.expect)(BridgeTokenFactory.finTransfer(signature, payload)).to.be.revertedWithCustomError(BridgeTokenFactory, "InvalidSignature");
    });
    it("can't deposit with invalid nonce", async () => {
        await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        payload.destinationNonce = 99;
        await (0, chai_1.expect)(BridgeTokenFactory.finTransfer(signature, payload)).to.be.revertedWithCustomError(BridgeTokenFactory, "InvalidSignature");
    });
    it("can't deposit with invalid token", async () => {
        await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        payload.tokenAddress = "test-token.testnet";
        await (0, chai_1.expect)(BridgeTokenFactory.finTransfer(signature, payload)).to.be.revertedWithCustomError(BridgeTokenFactory, "InvalidSignature");
    });
    it("can't deposit with invalid recipient", async () => {
        await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        payload.recipient = user2.address;
        await (0, chai_1.expect)(BridgeTokenFactory.finTransfer(signature, payload)).to.be.revertedWithCustomError(BridgeTokenFactory, "InvalidSignature");
    });
    it("can't deposit with invalid relayer", async () => {
        await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        payload.feeRecipient = "testrecipient.near";
        await (0, chai_1.expect)(BridgeTokenFactory.finTransfer(signature, payload)).to.be.revertedWithCustomError(BridgeTokenFactory, "InvalidSignature");
    });
    it("withdraw token", async () => {
        const { token } = await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        await BridgeTokenFactory.finTransfer(signature, payload);
        const recipient = "testrecipient.near";
        const fee = 0;
        const nativeFee = 0;
        await (0, chai_1.expect)(BridgeTokenFactory.connect(user1).initTransfer(wrappedNearId, payload.amount, fee, nativeFee, recipient, ""))
            .to.emit(BridgeTokenFactory, "InitTransfer")
            .withArgs(user1.address, await BridgeTokenFactory.nearToEthToken(wrappedNearId), 1, wrappedNearId, payload.amount, fee, nativeFee, recipient);
        (0, chai_1.expect)((await token.balanceOf(user1.address)).toString()).to.be.equal("0");
    });
    it("cant withdraw token when paused", async () => {
        await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        await BridgeTokenFactory.finTransfer(signature, payload);
        const fee = 0;
        const nativeFee = 0;
        const message = "";
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.PausedInitTransfer))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.PausedInitTransfer);
        await (0, chai_1.expect)(BridgeTokenFactory.initTransfer(wrappedNearId, payload.amount, fee, nativeFee, "testrecipient.near", message)).to.be.revertedWith("Pausable: paused");
    });
    it("can deposit and withdraw after unpausing", async () => {
        const { token } = await createToken(wrappedNearId);
        const { signature, payload } = (0, signatures_1.depositSignature)(wrappedNearId, user1.address);
        await BridgeTokenFactory.finTransfer(signature, payload);
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.PausedInitTransfer))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.PausedInitTransfer);
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.UnpausedAll))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.UnpausedAll);
        const recipient = "testrecipient.near";
        const fee = 0;
        const nativeFee = 0;
        const message = "";
        await BridgeTokenFactory.connect(user1).initTransfer(wrappedNearId, payload.amount, fee, nativeFee, recipient, message);
        (0, chai_1.expect)((await token.balanceOf(user1.address)).toString()).to.be.equal("0");
    });
    it("upgrade token contract", async () => {
        const { tokenProxyAddress } = await createToken(wrappedNearId);
        const BridgeTokenV2Instance = await hardhat_1.ethers.getContractFactory("TestBridgeToken");
        const BridgeTokenV2 = await BridgeTokenV2Instance.deploy();
        await BridgeTokenV2.waitForDeployment();
        await BridgeTokenFactory.upgradeToken(wrappedNearId, await BridgeTokenV2.getAddress());
        const BridgeTokenV2Proxied = BridgeTokenV2Instance.attach(tokenProxyAddress);
        (0, chai_1.expect)(await BridgeTokenV2Proxied.returnTestString()).to.equal("test");
        (0, chai_1.expect)(await BridgeTokenV2Proxied.name()).to.equal("Wrapped NEAR fungible token");
        (0, chai_1.expect)(await BridgeTokenV2Proxied.symbol()).to.equal("wNEAR");
        (0, chai_1.expect)((await BridgeTokenV2Proxied.decimals()).toString()).to.equal("24");
    });
    it("user cant upgrade token contract", async () => {
        await createToken(wrappedNearId);
        const BridgeTokenV2Instance = await hardhat_1.ethers.getContractFactory("TestBridgeToken");
        const BridgeTokenV2 = await BridgeTokenV2Instance.deploy();
        await BridgeTokenV2.waitForDeployment();
        await (0, chai_1.expect)(BridgeTokenFactory.connect(user1).upgradeToken(wrappedNearId, await BridgeTokenV2.getAddress())).to.be.revertedWithCustomError(BridgeTokenFactory, "AccessControlUnauthorizedAccount");
    });
    it("Test selective pause", async () => {
        // Pause withdraw
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.PausedInitTransfer))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.PausedInitTransfer);
        (0, chai_1.expect)(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.PausedInitTransfer);
        // Pause withdraw again
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.PausedInitTransfer))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.PausedInitTransfer);
        (0, chai_1.expect)(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.PausedInitTransfer);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedFinTransfer)).to.be.equal(false);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.equal(true);
        // Pause deposit
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
        (0, chai_1.expect)(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
        // Pause deposit again
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
        (0, chai_1.expect)(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
        // Pause deposit and withdraw
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
        (0, chai_1.expect)(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedFinTransfer)).to.be.equal(true);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.equal(true);
        // Unpause all
        await (0, chai_1.expect)(BridgeTokenFactory.pause(PauseMode.UnpausedAll))
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.UnpausedAll);
        (0, chai_1.expect)(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.UnpausedAll);
        // Pause all
        await (0, chai_1.expect)(BridgeTokenFactory.pauseAll())
            .to.emit(BridgeTokenFactory, "Paused")
            .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
        (0, chai_1.expect)(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedFinTransfer)).to.be.equal(true);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.equal(true);
    });
    it("Test grant admin role", async () => {
        await BridgeTokenFactory.connect(adminAccount).pause(PauseMode.UnpausedAll);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.false;
        await BridgeTokenFactory.connect(adminAccount).pauseAll();
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;
        const signers = await hardhat_1.ethers.getSigners();
        const newAdminAccount = signers[2];
        const DEFAULT_ADMIN_ROLE = "0x0000000000000000000000000000000000000000000000000000000000000000";
        await (0, chai_1.expect)(BridgeTokenFactory.connect(newAdminAccount).pause(PauseMode.UnpausedAll)).to.be.revertedWithCustomError(BridgeTokenFactory, "AccessControlUnauthorizedAccount");
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;
        // Grant DEFAULT_ADMIN_ROLE to newAdminAccount
        await (0, chai_1.expect)(BridgeTokenFactory.grantRole(DEFAULT_ADMIN_ROLE, newAdminAccount.address))
            .to.emit(BridgeTokenFactory, "RoleGranted")
            .withArgs(DEFAULT_ADMIN_ROLE, newAdminAccount.address, adminAccount.address);
        await BridgeTokenFactory.connect(newAdminAccount).pause(PauseMode.UnpausedAll);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.false;
        await BridgeTokenFactory.connect(newAdminAccount).pause(PauseAll);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;
        // Revoke DEFAULT_ADMIN_ROLE from adminAccount
        await (0, chai_1.expect)(BridgeTokenFactory.connect(newAdminAccount).revokeRole(DEFAULT_ADMIN_ROLE, adminAccount.address))
            .to.emit(BridgeTokenFactory, "RoleRevoked")
            .withArgs(DEFAULT_ADMIN_ROLE, adminAccount.address, newAdminAccount.address);
        // Check tx reverted on call from revoked adminAccount
        await (0, chai_1.expect)(BridgeTokenFactory.connect(adminAccount).pause(PauseMode.UnpausedAll)).to.be.revertedWithCustomError(BridgeTokenFactory, "AccessControlUnauthorizedAccount");
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;
        // Check newAdminAccount can perform admin calls
        await BridgeTokenFactory.connect(newAdminAccount).pause(PauseMode.UnpausedAll);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.false;
        await BridgeTokenFactory.connect(newAdminAccount).pause(PauseAll);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;
        // Check newAdminAccount can grant DEFAULT_ADMIN_ROLE to adminAccount
        await (0, chai_1.expect)(BridgeTokenFactory.connect(newAdminAccount).grantRole(DEFAULT_ADMIN_ROLE, adminAccount.address))
            .to.emit(BridgeTokenFactory, "RoleGranted")
            .withArgs(DEFAULT_ADMIN_ROLE, adminAccount.address, newAdminAccount.address);
        // Check that adminAccount can perform admin calls again
        await BridgeTokenFactory.connect(adminAccount).pause(PauseMode.UnpausedAll);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.false;
        await BridgeTokenFactory.connect(adminAccount).pause(PauseAll);
        (0, chai_1.expect)(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;
    });
});
