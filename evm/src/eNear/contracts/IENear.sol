// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

//eNear on mainnet: https://etherscan.io/address/0x85F17Cf997934a597031b2E18a9aB6ebD4B9f6a4#code
interface IENear {
    function transferToNear(uint256 _amount, string calldata _nearReceiverAccountId) external;
    function finaliseNearToEthTransfer(bytes calldata proofData, uint64 proofBlockHeight) external;
    function nearConnector() external view returns(bytes memory);
    function adminSstore(uint key, uint value) external;
    function balanceOf(address account) external view returns (uint256);
    function totalSupply() external view returns (uint256);
    function transfer(address recipient, uint256 amount) external returns (bool);
    function prover() external view returns (address);
    function admin() external view returns (address);
    function adminPause(uint256 flags) external;
}
