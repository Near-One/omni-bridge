import type { HardhatEthersSigner } from "@nomicfoundation/hardhat-ethers/signers"
import { ethers, upgrades } from "hardhat"
import type { TrustedRelayerRegistry } from "../../typechain-types"

export async function setupTrustedRelayers(
  bridgeAddress: string,
  admin: HardhatEthersSigner,
  relayers: string[],
): Promise<TrustedRelayerRegistry> {
  const registryFactory = await ethers.getContractFactory("TrustedRelayerRegistry")
  const registry = (await upgrades.deployProxy(registryFactory, [admin.address, 0, 0], {
    initializer: "initialize",
  })) as unknown as TrustedRelayerRegistry
  await registry.waitForDeployment()

  const trustedRelayerRole = await registry.TRUSTED_RELAYER_ROLE()
  for (const relayer of relayers) {
    await registry.connect(admin).grantRole(trustedRelayerRole, relayer)
  }

  const bridge = await ethers.getContractAt("OmniBridge", bridgeAddress)
  await bridge.connect(admin).setTrustedRelayerRegistry(await registry.getAddress())

  return registry
}
