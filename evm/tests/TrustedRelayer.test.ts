import type { BridgeToken, OmniBridge, TrustedRelayerRegistry } from "../typechain-types"

import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import { time } from "@nomicfoundation/hardhat-network-helpers"
import { expect } from "chai"
import { ethers, upgrades } from "hardhat"
import { depositSignature, metadataSignature, testWallet } from "./helpers/signatures"

describe("TrustedRelayerRegistry", () => {
  const wrappedNearId = "wrap.testnet"
  const STAKE = ethers.parseEther("1")
  const WAITING_PERIOD = 7 * 24 * 60 * 60

  let OmniBridge: OmniBridge
  let Registry: TrustedRelayerRegistry
  let tokenAddress: string
  let admin: HardhatEthersSigner
  let relayer: HardhatEthersSigner
  let manager: HardhatEthersSigner
  let user: HardhatEthersSigner

  async function deployBridge(): Promise<OmniBridge> {
    const bridgeTokenFactory = await ethers.getContractFactory("BridgeToken")
    const bridgeToken = await bridgeTokenFactory.deploy()
    await bridgeToken.waitForDeployment()

    const omniBridgeFactory = await ethers.getContractFactory("OmniBridge")
    const bridge = await upgrades.deployProxy(
      omniBridgeFactory,
      [await bridgeToken.getAddress(), testWallet.address, 0],
      { initializer: "initialize" },
    )
    return (await bridge.waitForDeployment()) as unknown as OmniBridge
  }

  async function createToken(bridge: OmniBridge): Promise<string> {
    const { signature, payload } = metadataSignature(wrappedNearId)
    await bridge.deployToken(signature, payload)
    return bridge.nearToEthToken(wrappedNearId)
  }

  function finTransfer(from: HardhatEthersSigner) {
    const { signature, payload } = depositSignature(tokenAddress, user.address)
    return OmniBridge.connect(from).finTransfer(signature, payload)
  }

  async function applyAsRelayer() {
    await Registry.setRelayerConfig(STAKE, WAITING_PERIOD)
    await Registry.connect(relayer).applyForTrustedRelayer({ value: STAKE })
  }

  beforeEach(async () => {
    ;[admin, relayer, manager, user] = await ethers.getSigners()

    OmniBridge = await deployBridge()
    tokenAddress = await createToken(OmniBridge)

    const registryFactory = await ethers.getContractFactory("TrustedRelayerRegistry")
    Registry = (await upgrades.deployProxy(registryFactory, [admin.address], {
      initializer: "initialize",
    })) as unknown as TrustedRelayerRegistry
    await Registry.waitForDeployment()

    await OmniBridge.setTrustedRelayerRegistry(await Registry.getAddress())
  })

  describe("bridge integration", () => {
    it("can't fin transfer when the registry is not set", async () => {
      OmniBridge = await deployBridge()
      tokenAddress = await createToken(OmniBridge)

      await expect(finTransfer(admin)).to.be.revertedWithCustomError(
        OmniBridge,
        "NotTrustedRelayer",
      )
    })

    it("can't fin transfer as an untrusted relayer", async () => {
      await expect(finTransfer(relayer)).to.be.revertedWithCustomError(
        OmniBridge,
        "NotTrustedRelayer",
      )
    })

    it("can fin transfer as a relayer granted by the admin", async () => {
      await Registry.grantRole(await Registry.TRUSTED_RELAYER_ROLE(), relayer.address)

      await expect(finTransfer(relayer)).to.emit(OmniBridge, "FinTransfer")

      const token = (await ethers.getContractAt("BridgeToken", tokenAddress)) as BridgeToken
      expect(await token.balanceOf(user.address)).to.equal(1)
    })

    it("can't fin transfer after the role is revoked", async () => {
      const role = await Registry.TRUSTED_RELAYER_ROLE()
      await Registry.grantRole(role, relayer.address)
      await Registry.revokeRole(role, relayer.address)

      await expect(finTransfer(relayer)).to.be.revertedWithCustomError(
        OmniBridge,
        "NotTrustedRelayer",
      )
    })

    it("only the admin can set the registry", async () => {
      await expect(
        OmniBridge.connect(user).setTrustedRelayerRegistry(user.address),
      ).to.be.revertedWithCustomError(OmniBridge, "AccessControlUnauthorizedAccount")
    })
  })

  describe("staking", () => {
    it("can't apply while staking is disabled", async () => {
      await expect(
        Registry.connect(relayer).applyForTrustedRelayer({ value: STAKE }),
      ).to.be.revertedWithCustomError(Registry, "RelayerStakingDisabled")
    })

    it("only the admin can set the relayer config", async () => {
      await expect(
        Registry.connect(user).setRelayerConfig(STAKE, WAITING_PERIOD),
      ).to.be.revertedWithCustomError(Registry, "AccessControlUnauthorizedAccount")

      await expect(Registry.setRelayerConfig(STAKE, WAITING_PERIOD))
        .to.emit(Registry, "RelayerConfigSet")
        .withArgs(STAKE, WAITING_PERIOD)
      const config = await Registry.relayerConfig()
      expect(config.stakeRequired).to.equal(STAKE)
      expect(config.waitingPeriod).to.equal(WAITING_PERIOD)
    })

    it("can't apply with a wrong stake", async () => {
      await Registry.setRelayerConfig(STAKE, WAITING_PERIOD)

      await expect(Registry.connect(relayer).applyForTrustedRelayer({ value: STAKE - 1n }))
        .to.be.revertedWithCustomError(Registry, "InvalidRelayerStake")
        .withArgs(STAKE - 1n, STAKE)
      await expect(Registry.connect(relayer).applyForTrustedRelayer({ value: STAKE + 1n }))
        .to.be.revertedWithCustomError(Registry, "InvalidRelayerStake")
        .withArgs(STAKE + 1n, STAKE)
    })

    it("staked relayer becomes trusted after the waiting period", async () => {
      await Registry.setRelayerConfig(STAKE, WAITING_PERIOD)

      const tx = Registry.connect(relayer).applyForTrustedRelayer({ value: STAKE })
      await expect(tx).to.changeEtherBalances([relayer, Registry], [-STAKE, STAKE])
      const activateAt = (await time.latest()) + WAITING_PERIOD
      await expect(tx)
        .to.emit(Registry, "RelayerApplied")
        .withArgs(relayer.address, STAKE, activateAt)

      expect(await Registry.isTrustedRelayer(relayer.address)).to.equal(false)
      await expect(finTransfer(relayer)).to.be.revertedWithCustomError(
        OmniBridge,
        "NotTrustedRelayer",
      )

      await time.increaseTo(activateAt)

      expect(await Registry.isTrustedRelayer(relayer.address)).to.equal(true)
      await expect(finTransfer(relayer)).to.emit(OmniBridge, "FinTransfer")
    })

    it("can't apply twice", async () => {
      await applyAsRelayer()

      await expect(
        Registry.connect(relayer).applyForTrustedRelayer({ value: STAKE }),
      ).to.be.revertedWithCustomError(Registry, "RelayerApplicationExists")
    })

    it("can't resign while the application is pending", async () => {
      await applyAsRelayer()

      await expect(Registry.connect(relayer).resignTrustedRelayer()).to.be.revertedWithCustomError(
        Registry,
        "RelayerNotActive",
      )
    })

    it("can't resign without an application", async () => {
      await expect(Registry.connect(relayer).resignTrustedRelayer()).to.be.revertedWithCustomError(
        Registry,
        "RelayerNotFound",
      )
    })

    it("active relayer can resign and get the original stake back", async () => {
      await applyAsRelayer()
      await time.increase(WAITING_PERIOD)
      await Registry.setRelayerConfig(STAKE * 2n, 0)

      const tx = Registry.connect(relayer).resignTrustedRelayer()
      await expect(tx).to.changeEtherBalances([relayer, Registry], [STAKE, -STAKE])
      await expect(tx).to.emit(Registry, "RelayerResigned").withArgs(relayer.address, STAKE)

      expect(await Registry.isTrustedRelayer(relayer.address)).to.equal(false)
      await expect(finTransfer(relayer)).to.be.revertedWithCustomError(
        OmniBridge,
        "NotTrustedRelayer",
      )
    })

    it("manager can reject an application and takes the stake", async () => {
      await Registry.grantRole(await Registry.RELAYER_MANAGER_ROLE(), manager.address)
      await applyAsRelayer()

      const tx = Registry.connect(manager).rejectRelayerApplication(relayer.address)
      await expect(tx).to.changeEtherBalances([manager, Registry, relayer], [STAKE, -STAKE, 0])
      await expect(tx)
        .to.emit(Registry, "RelayerRejected")
        .withArgs(relayer.address, STAKE, manager.address)

      const state = await Registry.relayers(relayer.address)
      expect(state.stake).to.equal(0)
      expect(state.activateAt).to.equal(0)

      await time.increase(WAITING_PERIOD)
      expect(await Registry.isTrustedRelayer(relayer.address)).to.equal(false)
    })

    it("admin can reject an active relayer", async () => {
      await applyAsRelayer()
      await time.increase(WAITING_PERIOD)

      await expect(Registry.rejectRelayerApplication(relayer.address)).to.changeEtherBalances(
        [admin, Registry],
        [STAKE, -STAKE],
      )
      expect(await Registry.isTrustedRelayer(relayer.address)).to.equal(false)
    })

    it("rejected relayer can apply again", async () => {
      await applyAsRelayer()
      await Registry.rejectRelayerApplication(relayer.address)

      await expect(Registry.connect(relayer).applyForTrustedRelayer({ value: STAKE })).to.emit(
        Registry,
        "RelayerApplied",
      )
    })

    it("only a manager or the admin can reject", async () => {
      await applyAsRelayer()

      await expect(
        Registry.connect(user).rejectRelayerApplication(relayer.address),
      ).to.be.revertedWithCustomError(Registry, "AccessControlUnauthorizedAccount")
    })

    it("can't reject an unknown relayer", async () => {
      await expect(
        Registry.rejectRelayerApplication(relayer.address),
      ).to.be.revertedWithCustomError(Registry, "RelayerNotFound")
    })
  })
})
