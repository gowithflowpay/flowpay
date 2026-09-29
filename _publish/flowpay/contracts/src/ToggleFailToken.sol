// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @notice Test-only ERC-20 used to reproduce a recovery simulation failure.
/// @dev Deposits can be made while failTransfers=false, then the operator toggles failure
///      before recovery simulation. This contract is never intended for production use.
contract ToggleFailToken {
    error TransfersDisabled();
    error InsufficientBalance();

    string public name;
    string public symbol;
    uint8 public immutable decimals;
    bool public failTransfers;
    mapping(address => uint256) public balanceOf;

    event Transfer(address indexed from, address indexed to, uint256 value);

    constructor(string memory name_, string memory symbol_, uint8 decimals_) {
        name = name_;
        symbol = symbol_;
        decimals = decimals_;
    }

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
        emit Transfer(address(0), to, amount);
    }

    function setFailTransfers(bool enabled) external {
        failTransfers = enabled;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        if (failTransfers) revert TransfersDisabled();
        uint256 balance = balanceOf[msg.sender];
        if (balance < amount) revert InsufficientBalance();
        unchecked {
            balanceOf[msg.sender] = balance - amount;
        }
        balanceOf[to] += amount;
        emit Transfer(msg.sender, to, amount);
        return true;
    }
}
