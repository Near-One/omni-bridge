const { expect } = require('chai');
const { ethers, upgrades} = require('hardhat');

const UNPAUSED_ALL = 0
const PAUSE_FINALISE_FROM_NEAR = 1 << 0;
const PAUSE_TRANSFER_TO_NEAR = 1 << 1;

describe('eNearProxy contract', () => {
  let deployer;
  let eNearAdmin;
  let alice;
  let bob;
  let nearProver;
  let eNear;
  let eNearProxy;

  const ERC20_NAME = 'eNear';
  const ERC20_SYMBOL = 'eNear';

  beforeEach(async () => {
    [deployer, eNearAdmin, alice, bob] = await ethers.getSigners();

    nearProverMockContractFactory = await ethers.getContractFactory('FakeProver')
    nearProver = await nearProverMockContractFactory.deploy();
    await nearProver.waitForDeployment();

    // Proofs coming from blocks below this value should be rejected
    minBlockAcceptanceHeight = 0;

    eNearContractFactory = await ethers.getContractFactory('src/eNear/contracts/ENear.sol:ENear');
    eNear = await eNearContractFactory
      .deploy(
        ERC20_NAME,
        ERC20_SYMBOL,
        Buffer.from('eNearBridge', 'utf-8'),
        await nearProver.getAddress(),
        minBlockAcceptanceHeight,
        eNearAdmin.address,
        UNPAUSED_ALL
    );

    await eNear.waitForDeployment();

    eNearProxyFactory = await ethers.getContractFactory('ENearProxy');
    eNearProxy = await upgrades.deployProxy(eNearProxyFactory, [
      await eNear.getAddress(),
      Buffer.from('eNearBridge', 'utf-8'),
      0
    ], { initializer: 'initialize' });

    await eNearProxy.waitForDeployment();

  });


  describe('transferToNear()', () => {
    it('Mint by using eNearProxy', async () => {
      await eNearProxy.connect(deployer).grantRole(ethers.keccak256(ethers.toUtf8Bytes("MINTER_ROLE")), alice.address);
      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100);
      expect(await eNear.balanceOf(alice.address)).to.equal(100);
    })

    it('Two mints by using eNearProxy', async () => {
      await eNearProxy.connect(deployer).grantRole(ethers.keccak256(ethers.toUtf8Bytes("MINTER_ROLE")), alice.address);
      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100);
      expect(await eNear.balanceOf(alice.address)).to.equal(100);

      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100);
      expect(await eNear.balanceOf(alice.address)).to.equal(200);
    })

    it('Burn by using eNearProxy', async () => {
      await eNearProxy.connect(deployer).grantRole(ethers.keccak256(ethers.toUtf8Bytes("MINTER_ROLE")), alice.address);
      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100);
      expect(await eNear.balanceOf(alice.address)).to.equal(100);
      expect(await eNear.totalSupply()).to.equal(100)

      await eNear.connect(alice).transfer(await eNearProxy.getAddress(), 100);

      expect(await eNear.totalSupply()).to.equal(100);

      await eNearProxy.connect(alice).burn(await eNear.getAddress(), 100);

      expect(await eNear.totalSupply()).to.equal(0);
    })

    it('Set Fake Prover', async () => {
      nearProverMockContractFactory = await ethers.getContractFactory('FakeProver')
      fakeProver = await nearProverMockContractFactory.deploy();
      await fakeProver.waitForDeployment();

      expect(await eNear.prover()).to.equal(await nearProver.getAddress());
      await eNear.connect(eNearAdmin).adminSstoreWithMask(5, await fakeProver.getAddress(), "0x000000000000000000000000ffffffffffffffffffffffffffffffffffffffff");
      expect(await eNear.prover()).to.equal(await fakeProver.getAddress());
    })

    it('Set Proxy as Admin', async () => {
      expect(await eNear.admin()).to.equal(await eNearAdmin.getAddress());
      await eNear.connect(eNearAdmin).adminSstoreWithMask(9, await eNearProxy.getAddress(), "0x000000000000000000000000ffffffffffffffffffffffffffffffffffffffff");
      expect(await eNear.admin()).to.equal(await eNearProxy.getAddress());
    })

    it('Pause All', async () => {
      await eNear.connect(eNearAdmin).adminPause(PAUSE_TRANSFER_TO_NEAR | PAUSE_FINALISE_FROM_NEAR);

      await eNearProxy.connect(deployer).grantRole(ethers.keccak256(ethers.toUtf8Bytes("MINTER_ROLE")), alice.address);
      await expect(eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100)).to.be.reverted;
      expect(await eNear.balanceOf(alice.address)).to.equal(0);

      await eNear.connect(eNearAdmin).adminSstoreWithMask(9, await eNearProxy.getAddress(), "0x000000000000000000000000ffffffffffffffffffffffffffffffffffffffff");
      await eNearProxy.connect(alice).mint(await eNear.getAddress(), alice.address, 100);
      expect(await eNear.balanceOf(alice.address)).to.equal(100);
    })
  })
})
