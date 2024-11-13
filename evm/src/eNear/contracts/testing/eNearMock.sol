// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

import "rainbow-bridge-sol/nearprover/contracts/INearProver.sol";
import { ENear } from "../ENear.sol";

contract eNearMock is ENear {

    constructor(
        string memory _tokenName,
        string memory _tokenSymbol,
        bytes memory _nearConnector,
        INearProver _prover,
        uint64 _minBlockAcceptanceHeight,
        address _admin,
        uint256 _pausedFlags
    ) ENear(_tokenName, _tokenSymbol, _nearConnector, _prover, _minBlockAcceptanceHeight, _admin, _pausedFlags)
    {
    }

    function mintTo(address _recipient, uint256 _amount) external {
        _mint(_recipient, _amount);
    }
}
