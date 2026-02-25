import type { HardhatRuntimeEnvironment } from "hardhat/types"

/**
 * Returns true if the current network is a ZKsync network.
 */
export function isZkSyncNetwork(hre: HardhatRuntimeEnvironment): boolean {
  return "zksync" in hre.network.config && hre.network.config.zksync === true
}

// ERC-1967 implementation slot: bytes32(uint256(keccak256('eip1967.proxy.implementation')) - 1)
const IMPLEMENTATION_SLOT = "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"

/**
 * Gets the implementation address of a UUPS/transparent proxy.
 * On standard networks, uses upgrades.erc1967.getImplementationAddress.
 * On ZKsync networks, reads the ERC-1967 implementation slot directly
 * since the ZKsync upgrades plugin does not implement erc1967 helpers.
 */
export async function getProxyImplementationAddress(
  hre: HardhatRuntimeEnvironment,
  proxyAddress: string,
): Promise<string> {
  if (isZkSyncNetwork(hre)) {
    const storageValue = await hre.ethers.provider.getStorage(proxyAddress, IMPLEMENTATION_SLOT)
    return hre.ethers.getAddress(`0x${storageValue.slice(26)}`)
  }
  return hre.upgrades.erc1967.getImplementationAddress(proxyAddress)
}
