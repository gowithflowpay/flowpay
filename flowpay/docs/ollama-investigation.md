# Ollama claim investigation

FlowPay uses Ollama as the model-driven investigator in model mode.

The agent is invoked only for payment exceptions and claims. It must:

1. load the authoritative payment and claim records;
2. verify the claimant wallet authorization and compare it with the transaction sender;
3. query the supplied transaction on the claimed chain;
4. verify receipt success, canonicality, recipient, token contract, amount, and confirmations;
5. verify the deterministic FlowPay checkout address and factory recovery capability;
6. read the current balance of the exact token at the checkout address;
7. compare actual chain, asset, and amount with the expected payment;
8. recommend `RECOVERABLE_CANDIDATE`, `NEEDS_MORE_EVIDENCE`, `NOT_RECOVERABLE`, or `ESCALATE`.

Screenshots, receipts, explanations, and hashes are leads only. Ollama cannot make them authoritative.

The model has typed read-only investigation tools only. It does not receive private keys, seed phrases,
arbitrary RPC access, transaction-signing tools, or a generic execution tool.

After the recommendation, Rust performs the money-moving work:

- `RecoveryPolicy` evaluates supported chain/token, ownership, factory, balance, gas, and risk flags;
- the exact allowlisted recovery transaction is built;
- the transaction is simulated;
- cross-chain and amount-discrepancy cases require human approval;
- high-risk or ambiguous cases escalate;
- the restricted signer submits only an approved transaction;
- the receipt and destination balance change are verified on-chain.

## Configuration

```bash
FLOWPAY_AGENT_MODE=model
FLOWPAY_MODEL_PROVIDER=ollama
FLOWPAY_MODEL_ENDPOINT=http://127.0.0.1:11434/api/chat
FLOWPAY_AGENT_MODEL=qwen2.5-coder:7b
FLOWPAY_AGENT_MAX_STEPS=12
FLOWPAY_AGENT_RETRY_BUDGET=3
```

Install and prepare the model outside FlowPay, then start the worker:

```bash
ollama pull qwen2.5-coder:7b
./scripts/run-worker.sh model
```

The API process does not call Ollama for investigation; the worker owns claim investigation. If Ollama is unavailable, retryable failures remain bounded and unresolved claims are escalated rather than silently switched to another provider.
