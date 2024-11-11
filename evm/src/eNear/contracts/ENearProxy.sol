// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

import {AccessControlUpgradeable} from '@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol';
import {UUPSUpgradeable} from '@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol';
import {ENear} from './ENear.sol';
import {ICustomMinter} from '../../common/ICustomMinter.sol';

contract ENearProxy is UUPSUpgradeable, AccessControlUpgradeable, ICustomMinter {
    ENear public eNear;

    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(address _eNear) public initializer {
        __UUPSUpgradeable_init();
        __AccessControl_init();
        eNear = ENear(_eNear);
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
    }

    function mint(address, address to, uint128 amount) public onlyRole(MINTER_ROLE) {
        bytes memory fakeProofData = bytes.concat(
            hex"0200000053593a7027c577e14a0a9d47a7ac9222f0814bf6b23c60ac0ee88d80f67901e001bcd53bd4d1ee35c84053491f0d0b602daeae081236372b4bf803d9abd9330706007045c733bac5f0ac28501a9151d96eadd1670978fa84fb6dfdb579c87a1f6dedffdd108715b118508da431070b9372389d410b487549bd7d49b31e23b5dbfc310000000001000000d9f971f15e045e690c1d9bf6e87b73805668ba421350384f1e75bd34e23ecc6437721362b8010000005705790a4ce3400a000000000000000b000000652d6e6561722e6e656172022500000000",
            abi.encodePacked(swapBytes16(amount)),
            abi.encodePacked(to),
            hex"030000004cea846819dd1bf56c738dfacc5cebdf1eb7dd22f54a676cd1cf0b22bf5393f9011c35dbdc79e7950d88ebf8924f5ec4592eb020e9a9df31e5ed0a13dbdd8927b70050362022b2e7dbdc940f61f21b778881f1d479f20d0dcba36c09557f6cd12e2f01d58e75496f1a2b77a10316a279af2adcbb6c2d90b99978bddbb9c7413542cd81f5556b0f286bc591a39a036a64bb1cbd57324b4c4f1822668906508c8969f31b3681c8070000000098bac7912163236d9dd2de729d808cbd7c7b359e85402cb6965257831c7a991d692e59a520079f4d1d3cb8654a0fd4987a62cbcf2ffd111efeb3aed99c4b44bd8de68860ebfc573df63fd50c004da20aa2dce1d00d72ba8003e8fffcdc19fe71f55aded0dc337ba56e84e50fdc1270a4cb6888690f08344f19029830086f5cb9a7ce9d774d57ff179b20f1d688b324a6a6f62e60a5ab493498f6ee8550b00acb7413f6bc3cc92e487f9c2daa345580e8a42d4272e859fc294ddcaf572164623991c0025132f5dfc8160000008cab4f7917658c0ea3265b0423e52cc2022dfbd0b62024004de90d351932b2e101c31a3820444d0ae3acee5b7c4d99d4eba49dcb44ec154d75c1217f54fbc2eb3c0109bc84704c5cfe6ba698b465cba3df8a8d7c5595027837a040b8be4c827450cd0176ededcf963f25ee1f8e53e46503a0d7a764d7ce76b8e0f6f756bf29c0ade5130111274bbd148fca204baa857aad66db092c89fdd2254d1565250d0fa1307af0bd01639b386ef85d52165cd91c0b0d7ff3b72545e6ef652030c9271ab6807b6bcd3b01d13445f2ea94746501e2044ea06f22111fe718788615318b50439789575cf3f601198df8d64579dbe8ccedf43a29b93af50ebf4af0d808cda6b8dee39764f3b03301c39900c6d93efe1755b283cd01a5f4717f6bb6c989617a0fb990c3a2b900796f0083b7cad2577c5a1d3be71aa6f56f75544327569119dc7d0df0ab7e3299b6070700fb2726881c7fa27650003873eb777a1dc21a7c0e63e227da32e64e5160a0425b0062f5c0b2a58d6cae9e53770c93a2eed9685d1c5892a7e1d05b383b68e781d90601120e09be45535aff6e794e78a45ade30f541c23369225e13b441eb58016e0e3301945a5d758ccd4d686c486c210d2ab4e5f4fa15e208d118298f3de87a1e361b4b0160f2e83a0bb4d2cd1cba6ba50c8d66208a90c08bd694e9b2c33f229e9e9a9a03000dd2a5f4062d9a92130cede984a03f81121667024cc4967d002bd889587fc6de0054ce0c34d981837f3fa9f1d99eb5905be18876bba23f6e1016603c505099403e005c0fb550cec2d64f12b537f2c2a00aa339412b39a7c6b7e9506acdcee19b43af00ea6693e8d743a36fa44009abd3261f1b484e339c01383fe684cc054b3f04384000e3829b656ca60088ebdd88090d46f1d39a2f6c0744512c5a38614c86e4bf586200273a218152b554401ec79fab2e5606126e01f3c560f2040abe4487c3bba8180100124bb839a5b9c44856054c6d80bd5229ac2b40fa8a0cd44911629f295b11f08200"
        );
        eNear.finaliseNearToEthTransfer(fakeProofData, 0);
    }

    function burn(address, uint128 amount) public onlyRole(MINTER_ROLE) {
        eNear.transferToNear(amount, string(''));
    }

    function swapBytes16(uint128 v) internal pure returns (uint128) {
        v = ((v & 0x00ff00ff00ff00ff00ff00ff00ff00ff) << 8) | ((v & 0xff00ff00ff00ff00ff00ff00ff00ff00) >> 8);
        v = ((v & 0x0000ffff0000ffff0000ffff0000ffff) << 16) | ((v & 0xffff0000ffff0000ffff0000ffff0000) >> 16);
        v = ((v & 0x00000000ffffffff00000000ffffffff) << 32) | ((v & 0xffffffff00000000ffffffff00000000) >> 32);
        return (v << 64) | (v >> 64);
    }

    function _authorizeUpgrade(
        address newImplementation
    ) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
