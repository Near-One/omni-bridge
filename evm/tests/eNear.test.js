const { expect } = require('chai');

const { serialize } = require('rainbow-bridge-lib/borsh.js');
const { borshifyOutcomeProof } = require('rainbow-bridge-lib/borshify-proof.js');

const { ethers } = require('hardhat');

const proof_template = require('./proof_template.json');

const SCHEMA = {
  'MigrateNearToEthereum': {
    kind: 'struct', fields: [
      ['flag', 'u8'],
      ['amount', 'u128'],
      ['recipient', [20]],
    ]
  }
};

const UNPAUSED_ALL = 0

describe('eNear contract', () => {
  let deployer;
  let eNearAdmin;
  let alice;
  let bob;
  let nearProver;
  let eNear;

  const ERC20_NAME = 'eNear';
  const ERC20_SYMBOL = 'eNear';

  const ONE_HUNDRED_TOKENS = ethers.toBigInt(100) * (ethers.toBigInt(10) ** (ethers.toBigInt(24)))

  beforeEach(async () => {
    [deployer, eNearAdmin, alice, bob] = await ethers.getSigners();

    nearProverMockContractFactory = await ethers.getContractFactory('FakeProver')
    nearProver = await nearProverMockContractFactory.deploy();
    await nearProver.waitForDeployment();

    // Proofs coming from blocks below this value should be rejected
    minBlockAcceptanceHeight = 0;

    eNearContractFactory = await ethers.getContractFactory('eNearMock');
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
  });


  describe('transferToNear()', () => {
    it('Burns eNear when transferring to near', async () => {
      // check supply zero and balance zero
      expect(await eNear.totalSupply()).to.equal(0)
      expect(await eNear.balanceOf(alice.address)).to.equal(0)

      // mint some tokens to account bridging
      await eNear.mintTo(alice.address, ONE_HUNDRED_TOKENS)

      // check supply and balance
      expect(await eNear.totalSupply()).to.equal(ONE_HUNDRED_TOKENS)
      expect(await eNear.balanceOf(alice.address)).to.equal(ONE_HUNDRED_TOKENS)

      // call xfer to near
      await expect(
        eNear
          .connect(alice)
          .transferToNear(ONE_HUNDRED_TOKENS, 'vince.near')
      )
        .to
        .emit(eNear, 'TransferToNearInitiated')
        .withArgs(alice.address, ONE_HUNDRED_TOKENS, 'vince.near');

      // check supply zero and balance zero
      expect(await eNear.totalSupply()).to.equal(0)
      expect(await eNear.balanceOf(alice.address)).to.equal(0)
    })
  })

  describe('finaliseNearToEthTransfer()', () => {
    it('Mints eNear after bridging Near', async () => {
      let proof = JSON.parse(JSON.stringify(proof_template));

      const amount = ethers.parseUnits('1', 24);
      proof.outcome_proof.outcome.status.SuccessValue = serialize(SCHEMA, 'MigrateNearToEthereum', {
        flag: 0,
        amount: amount.toString(),
        recipient: ethers.getBytes(bob.address),
      }).toString('base64');

      const receiverBalance = await eNear.balanceOf(bob.address);

      await eNear.finaliseNearToEthTransfer(borshifyOutcomeProof(proof), 1099);

      const newReceiverBalance = await eNear.balanceOf(bob.address);
      expect((newReceiverBalance - receiverBalance).toString()).to.be.equal(amount.toString());
    });

    it('Reverts when reusing proof event', async () => {
      let proof = JSON.parse(JSON.stringify(proof_template));
      const amount = ethers.parseUnits('1', 24);
      proof.outcome_proof.outcome.status.SuccessValue = serialize(SCHEMA, 'MigrateNearToEthereum', {
        flag: 0,
        amount: amount.toString(),
        recipient: ethers.getBytes(bob.address),
      }).toString('base64');

      await eNear.finaliseNearToEthTransfer(borshifyOutcomeProof(proof), 1099);

      await expect(
        eNear.finaliseNearToEthTransfer(borshifyOutcomeProof(proof), 1099)
      )
        .to
        .be
        .revertedWith("The burn event proof cannot be reused");
    })

    it('Reverts when event comes from the wrong executor', async () => {
      let proof = JSON.parse(JSON.stringify(proof_template));
      proof.outcome_proof.outcome.executor_id = 'eNearBridgeInvalid'

      const amount = ethers.parseUnits('1', 24);
      proof.outcome_proof.outcome.status.SuccessValue = serialize(SCHEMA, 'MigrateNearToEthereum', {
        flag: 0,
        amount: amount.toString(),
        recipient: ethers.getBytes(bob.address),
      }).toString('base64');

      await expect(
        eNear.finaliseNearToEthTransfer(borshifyOutcomeProof(proof), 1099)
      )
        .to
        .be
        .revertedWith("Can only unlock tokens from the linked proof producer on Near blockchain");
    })

    it('Reverts if flag is not zero', async () => {
      let proof = JSON.parse(JSON.stringify(proof_template));

      const amount = ethers.parseUnits('1', 24);
      proof.outcome_proof.outcome.status.SuccessValue = serialize(SCHEMA, 'MigrateNearToEthereum', {
        flag: 3,
        amount: amount.toString(),
        recipient: ethers.getBytes(bob.address),
      }).toString('base64');

      await expect(
        eNear.finaliseNearToEthTransfer(borshifyOutcomeProof(proof), 1099)
      )
        .to
        .be
        .revertedWith("ERR_NOT_WITHDRAW_RESULT");
    })
  })
})
