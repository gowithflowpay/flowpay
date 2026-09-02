// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IERC20Minimal {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address to, uint256 amount) external returns (bool);
}

/// @notice Minimal receiver deployed only when settlement/recovery is required.
/// @dev The controller is always the FlowPay factory. No AI logic lives here.
contract CheckoutReceiver {
    error OnlyController();
    error TransferFailed();
    error NativeTransferFailed();
    error ZeroDestination();

    address public immutable CONTROLLER;

    constructor(address controller) payable {
        if (controller == address(0)) revert ZeroDestination();
        CONTROLLER = controller;
    }

    modifier onlyController() {
        if (msg.sender != CONTROLLER) revert OnlyController();
        _;
    }

    receive() external payable {}

    function sweepToken(address token, address destination, uint256 amount)
        external
        onlyController
        returns (uint256 transferred)
    {
        if (destination == address(0)) revert ZeroDestination();
        uint256 balance = IERC20Minimal(token).balanceOf(address(this));
        transferred = amount > balance ? balance : amount;
        if (transferred != 0) {
            // Support both standard ERC-20 tokens and legacy tokens such as USDT
            // that return no data from transfer().
            (bool ok, bytes memory result) = token.call(
                abi.encodeWithSelector(IERC20Minimal.transfer.selector, destination, transferred)
            );
            if (!ok || (result.length != 0 && !abi.decode(result, (bool)))) {
                revert TransferFailed();
            }
        }
    }

    function sweepNative(address payable destination, uint256 maxAmount)
        external
        onlyController
        returns (uint256 amount)
    {
        if (destination == address(0)) revert ZeroDestination();
        uint256 balance = address(this).balance;
        amount = maxAmount > balance ? balance : maxAmount;
        if (amount != 0) {
            (bool ok,) = destination.call{value: amount}("");
            if (!ok) revert NativeTransferFailed();
        }
    }
}
