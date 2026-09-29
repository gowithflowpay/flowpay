// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {FlowPayFactory} from "../src/FlowPayFactory.sol";
import {CheckoutProxy} from "../src/CheckoutProxy.sol";
import {CheckoutReceiver} from "../src/CheckoutReceiver.sol";
import {TestToken} from "../src/TestToken.sol";
import {ToggleFailToken} from "../src/ToggleFailToken.sol";

contract FlowPayFactoryTest is Test {
    FlowPayFactory factory;
    TestToken token;
    address operator = address(0xA11CE);
    address customer = address(0xC0FFEE);
    address recovery = address(0xBEEF);

    function setUp() public {
        factory = new FlowPayFactory(operator);
        token = new TestToken("Test USD", "tUSD", 6);
        token.mint(customer, 1_000_000_000);
        vm.deal(customer, 10 ether);
    }

    function test_computeCheckoutAddressMatchesCreateAddress() public {
        bytes32 salt = keccak256("checkout-1");
        address predicted = factory.computeCheckoutAddress(salt);
        vm.prank(operator);
        address deployed = factory.deployCheckout(salt);
        assertEq(deployed, predicted);
        assertGt(deployed.code.length, 0);
    }

    function test_assetsSentBeforeDeploymentRemainRecoverable() public {
        bytes32 salt = keccak256("predeploy-token");
        address predicted = factory.computeCheckoutAddress(salt);
        vm.prank(customer);
        assertTrue(token.transfer(predicted, 50_000_000));
        assertEq(token.balanceOf(predicted), 50_000_000);
        assertEq(predicted.code.length, 0);

        vm.prank(operator);
        (address receiver, uint256 amount) = factory.recoverToken(salt, address(token), recovery, 50_000_000);
        assertEq(receiver, predicted);
        assertEq(amount, 50_000_000);
        assertEq(token.balanceOf(recovery), 50_000_000);
        assertEq(token.balanceOf(predicted), 0);
    }

    function test_recoveryIsAmountBounded() public {
        bytes32 salt = keccak256("bounded");
        address predicted = factory.computeCheckoutAddress(salt);
        vm.prank(customer);
        assertTrue(token.transfer(predicted, 100_000_000));
        vm.prank(operator);
        (, uint256 amount) = factory.recoverToken(salt, address(token), recovery, 50_000_000);
        assertEq(amount, 50_000_000);
        assertEq(token.balanceOf(recovery), 50_000_000);
        assertEq(token.balanceOf(predicted), 50_000_000);
    }

    function test_nativeRecoveryIsAmountBounded() public {
        bytes32 salt = keccak256("bounded-native");
        address predicted = factory.computeCheckoutAddress(salt);
        vm.deal(predicted, 3 ether);
        uint256 beforeBalance = recovery.balance;
        vm.prank(operator);
        (, uint256 amount) = factory.recoverNative(salt, payable(recovery), 1 ether);
        assertEq(amount, 1 ether);
        assertEq(recovery.balance - beforeBalance, 1 ether);
        assertEq(predicted.balance, 2 ether);
    }

    function test_nativeSentBeforeDeploymentRemainRecoverable() public {
        bytes32 salt = keccak256("predeploy-native");
        address predicted = factory.computeCheckoutAddress(salt);
        vm.prank(customer);
        (bool ok,) = predicted.call{value: 2 ether}("");
        assertTrue(ok);
        assertEq(predicted.balance, 2 ether);

        vm.prank(operator);
        (, uint256 amount) = factory.recoverNative(salt, payable(recovery), 2 ether);
        assertEq(amount, 2 ether);
        assertEq(recovery.balance, 2 ether);
    }

    function test_unauthorizedCallerCannotDeployOrRecover() public {
        bytes32 salt = keccak256("authz");
        vm.expectRevert(FlowPayFactory.OnlyOperator.selector);
        factory.deployCheckout(salt);
        vm.expectRevert(FlowPayFactory.OnlyOperator.selector);
        factory.recoverToken(salt, address(token), recovery, 50_000_000);
    }

    function test_proxyCannotBeUsedByAttacker() public {
        bytes32 salt = keccak256("proxy-authz");
        vm.prank(operator);
        factory.deployCheckout(salt);
        address proxy = factory.computeProxyAddress(salt);
        vm.expectRevert(CheckoutProxy.OnlyController.selector);
        CheckoutProxy(proxy).deploy(hex"60006000f3");
    }

    function test_receiverCannotBeSweptByAttacker() public {
        bytes32 salt = keccak256("receiver-authz");
        vm.prank(operator);
        address receiver = factory.deployCheckout(salt);
        vm.expectRevert(CheckoutReceiver.OnlyController.selector);
        CheckoutReceiver(payable(receiver)).sweepToken(address(token), recovery, 1);
    }

    function test_testOnlyFailTokenCanReproduceSimulationFailure() public {
        ToggleFailToken failing = new ToggleFailToken("Failure Token", "FAIL", 6);
        failing.mint(customer, 50_000_000);
        bytes32 salt = keccak256("simulation-failure");
        address predicted = factory.computeCheckoutAddress(salt);
        vm.prank(customer);
        assertTrue(failing.transfer(predicted, 50_000_000));
        assertEq(failing.balanceOf(predicted), 50_000_000);

        failing.setFailTransfers(true);
        vm.prank(operator);
        vm.expectRevert();
        factory.recoverToken(salt, address(failing), recovery, 50_000_000);
        assertEq(failing.balanceOf(predicted), 50_000_000);
    }

    function test_sameSaltIsStable() public view {
        bytes32 salt = keccak256("stable");
        assertEq(factory.computeCheckoutAddress(salt), factory.computeCheckoutAddress(salt));
    }

    function testFuzz_differentSaltsDoNotCollide(bytes32 a, bytes32 b) public view {
        vm.assume(a != b);
        assertTrue(factory.computeCheckoutAddress(a) != factory.computeCheckoutAddress(b));
    }
}
