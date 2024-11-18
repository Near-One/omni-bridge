import { task } from "hardhat/config"
import type { HardhatRuntimeEnvironment } from "hardhat/types"

task("deploy-e-near-proxy", "Deploys the ENearProxy contract")
	.addParam("enear", "Address of eNear contract")
	.setAction(async (taskArgs, hre: HardhatRuntimeEnvironment) => {
		const { ethers, upgrades } = hre

		const eNear = await ethers.getContractAt("IENear", taskArgs.enear)
		const nearConnector = await eNear.nearConnector()

		const eNearProxyContract = await ethers.getContractFactory("ENearProxy")
		const eNearProxy = await upgrades.deployProxy(
			eNearProxyContract,
			[taskArgs.enear, nearConnector, 0],
			{
				initializer: "initialize",
				timeout: 0,
			},
		)

		await eNearProxy.waitForDeployment()
		console.log(`eNearProxy deployed at ${await eNearProxy.getAddress()}`)
		console.log(
			"Implementation address:",
			await upgrades.erc1967.getImplementationAddress(await eNearProxy.getAddress()),
		)
	})

task("e-near-set-admin", "Set the proxy as admin for eNear")
	.addParam("newAdmin", "New admin address")
	.addParam("enear", "Address of the eNear contract")
	.setAction(async (taskArgs, hre: HardhatRuntimeEnvironment) => {
		const { ethers } = hre
		const eNear = await ethers.getContractAt("IENear", taskArgs.enear)
		await eNear.adminSstore(9, ethers.zeroPadValue(taskArgs.newAdmin, 32))
	})

task("deploy-fake-prover", "Deploy fake prover").setAction(
	async (_taskArgs, hre: HardhatRuntimeEnvironment) => {
		const { ethers } = hre
		const FakeProverContractFactory = await ethers.getContractFactory("FakeProver")
		const FakeProverContract = await FakeProverContractFactory.deploy()
		await FakeProverContract.waitForDeployment()

		console.log(`FakeProver deployed at ${await FakeProverContract.getAddress()}`)
	},
)
