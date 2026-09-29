# Improvement Changelog

This log records changes when they are made. Results are labeled by what was actually measured.

## Stage: Architecture foundation

**Problem observed:** deterministic payment processing, claim investigation and recovery execution initially shared an insufficiently explicit trust boundary.

**Hypothesis:** an agent is useful at the exception boundary only if uncertain investigation cannot directly become a financial action.

**Change:** explicit payment/claim state machines; typed investigation tools; deterministic policy/simulation/approval/signer path; CREATE3 cross-chain invariants; normalized audit schema.

**Result:** architectural/test foundation only; no performance result was claimed at this stage.

**Decision:** keep the happy path deterministic and make escalation the default for ambiguous recovery facts.

**Lesson:** CREATE3 alone does not guarantee a same-address cross-chain recovery path; factory deployment is part of the protocol.

## Stage: EVM payment and recovery implementation

**Problem observed:** a counterfactual-address design is only useful if pre-deployment assets remain controllable after receiver deployment and the deployment path cannot be front-run by arbitrary child code.

**Hypothesis:** a fixed one-shot proxy plus operator-restricted factory and tiny receiver can preserve the useful CREATE3 property without giving arbitrary callers deployment control.

**Change:** implemented `FlowPayFactory`, `CheckoutProxy`, `CheckoutReceiver`, deterministic address tests, bounded ERC-20/native recovery and local identical-factory deployment checks.

**Result:** implementation and tests are present in source; this current container cannot execute Foundry, so no fresh Foundry pass is claimed by this changelog update.

**Decision:** retain the fixed factory/proxy/receiver construction and amount-bounded recovery.

**Lesson:** “sweep entire balance” is unsafe when more than one sender could have transferred to an address; recovery plans should bind the maximum amount they are allowed to move.

## Stage: Model-driven investigation

**Problem observed:** the first ClaimAgent orchestration was a safe deterministic decision tree, but judges could reasonably classify it as workflow automation rather than a tool-using AI agent.

**Hypothesis:** model-selected investigation tools can add genuine agentic behavior without weakening financial controls if execution tools remain unavailable to the model.

**Change:** added `backend/crates/agent/src/model.rs` using model-driven typed tool selection and a constrained final disposition. The model has no private-key, arbitrary RPC, arbitrary calldata, approval or signing tool. Model mode refuses silent deterministic substitution when its credential is missing.

**Result:** source-level implementation and deterministic tool-boundary tests were added. Live model E2E results remain dependent on running the real E2E harness.

**Decision:** preserve deterministic policy, simulation, approval and signer gates after every model recommendation.

**Lesson:** “agentic” need not mean “agent controls money.” The useful boundary is investigation and constrained recommendation.

## Stage: Evaluation correction

**Problem observed:** the original 20-case JavaScript evaluator mirrored expected workflow behavior but did not drive Rust services, contracts or chains.

**Hypothesis:** the same scenarios should be replayed through real infrastructure before metrics are presented as system performance.

**Change:** retained the original runner as a spec-regression suite and added `evals/e2e/run.mjs`, dual-chain/RPC fault fixtures, webhook retry sink and `scripts/run-e2e-evals.sh` for the live system.

**Result:** the legacy fixture runner produced 35% baseline vs 80% constrained-workflow autonomous resolution and 0% unsafe actions. Those numbers are now explicitly labeled **fixture/spec results**, not live-system proof. No new authoritative E2E score is claimed until the live harness runs successfully with Rust, Foundry, native PostgreSQL/RabbitMQ/Kafka services and, for model mode, a model credential.

**Decision:** only `evals/results/e2e/*` generated from the actual backend/chains may be used as the hackathon system benchmark.

**Lesson:** an evaluation can be deterministic and reproducible while still measuring the wrong layer.

## Stage: Runtime hardening discovered by real-system design

**Problem observed:** source review of the live path exposed bugs the fixture evaluator could not detect.

**Changes:** corrected simulation caller identity for `onlyOperator`; separated technical `eth_call` validity from operator gas availability; canonicalized persisted chain keys; fixed custom-chain round-tripping; strengthened reorg revalidation for disappeared logs; tightened approval consumption semantics.

**Result:** regression coverage/source changes added. Fresh full integration verification remains pending in this execution environment.

**Decision:** keep these checks in the live path and require full E2E execution before submission.

**Lesson:** integration architecture tests catch failures that state-only fixtures cannot.

## Stage: Backend-first repository restructure

**Problem observed:** backend crates, contracts, apps, SDKs and evals were all top-level peers, obscuring that FlowPay's backend is the product and the UIs are clients.

**Hypothesis:** a backend-first modular-monolith layout with only operationally meaningful process boundaries will improve ownership, scale boundaries and reproducibility without creating a microservice jungle.

**Change:** moved Rust libraries and migrations under `backend/`; split binaries into `api-server`, `worker` and `event-relay`; renamed `apps/dashboard` to `apps/merchant`; retained contracts/apps/SDK/evals as separate top-level concerns.

**Result:** path/dependency structure has been rewritten and statically checked for missing Cargo path dependencies. Fresh Cargo/Foundry builds cannot be run in this container because those toolchains are absent.

**Decision:** keep business modules as shared Rust crates and process separation limited to API, worker and relay. The signer remains a crate with an extraction-friendly interface.

**Lesson:** process boundaries should follow operational/security reasons, not crate boundaries.

## Stage: Transactional outbox + RabbitMQ + Kafka

**Problem observed:** direct database mutation followed by broker publication can lose events when a process crashes between the two operations; using all messaging mechanisms for the same responsibility creates duplicate-delivery ambiguity.

**Hypothesis:** PostgreSQL as source of truth + transactional outbox + RabbitMQ for commands + Kafka for facts gives useful decoupling while retaining deterministic recovery from broker failure.

**Change:** added `backend/crates/messaging`, migration `0005_messaging.sql`, `flowpay-event-relay`, RabbitMQ durable/idempotent worker commands and Kafka domain-event publishing. Payment creation atomically records both `payment.created` and `payment.monitor.start`; claim creation records `claim.created`, and the transition into `INVESTIGATING` atomically records `claim.investigation.started` plus `claim.investigate`; approved recovery records `recovery.approved` and `recovery.execute` in the approval transaction.

**Result:** architecture and source implementation complete in this pass. Broker/runtime execution is not claimed until native-service and Cargo verification runs.

**Decision:** preserve periodic PostgreSQL reconciliation loops as correctness recovery while brokers provide low-latency triggers/event fan-out.

**Lesson:** messaging should remove coupling, not become a second source of financial truth.
