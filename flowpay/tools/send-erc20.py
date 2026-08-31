import os
import sys

from web3 import Web3


def required(name: str) -> str:
    value = os.environ.get(name, "").strip()
    if not value:
        raise RuntimeError(f"{name} is required")
    return value


def main() -> int:
    rpc_url = required("RPC_URL")
    private_key = required("PRIVATE_KEY")
    token = Web3.to_checksum_address(required("TOKEN_ADDRESS"))
    recipient = Web3.to_checksum_address(required("RECIPIENT"))
    amount = int(required("AMOUNT_ATOMIC"))
    if amount <= 0:
        raise RuntimeError("AMOUNT_ATOMIC must be positive")

    web3 = Web3(Web3.HTTPProvider(rpc_url, request_kwargs={"timeout": 30}))
    if not web3.is_connected():
        raise RuntimeError("RPC connection failed")

    account = web3.eth.account.from_key(private_key)
    contract = web3.eth.contract(
        address=token,
        abi=[
            {
                "inputs": [
                    {"name": "account", "type": "address"},
                ],
                "name": "balanceOf",
                "outputs": [{"name": "", "type": "uint256"}],
                "stateMutability": "view",
                "type": "function",
            },
            {
                "inputs": [
                    {"name": "to", "type": "address"},
                    {"name": "amount", "type": "uint256"},
                ],
                "name": "transfer",
                "outputs": [{"name": "", "type": "bool"}],
                "stateMutability": "nonpayable",
                "type": "function",
            },
        ],
    )
    balance = contract.functions.balanceOf(account.address).call()
    if balance < amount:
        raise RuntimeError(f"insufficient token balance: have {balance}, need {amount}")

    transaction = contract.functions.transfer(recipient, amount).build_transaction(
        {
            "from": account.address,
            "nonce": web3.eth.get_transaction_count(account.address, "pending"),
            "chainId": web3.eth.chain_id,
        }
    )
    signed = account.sign_transaction(transaction)
    tx_hash = web3.eth.send_raw_transaction(signed.raw_transaction)
    receipt = web3.eth.wait_for_transaction_receipt(tx_hash, timeout=180)
    if receipt.status != 1:
        raise RuntimeError(f"transaction reverted: {tx_hash.hex()}")
    print(tx_hash.hex())
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)
