# FlowPay Evaluation Fixtures

`scenarios/manifest.json` enumerates the same 20 deterministic cases that will be consumed by
both the baseline runner and the final agent runner. `result` is intentionally `null` in every
fixture until an executable runner produces a measured outcome.

Scenarios that execute recovery require all five checkpoints: policy, simulation, human approval,
restricted signer, and receipt verification.
