// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {CheckoutProxy} from "./CheckoutProxy.sol";
import {CheckoutReceiver} from "./CheckoutReceiver.sol";

/// @notice Deterministic checkout factory for FlowPay EVM networks.
/// @dev The final receiver address depends on this factory, salt and CheckoutProxy creation code.
contract FlowPayFactory {
    error OnlyOwner();
    error OnlyOperator();
    error ZeroAddress();
    error ReceiverAddressMismatch();
    error ProxyDeploymentFailed();

    event OperatorUpdated(address indexed previousOperator, address indexed newOperator);
    event CheckoutDeployed(bytes32 indexed salt, address indexed proxy, address indexed receiver);
    event TokenRecovered(bytes32 indexed salt, address indexed token, address indexed destination, uint256 amount);
    event NativeRecovered(bytes32 indexed salt, address indexed destination, uint256 amount);

    address public immutable OWNER;
    address public operator;

    bytes32 public constant PROXY_CREATION_CODE_HASH = keccak256(type(CheckoutProxy).creationCode);

    constructor(address initialOperator) {
        if (initialOperator == address(0)) revert ZeroAddress();
        OWNER = msg.sender;
        operator = initialOperator;
    }

    modifier onlyOwner() {
        if (msg.sender != OWNER) revert OnlyOwner();
        _;
    }

    modifier onlyOperator() {
        if (msg.sender != operator) revert OnlyOperator();
        _;
    }

    function setOperator(address newOperator) external onlyOwner {
        if (newOperator == address(0)) revert ZeroAddress();
        emit OperatorUpdated(operator, newOperator);
        operator = newOperator;
    }

    function computeProxyAddress(bytes32 salt) public view returns (address proxy) {
        bytes32 digest = keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, PROXY_CREATION_CODE_HASH));
        proxy = address(uint160(uint256(digest)));
    }

    function computeCheckoutAddress(bytes32 salt) public view returns (address receiver) {
        address proxy = computeProxyAddress(salt);
        // RLP([proxy, nonce=1]) = 0xd6 0x94 <20-byte-address> 0x01
        bytes32 digest = keccak256(abi.encodePacked(hex"d694", proxy, hex"01"));
        receiver = address(uint160(uint256(digest)));
    }

    function checkoutDeployed(bytes32 salt) public view returns (bool) {
        return computeCheckoutAddress(salt).code.length != 0;
    }

    function deployCheckout(bytes32 salt) public onlyOperator returns (address receiver) {
        receiver = computeCheckoutAddress(salt);
        if (receiver.code.length != 0) return receiver;

        address proxy = computeProxyAddress(salt);
        if (proxy.code.length == 0) {
            CheckoutProxy deployedProxy = new CheckoutProxy{salt: salt}();
            if (address(deployedProxy) != proxy) revert ProxyDeploymentFailed();
        }

        bytes memory initCode = abi.encodePacked(type(CheckoutReceiver).creationCode, abi.encode(address(this)));
        address deployedReceiver = CheckoutProxy(proxy).deploy(initCode);
        if (deployedReceiver != receiver) revert ReceiverAddressMismatch();
        emit CheckoutDeployed(salt, proxy, receiver);
    }

    function recoverToken(bytes32 salt, address token, address destination, uint256 maxAmount)
        external
        onlyOperator
        returns (address receiver, uint256 amount)
    {
        if (token == address(0) || destination == address(0)) revert ZeroAddress();
        receiver = deployCheckout(salt);
        amount = CheckoutReceiver(payable(receiver)).sweepToken(token, destination, maxAmount);
        emit TokenRecovered(salt, token, destination, amount);
    }

    function recoverNative(bytes32 salt, address payable destination, uint256 maxAmount)
        external
        onlyOperator
        returns (address receiver, uint256 amount)
    {
        if (destination == address(0)) revert ZeroAddress();
        receiver = deployCheckout(salt);
        amount = CheckoutReceiver(payable(receiver)).sweepNative(destination, maxAmount);
        emit NativeRecovered(salt, destination, amount);
    }
}
