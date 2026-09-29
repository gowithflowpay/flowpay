// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {FlowPayFactory} from "../src/FlowPayFactory.sol";
import {CheckoutProxy} from "../src/CheckoutProxy.sol";

contract FactorySafetyTest is Test {
    function test_wrongFactoryProducesDifferentAddress() public {
        FlowPayFactory a = new FlowPayFactory(address(this));
        FlowPayFactory b = new FlowPayFactory(address(this));
        bytes32 salt = keccak256("same-payment");
        assertTrue(a.computeCheckoutAddress(salt) != b.computeCheckoutAddress(salt));
    }

    function test_checkoutCannotBeRedeployed() public {
        FlowPayFactory factory = new FlowPayFactory(address(this));
        bytes32 salt = keccak256("once");
        address first = factory.deployCheckout(salt);
        address second = factory.deployCheckout(salt);
        assertEq(first, second);
        assertEq(CheckoutProxy(factory.computeProxyAddress(salt)).deployed(), true);
    }
}
