/* task('deploy-e-near-proxy', 'Deploys the ENearProxy contract')
    .setAction(async () => {
      const { ethers, upgrades } = hre;
  
      const eNearProxyContract = 
        await ethers.getContractFactory("ENearProxy");
      const eNearProxy = await upgrades.deployProxy(
        eNearProxyContract,
        [
          taskArgs.eNear,
        ],
        {
          initializer: "initialize",
          timeout: 0,
        },
      );
  
      await eNearProxy.waitForDeployment();
      console.log(`eNearProxy deployed at ${await eNearProxy.getAddress()}`);
      console.log(
        "Implementation address:",
        await upgrades.erc1967.getImplementationAddress(
          await eNearProxy.getAddress(),
        ),
      );
    });

task('set-proxy-as-admin', 'Set the proxy as admin for eNear')
    .addParam('proxy', 'Address of the proxy to set as admin')
    .addParam('eNear', 'Address of the eNear contract')
    .setAction(async (taskArgs, hre) => {
        const { ethers } = hre;
        const eNear = await ethers.getContractAt('ENear', taskArgs.eNear);
        await eNear.nominateAdmin(taskArgs.proxy);
        await eNear.acceptAdmin(taskArgs.proxy);
    });

task('set-fake-prover', 'Set the fake prover for eNear')
    .addParam('eNear', 'Address of the eNear contract')
    .setAction(async (taskArgs, hre) => {
        const { ethers } = hre;

        const FakeProverContractFactory = await ethers.getContractFactory("FakeProver");
        const FakeProverContract = await FakeProverContractFactory.deploy();
        await FakeProverContract.waitForDeployment();

        const eNear = await ethers.getContractAt('ENear', taskArgs.eNear);
        eNear.adminSstore(5, uint256(await FakeProverContract.getAddress()));
    }); */