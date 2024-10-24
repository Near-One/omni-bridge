const { expect } = require('chai')
const { ethers, upgrades } = require('hardhat')
const { metadataSignature, depositSignature } = require('./helpers/signatures')
const { anyValue } = require("@nomicfoundation/hardhat-chai-matchers/withArgs")
const { deriveEthereumAddress } = require('./helpers/kdf')

describe('BridgeTokenWormhole', () => {
  const wrappedNearId = 'wrap.testnet';
  const consistencyLevel = 3;

  let user1, adminAccount;
  let BridgeTokenInstance;
  let BridgeTokenFactory;
  let TestWormhole;

  beforeEach(async function () {
    [adminAccount] = await ethers.getSigners();
    user1 = await ethers.getImpersonatedSigner('0x3A445243376C32fAba679F63586e236F77EA601e');
    await fundAddress(user1.address, "1");

    BridgeTokenInstance = await ethers.getContractFactory('BridgeToken');
    const bridgeToken = await BridgeTokenInstance.deploy();
    await bridgeToken.waitForDeployment();

    TestWormhole = await ethers.getContractFactory('TestWormhole');
    TestWormhole = await TestWormhole.deploy();
    await TestWormhole.waitForDeployment();

    const nearBridgeDeriveAddress = await deriveEthereumAddress('omni-locker.test1-dev.testnet', 'bridge-1');
    const omniBridgeChainId = 0;

    BridgeTokenFactory = await ethers.getContractFactory('BridgeTokenFactoryWormhole');
    BridgeTokenFactory = await upgrades.deployProxy(BridgeTokenFactory, [
      await bridgeToken.getAddress(),
      nearBridgeDeriveAddress,
      omniBridgeChainId,
      await TestWormhole.getAddress(),
      consistencyLevel
    ], { initializer: 'initializeWormhole' });
    await BridgeTokenFactory.waitForDeployment();
  });

  async function fundAddress(address, amount) {
    const tx = await adminAccount.sendTransaction({
      to: address,
      value: ethers.parseEther(amount)
    });
    await tx.wait();
  }

  async function createToken(tokenId) {
    const { signature, payload } = metadataSignature(tokenId);
  
    await BridgeTokenFactory.deployToken(signature, payload);
    const tokenProxyAddress = await BridgeTokenFactory.nearToEthToken(tokenId)
    const token = BridgeTokenInstance.attach(tokenProxyAddress)
    return { tokenProxyAddress, token }
  }

  it('deploy token', async function () {
    const { signature, payload } = metadataSignature(wrappedNearId);

    await expect(
      await BridgeTokenFactory.deployToken(signature, payload)
    )
      .to
      .emit(TestWormhole, 'MessagePublished')
      .withArgs(0, anyValue, consistencyLevel);
  });

  it('deposit token', async function () {
    const { token } = await createToken(wrappedNearId);
    const { signature, payload } = depositSignature(wrappedNearId, user1.address);

    const expectedPayload = ethers.AbiCoder.defaultAbiCoder().encode(
        ["uint8", "string", "uint256", "string", "uint128"],
        [1, wrappedNearId, payload.amount, payload.feeRecipient, payload.nonce]
    );

    await expect(
      BridgeTokenFactory
        .finTransfer(signature, payload)
    )
       .to
       .emit(TestWormhole, 'MessagePublished')
       .withArgs(1, expectedPayload, consistencyLevel);

    expect(
      (await token.balanceOf(payload.recipient))
        .toString()
    )
      .to
      .be
      .equal(payload.amount.toString())
  });

  it('withdraw token', async function () {
    const { token } = await createToken(wrappedNearId);
    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    await BridgeTokenFactory
      .finTransfer(signature, payload);

    const recipient = 'testrecipient.near';
    const fee = 0;
    const nativeFee = 0;
    const nonce = 1;
    const expectedPayload = ethers.AbiCoder.defaultAbiCoder().encode(
        ["uint8", "uint128", "string", "uint128", "uint128", "uint128", "string", "address"],
        [0, nonce, wrappedNearId, payload.amount, fee, nativeFee, recipient, user1.address]
    );

    await expect(
      BridgeTokenFactory.connect(user1).initTransfer(
        wrappedNearId,
        payload.amount,
        fee,
        nativeFee,
        recipient
      )
    )
      .to
      .emit(TestWormhole, "MessagePublished")
      .withArgs(2, expectedPayload, consistencyLevel);

    expect((await token.balanceOf(user1.address)).toString()).to.be.equal('0')
  });
});