const { expect } = require('chai')
const { ethers, upgrades } = require('hardhat')
const { metadataSignature, depositSignature } = require('./helpers/signatures')
const { deriveEthereumAddress } = require('./helpers/kdf')

const WhitelistMode = {
  NotInitialized: 0,
  Blocked: 1,
  CheckToken: 2,
  CheckAccountAndToken: 3
}

const PauseMode = {
  UnpausedAll: 0,
  PausedInitTransfer: 1 << 0,
  PausedFinTransfer: 1 << 1,
}
const PauseAll = PauseMode.PausedInitTransfer | PauseMode.PausedFinTransfer;

describe('BridgeToken', () => {
  const wrappedNearId = 'wrap.testnet'

  let BridgeTokenInstance
  let BridgeTokenFactory
  let adminAccount
  let user1
  let user2

  beforeEach(async function () {
    [adminAccount] = await ethers.getSigners()
    user1 = await ethers.getImpersonatedSigner('0x3A445243376C32fAba679F63586e236F77EA601e')
    user2 = await ethers.getImpersonatedSigner('0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265');

    await fundAddress(user1.address, "1");
    await fundAddress(user2.address, "1");

    BridgeTokenInstance = await ethers.getContractFactory('BridgeToken')
    const bridgeToken = await BridgeTokenInstance.deploy()
    await bridgeToken.waitForDeployment()

    const nearBridgeDeriveAddress = await deriveEthereumAddress('omni-locker.test1-dev.testnet', 'bridge-1');
    const omniBridgeChainId = 0;

    BridgeTokenFactory = await ethers.getContractFactory('BridgeTokenFactory')
    BridgeTokenFactory = await upgrades.deployProxy(BridgeTokenFactory, [
      await bridgeToken.getAddress(),
      nearBridgeDeriveAddress,
      omniBridgeChainId,
    ], { initializer: 'initialize' });
    await BridgeTokenFactory.waitForDeployment();
  })

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

  it('can create a token', async function () {
    await createToken(wrappedNearId);
    const tokenProxyAddress = await BridgeTokenFactory.nearToEthToken(wrappedNearId)
    const token = BridgeTokenInstance.attach(tokenProxyAddress)
    expect(await token.name()).to.be.equal('Wrapped NEAR fungible token')
    expect(await token.symbol()).to.be.equal('wNEAR')
    expect((await token.decimals()).toString()).to.be.equal('24')
  })

  it('can\'t create token if token already exists', async function () {
    await createToken(wrappedNearId);
    await expect(createToken(wrappedNearId))
      .to.be.revertedWith('ERR_TOKEN_EXIST')
  })

  it("can update token's metadata", async function() {
    const { token } = await createToken(wrappedNearId);

    await BridgeTokenFactory.setMetadata(wrappedNearId, 'Circle USDC Bridged', 'USDC.E');
    expect(await token.name()).to.equal('Circle USDC Bridged');
    expect(await token.symbol()).to.equal('USDC.E');
  });

  it('can\'t update metadata of non-existent token', async function () {
    await createToken(wrappedNearId);

    await expect(
      BridgeTokenFactory.setMetadata('non-existing', 'Circle USDC', 'USDC')
    ).to.be.revertedWith('ERR_NOT_BRIDGE_TOKEN');
  })

  it('can\'t update metadata as a normal user', async function () {
    await createToken(wrappedNearId);

    await expect(
      BridgeTokenFactory.connect(user1).setMetadata(wrappedNearId, 'Circle USDC', 'USDC')
    ).to.be.revertedWithCustomError(BridgeTokenFactory, 'AccessControlUnauthorizedAccount');
  })

  it('deposit token', async function () {
    const { token } = await createToken(wrappedNearId);

    const { signature, payload } = depositSignature(wrappedNearId, user1.address);

    await expect(
      BridgeTokenFactory
        .finTransfer(signature, payload)
    )
      .to
      .emit(BridgeTokenFactory, 'FinTransfer')
      .withArgs(payload.nonce, wrappedNearId, 1, payload.recipient, payload.feeRecipient);

    expect(
      (await token.balanceOf(payload.recipient))
          .toString()
    )
      .to
      .be
      .equal(payload.amount.toString())
  })

  it('can\'t deposit if the contract is paused', async function () {
    await createToken(wrappedNearId);

    await expect (
      BridgeTokenFactory.pause(PauseMode.PausedFinTransfer)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer);

    const { signature, payload } = depositSignature(wrappedNearId, user1.address);

    await expect(
      BridgeTokenFactory
        .finTransfer(signature, payload)
    )
      .to
      .be
      .revertedWith('Pausable: paused');
  })

  it('can\'t deposit twice with the same signature', async function () {
    await createToken(wrappedNearId);
    
    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    await BridgeTokenFactory.finTransfer(signature, payload);

    await expect(
      BridgeTokenFactory.finTransfer(signature, payload)
    )
      .to.be.revertedWithCustomError(BridgeTokenFactory, 'NonceAlreadyUsed');
  })

  it('can\'t deposit with invalid amount', async function () {
    await createToken(wrappedNearId);
    
    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    payload.amount = 100000;

    await expect(
      BridgeTokenFactory.finTransfer(signature, payload)
    )
      .to.be.revertedWithCustomError(BridgeTokenFactory, 'InvalidSignature');
  })

  it('can\'t deposit with invalid nonce', async function () {
    await createToken(wrappedNearId);
    
    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    payload.nonce = 99;

    await expect(
      BridgeTokenFactory.finTransfer(signature, payload)
    )
      .to.be.revertedWithCustomError(BridgeTokenFactory, 'InvalidSignature');
  })

  it('can\'t deposit with invalid token', async function () {
    await createToken(wrappedNearId);
    
    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    payload.token = 'test-token.testnet';

    await expect(
      BridgeTokenFactory.finTransfer(signature, payload)
    )
      .to.be.revertedWithCustomError(BridgeTokenFactory, 'InvalidSignature');
  })

  it('can\'t deposit with invalid recipient', async function () {
    await createToken(wrappedNearId);
    
    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    payload.recipient = user2.address;

    await expect(
      BridgeTokenFactory.finTransfer(signature, payload)
    )
      .to.be.revertedWithCustomError(BridgeTokenFactory, 'InvalidSignature');
  })

  it('can\'t deposit with invalid relayer', async function () {
    await createToken(wrappedNearId);
    
    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    payload.feeRecipient = "testrecipient.near";

    await expect(
      BridgeTokenFactory.finTransfer(signature, payload)
    )
      .to.be.revertedWithCustomError(BridgeTokenFactory, 'InvalidSignature');
  })

  it('withdraw token', async function () {
    const { token } = await createToken(wrappedNearId);

    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    await BridgeTokenFactory.finTransfer(signature, payload);

    const recipient = 'testrecipient.near';
    const fee = 0;
    const nativeFee = 0;

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
      .emit(BridgeTokenFactory, "InitTransfer")
      .withArgs(
        user1.address,
        await BridgeTokenFactory.nearToEthToken(wrappedNearId),
        1,
        wrappedNearId,
        payload.amount,
        fee,
        nativeFee,
        recipient,
      );

    expect((await token.balanceOf(user1.address)).toString()).to.be.equal('0')
  })

  it('cant withdraw token when paused', async function () {
    await createToken(wrappedNearId);

    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    await BridgeTokenFactory.finTransfer(signature, payload);

    const fee = 0;
    const nativeFee = 0;
    await expect(
      BridgeTokenFactory.pause(PauseMode.PausedInitTransfer)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.PausedInitTransfer);
    await expect(
      BridgeTokenFactory.initTransfer(wrappedNearId, payload.amount, fee, nativeFee, 'testrecipient.near')
    )
      .to
      .be
      .revertedWith('Pausable: paused');
  })

  it('can deposit and withdraw after unpausing', async function () {
    const { token } = await createToken(wrappedNearId);

    const { signature, payload } = depositSignature(wrappedNearId, user1.address);
    await BridgeTokenFactory.finTransfer(signature, payload);
  
    await expect(
      BridgeTokenFactory.pause(PauseMode.PausedInitTransfer)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.PausedInitTransfer);

    await expect(
      BridgeTokenFactory.pause(PauseMode.UnpausedAll)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.UnpausedAll);

      const recipient = 'testrecipient.near';
      const fee = 0;
      const nativeFee = 0;
      await BridgeTokenFactory.connect(user1).initTransfer(
        wrappedNearId,
        payload.amount,
        fee,
        nativeFee,
        recipient
      );
  
      expect((await token.balanceOf(user1.address)).toString()).to.be.equal('0')
  })

  it('upgrade token contract', async function () {
    const { tokenProxyAddress } = await createToken(wrappedNearId);

    const BridgeTokenV2Instance = await ethers.getContractFactory("TestBridgeToken");
    const BridgeTokenV2 = await BridgeTokenV2Instance.deploy();
    await BridgeTokenV2.waitForDeployment();

    await BridgeTokenFactory.upgradeToken(wrappedNearId, await BridgeTokenV2.getAddress())
    const BridgeTokenV2Proxied = BridgeTokenV2Instance.attach(tokenProxyAddress)
    expect(await BridgeTokenV2Proxied.returnTestString()).to.equal('test')
    expect(await BridgeTokenV2Proxied.name()).to.equal('Wrapped NEAR fungible token')
    expect(await BridgeTokenV2Proxied.symbol()).to.equal('wNEAR')
    expect((await BridgeTokenV2Proxied.decimals()).toString()).to.equal('24')
  })

  it('user cant upgrade token contract', async function () {
    await createToken(wrappedNearId);

    const BridgeTokenV2Instance = await ethers.getContractFactory("TestBridgeToken");
    const BridgeTokenV2 = await BridgeTokenV2Instance.deploy();
    await BridgeTokenV2.waitForDeployment();

    await expect(BridgeTokenFactory.connect(user1).upgradeToken(wrappedNearId, await BridgeTokenV2.getAddress()))
      .to.be.revertedWithCustomError(BridgeTokenFactory, 'AccessControlUnauthorizedAccount');
  })

  it('Test selective pause', async function () {
    // Pause withdraw
    await expect(
      BridgeTokenFactory.pause(PauseMode.PausedInitTransfer)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.PausedInitTransfer);
    expect(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.PausedInitTransfer);

    // Pause withdraw again
    await expect(
      BridgeTokenFactory.pause(PauseMode.PausedInitTransfer)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.PausedInitTransfer);
    expect(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.PausedInitTransfer);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedFinTransfer)).to.be.equal(false);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.equal(true);

    // Pause deposit
    await expect(
      BridgeTokenFactory.pause(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
    expect(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);

    // Pause deposit again
    await expect(
      BridgeTokenFactory.pause(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
    expect(await BridgeTokenFactory.pausedFlags())
      .to
      .be
      .equal(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);

    // Pause deposit and withdraw
    await expect(
      BridgeTokenFactory.pause(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
    expect(await BridgeTokenFactory.pausedFlags())
      .to
      .be
      .equal(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedFinTransfer)).to.be.equal(true);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.equal(true);

    // Unpause all
    await expect(
      BridgeTokenFactory.pause(PauseMode.UnpausedAll)
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.UnpausedAll);
    expect(await BridgeTokenFactory.pausedFlags()).to.be.equal(PauseMode.UnpausedAll);

    // Pause all
    await expect(
      BridgeTokenFactory.pauseAll()
    )
      .to
      .emit(BridgeTokenFactory, 'Paused')
      .withArgs(adminAccount.address, PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
    expect(await BridgeTokenFactory.pausedFlags())
      .to
      .be
      .equal(PauseMode.PausedFinTransfer | PauseMode.PausedInitTransfer);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedFinTransfer)).to.be.equal(true);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.equal(true);
  })

  it("Test grant admin role", async function() {
    await BridgeTokenFactory.connect(adminAccount).pause(PauseMode.UnpausedAll);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.false;

    await BridgeTokenFactory.connect(adminAccount).pauseAll();
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;

    const signers = await ethers.getSigners();
    const newAdminAccount = signers[2];
    const DEFAULT_ADMIN_ROLE = "0x0000000000000000000000000000000000000000000000000000000000000000";
    await expect(
      BridgeTokenFactory.connect(newAdminAccount).pause(PauseMode.UnpausedAll)
    ).to.be.revertedWithCustomError(BridgeTokenFactory, 'AccessControlUnauthorizedAccount');
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;

    // Grant DEFAULT_ADMIN_ROLE to newAdminAccount
    await expect(
      BridgeTokenFactory.grantRole(DEFAULT_ADMIN_ROLE, newAdminAccount.address)
    )
      .to
      .emit(BridgeTokenFactory, "RoleGranted")
      .withArgs(
        DEFAULT_ADMIN_ROLE,
        newAdminAccount.address,
        adminAccount.address
      );
    await BridgeTokenFactory.connect(newAdminAccount).pause(PauseMode.UnpausedAll);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.false;

    await BridgeTokenFactory.connect(newAdminAccount).pause(PauseAll);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;

    // Revoke DEFAULT_ADMIN_ROLE from adminAccount
    await expect(
      BridgeTokenFactory
        .connect(newAdminAccount)
        .revokeRole(
          DEFAULT_ADMIN_ROLE,
          adminAccount.address
        )
    )
      .to
      .emit(BridgeTokenFactory, "RoleRevoked")
      .withArgs(
        DEFAULT_ADMIN_ROLE,
        adminAccount.address,
        newAdminAccount.address
      );

    // Check tx reverted on call from revoked adminAccount
    await expect(
      BridgeTokenFactory.connect(adminAccount).pause(PauseMode.UnpausedAll)
    ).to.be.revertedWithCustomError(BridgeTokenFactory, 'AccessControlUnauthorizedAccount');
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;

    // Check newAdminAccount can perform admin calls
    await BridgeTokenFactory.connect(newAdminAccount).pause(PauseMode.UnpausedAll);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.false;
    await BridgeTokenFactory.connect(newAdminAccount).pause(PauseAll);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;

    // Check newAdminAccount can grant DEFAULT_ADMIN_ROLE to adminAccount
    await expect(
      BridgeTokenFactory
        .connect(newAdminAccount)
        .grantRole(DEFAULT_ADMIN_ROLE, adminAccount.address)
    )
      .to
      .emit(BridgeTokenFactory, "RoleGranted")
      .withArgs(
        DEFAULT_ADMIN_ROLE,
        adminAccount.address,
        newAdminAccount.address
      );

    // Check that adminAccount can perform admin calls again
    await BridgeTokenFactory.connect(adminAccount).pause(PauseMode.UnpausedAll);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.false;
    await BridgeTokenFactory.connect(adminAccount).pause(PauseAll);
    expect(await BridgeTokenFactory.paused(PauseMode.PausedInitTransfer)).to.be.true;
  });
})
