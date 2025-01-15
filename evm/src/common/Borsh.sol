// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

library Borsh {
    function encodeUint32(uint32 val) internal pure returns (bytes4) {
        return bytes4(swapBytes4(val));
    }

    function encodeUint64(uint64 val) internal pure returns (bytes8) {
        return bytes8(swapBytes8(val));
    }

    function encodeUint128(uint128 val) internal pure returns (bytes16) {
        return bytes16(swapBytes16(val));
    }

    function encodeString(string memory val) internal pure returns (bytes memory) {
        bytes memory b = bytes(val);
        return bytes.concat(
            encodeUint32(uint32(b.length)),
            bytes(val)
        );
    }

    function encodeAddress(address val) internal pure returns (bytes20) {
        return bytes20(val);
    }

    function swapBytes4(uint32 v) internal pure returns (uint32) {
        v = ((v & 0x00ff00ff) << 8) | ((v & 0xff00ff00) >> 8);
        return (v << 16) | (v >> 16);
    }

    function swapBytes8(uint64 v) internal pure returns (uint64) {
        v = ((v & 0x00ff00ff00ff00ff) << 8) | ((v & 0xff00ff00ff00ff00) >> 8);
        v = ((v & 0x0000ffff0000ffff) << 16) | ((v & 0xffff0000ffff0000) >> 16);
        return (v << 32) | (v >> 32);
    }

    function swapBytes16(uint128 v) internal pure returns (uint128) {
        v = ((v & 0x00ff00ff00ff00ff00ff00ff00ff00ff) << 8) | ((v & 0xff00ff00ff00ff00ff00ff00ff00ff00) >> 8);
        v = ((v & 0x0000ffff0000ffff0000ffff0000ffff) << 16) | ((v & 0xffff0000ffff0000ffff0000ffff0000) >> 16);
        v = ((v & 0x00000000ffffffff00000000ffffffff) << 32) | ((v & 0xffffffff00000000ffffffff00000000) >> 32);
        return (v << 64) | (v >> 64);
    }
}
