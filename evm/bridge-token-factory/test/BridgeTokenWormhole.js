const { expect } = require('chai')
const { ethers, upgrades } = require('hardhat')
const { metadataSignature, depositSignature } = require('./signatures')

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

    const nearBridgeDeriveAddress = "0xa966f32b64caaee9211d674e698cb72100b5e792";

    BridgeTokenFactory = await ethers.getContractFactory('BridgeTokenFactoryWormhole');
    BridgeTokenFactory = await upgrades.deployProxy(BridgeTokenFactory, [
      await bridgeToken.getAddress(),
      nearBridgeDeriveAddress,
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
  
    await BridgeTokenFactory.newBridgeToken(signature, payload);
    const tokenProxyAddress = await BridgeTokenFactory.nearToEthToken(tokenId)
    const token = BridgeTokenInstance.attach(tokenProxyAddress)
    return { tokenProxyAddress, token }
  }

  it('deposit token', async function () {
    const { token } = await createToken(wrappedNearId);
    const { signature, payload } = depositSignature(wrappedNearId, user1.address);

    const expectedPayload = ethers.AbiCoder.defaultAbiCoder().encode(
        ["string", "uint256", "string", "uint128"],
        [wrappedNearId, payload.amount, payload.feeRecipient, payload.nonce]
    );

    await expect(
      BridgeTokenFactory
        .deposit(signature, payload)
    )
       .to
       .emit(TestWormhole, 'MessagePublished')
       .withArgs(0, expectedPayload, consistencyLevel);

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
      .deposit(signature, payload);

    const recipient = 'testrecipient.near';
    const expectedPayload = ethers.AbiCoder.defaultAbiCoder().encode(
        ["string", "uint128", "string"],
        [wrappedNearId, payload.amount, recipient]
    );

    await expect(
      BridgeTokenFactory.connect(user1).withdraw(
        wrappedNearId,
        payload.amount,
        recipient
      )
    )
      .to
      .emit(TestWormhole, "MessagePublished")
      .withArgs(1, expectedPayload, consistencyLevel);

    expect((await token.balanceOf(user1.address)).toString()).to.be.equal('0')
  });
});