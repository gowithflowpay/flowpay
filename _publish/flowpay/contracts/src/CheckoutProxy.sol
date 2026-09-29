// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @notice CREATE2 proxy whose first CREATE child is the FlowPay checkout receiver.
/// @dev Creation code is constant. The controller is captured from msg.sender at construction.
contract CheckoutProxy {
    error OnlyController();
    error AlreadyDeployed();
    error EmptyInitCode();
    error DeploymentFailed();

    address public immutable CONTROLLER;
    bool public deployed;

    constructor() {
        CONTROLLER = msg.sender;
    }

    function deploy(bytes calldata initCode) external returns (address child) {
        if (msg.sender != CONTROLLER) revert OnlyController();
        if (deployed) revert AlreadyDeployed();
        if (initCode.length == 0) revert EmptyInitCode();

        deployed = true;
        bytes memory code = initCode;
        assembly ("memory-safe") {
            child := create(0, add(code, 0x20), mload(code))
        }
        if (child == address(0)) revert DeploymentFailed();
    }
}
