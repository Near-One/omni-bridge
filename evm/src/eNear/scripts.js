const eNearAbi = [{"inputs":[{"internalType":"string","name":"_tokenName","type":"string"},{"internalType":"string","name":"_tokenSymbol","type":"string"},{"internalType":"bytes","name":"_nearConnector","type":"bytes"},{"internalType":"contract INearProver","name":"_prover","type":"address"},{"internalType":"uint64","name":"_minBlockAcceptanceHeight","type":"uint64"},{"internalType":"address","name":"_admin","type":"address"},{"internalType":"uint256","name":"_pausedFlags","type":"uint256"}],"stateMutability":"nonpayable","type":"constructor"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"owner","type":"address"},{"indexed":true,"internalType":"address","name":"spender","type":"address"},{"indexed":false,"internalType":"uint256","name":"value","type":"uint256"}],"name":"Approval","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"bytes32","name":"_receiptId","type":"bytes32"}],"name":"ConsumedProof","type":"event"},{"anonymous":false,"inputs":[{"indexed":false,"internalType":"uint128","name":"amount","type":"uint128"},{"indexed":true,"internalType":"address","name":"recipient","type":"address"}],"name":"NearToEthTransferFinalised","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"from","type":"address"},{"indexed":true,"internalType":"address","name":"to","type":"address"},{"indexed":false,"internalType":"uint256","name":"value","type":"uint256"}],"name":"Transfer","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"sender","type":"address"},{"indexed":false,"internalType":"uint256","name":"amount","type":"uint256"},{"indexed":false,"internalType":"string","name":"accountId","type":"string"}],"name":"TransferToNearInitiated","type":"event"},{"inputs":[],"name":"admin","outputs":[{"internalType":"address","name":"","type":"address"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"target","type":"address"},{"internalType":"bytes","name":"data","type":"bytes"}],"name":"adminDelegatecall","outputs":[{"internalType":"bytes","name":"","type":"bytes"}],"stateMutability":"payable","type":"function"},{"inputs":[{"internalType":"uint256","name":"flags","type":"uint256"}],"name":"adminPause","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[],"name":"adminReceiveEth","outputs":[],"stateMutability":"payable","type":"function"},{"inputs":[{"internalType":"address payable","name":"destination","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"adminSendEth","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"uint256","name":"key","type":"uint256"},{"internalType":"uint256","name":"value","type":"uint256"}],"name":"adminSstore","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"owner","type":"address"},{"internalType":"address","name":"spender","type":"address"}],"name":"allowance","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"spender","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"approve","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"account","type":"address"}],"name":"balanceOf","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"decimals","outputs":[{"internalType":"uint8","name":"","type":"uint8"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"spender","type":"address"},{"internalType":"uint256","name":"subtractedValue","type":"uint256"}],"name":"decreaseAllowance","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"bytes","name":"proofData","type":"bytes"},{"internalType":"uint64","name":"proofBlockHeight","type":"uint64"}],"name":"finaliseNearToEthTransfer","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"spender","type":"address"},{"internalType":"uint256","name":"addedValue","type":"uint256"}],"name":"increaseAllowance","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[],"name":"minBlockAcceptanceHeight","outputs":[{"internalType":"uint64","name":"","type":"uint64"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"name","outputs":[{"internalType":"string","name":"","type":"string"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"nearConnector","outputs":[{"internalType":"bytes","name":"","type":"bytes"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"paused","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"prover","outputs":[{"internalType":"contract INearProver","name":"","type":"address"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"symbol","outputs":[{"internalType":"string","name":"","type":"string"}],"stateMutability":"view","type":"function"},{"inputs":[],"name":"totalSupply","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"view","type":"function"},{"inputs":[{"internalType":"address","name":"recipient","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"transfer","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"address","name":"sender","type":"address"},{"internalType":"address","name":"recipient","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"transferFrom","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"uint256","name":"_amount","type":"uint256"},{"internalType":"string","name":"_nearReceiverAccountId","type":"string"}],"name":"transferToNear","outputs":[],"stateMutability":"nonpayable","type":"function"},{"inputs":[{"internalType":"bytes32","name":"","type":"bytes32"}],"name":"usedProofs","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"view","type":"function"}]

task('deploy-e-near-proxy', 'Deploys the ENearProxy contract')
    .addParam('enear', 'Address of eNear contract')
    .setAction(async (taskArgs, hre) => {
        const { ethers, upgrades } = hre;

        const eNear = await ethers.getContractAt(eNearAbi, taskArgs.enear);
        const nearConnector =  await eNear.nearConnector();

        const eNearProxyContract =
            await ethers.getContractFactory("ENearProxy");
        const eNearProxy = await upgrades.deployProxy(
            eNearProxyContract, [
                taskArgs.enear,
                nearConnector,
                0
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

task('e-near-set-admin', 'Set the proxy as admin for eNear')
    .addParam('newAdmin', 'New admin address')
    .addParam('enear', 'Address of the eNear contract')
    .setAction(async (taskArgs, hre) => {
        const { ethers } = hre;
        const eNear = await ethers.getContractAt(eNearAbi, taskArgs.enear);
        await eNear.adminSstore(9, ethers.zeroPadValue(taskArgs.newAdmin, 32));
    });

task('deploy-fake-prover', 'Deploy fake prover')
    .setAction(async (_taskArgs, hre) => {
        const { ethers } = hre;
        const FakeProverContractFactory = await ethers.getContractFactory("FakeProver");
        const FakeProverContract = await FakeProverContractFactory.deploy();
        await FakeProverContract.waitForDeployment();

        console.log(`FakeProver deployed at ${await FakeProverContract.getAddress()}`);
    });

task('e-near-set-prover', 'Set new prover for eNear')
    .addParam('enear', 'Address of the eNear contract')
    .addParam('newProver', 'Address of the new prover contract')
    .setAction(async (taskArgs, hre) => {
        const { ethers } = hre;

        const eNear = await ethers.getContractAt(eNearAbi, taskArgs.enear);
        let slotValue = await ethers.provider.getStorage(await eNear.getAddress(), 5);
        slotValue = (taskArgs.newProver).concat(slotValue.slice(-2));

        await eNear.adminSstore(5, ethers.zeroPadValue(slotValue, 32));
    });
