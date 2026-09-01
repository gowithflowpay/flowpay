"use client";

import { FormEvent, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { ArrowLeftIcon, CheckIcon, InfoIcon, LockIcon } from "../components/Icons";
import { Brand, LanguageButton } from "../components/Brand";

declare global {
  interface Window {
    ethereum?: { request: (args: { method: string; params?: unknown[] }) => Promise<unknown> };
  }
}

type Payment = { id: string; status: string; chain: string; asset: string; expected_amount_decimal: string };
type Deposit = { tx_hash: string; asset_symbol: string; chain?: string; observed_chain?: string };
type ClaimResult = { id: string; status: string; wallet_challenge?: string; error?: string };

const networks = [
  ["base-sepolia", "Base Sepolia"],
  ["ethereum-sepolia", "Ethereum Sepolia"],
  ["arbitrum-sepolia", "Arbitrum Sepolia"],
  ["optimism-sepolia", "Optimism Sepolia"],
  ["polygon-amoy", "Polygon Amoy"],
  ["bsc-testnet", "BSC Testnet"],
] as const;
const networkName: Record<string, string> = Object.fromEntries(networks);
const isTxHash = (value: string) => /^0x[0-9a-fA-F]{64}$/.test(value.trim());
const isAddress = (value: string) => /^0x[0-9a-fA-F]{40}$/.test(value.trim());

function normalizeChain(value?: string) {
  if (!value) return "";
  const normalized = value.toLowerCase().replaceAll("_", "-");
  const aliases: Record<string, string> = {
    "base-sepolia-testnet": "base-sepolia",
    sepolia: "ethereum-sepolia",
    "eth-sepolia": "ethereum-sepolia",
    "binance-smart-chain-testnet": "bsc-testnet",
  };
  return aliases[normalized] ?? normalized;
}

export function ClaimClient() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const paymentId = searchParams.get("payment_id") ?? "";
  const [payment, setPayment] = useState<Payment | null>(null);
  const [loading, setLoading] = useState(Boolean(paymentId));
  const [transactionHash, setTransactionHash] = useState("");
  const [network, setNetwork] = useState("");
  const [refundAddress, setRefundAddress] = useState("");
  const [actualAsset, setActualAsset] = useState("USDC");
  const [error, setError] = useState("");
  const [submitLabel, setSubmitLabel] = useState("Submit refund claim");
  const [submitting, setSubmitting] = useState(false);
  const [submittedClaim, setSubmittedClaim] = useState<ClaimResult | null>(null);

  useEffect(() => {
    if (!paymentId) return;
    Promise.all([
      fetch(`/api/payment/${paymentId}`, { cache: "no-store" }),
      fetch(`/api/payment/${paymentId}/deposits`, { cache: "no-store" }),
    ])
      .then(async ([paymentResponse, depositResponse]) => {
        if (!paymentResponse.ok) throw new Error("Payment not found.");
        const paymentData = (await paymentResponse.json()) as Payment;
        const deposits = depositResponse.ok ? ((await depositResponse.json()) as Deposit[]) : [];
        const deposit = deposits[0];
        setPayment(paymentData);
        setActualAsset(deposit?.asset_symbol || paymentData.asset || "USDC");
        setNetwork(normalizeChain(deposit?.observed_chain || deposit?.chain || paymentData.chain));
        if (deposit?.tx_hash) setTransactionHash(deposit.tx_hash);
      })
      .catch((cause: unknown) => setError(cause instanceof Error ? cause.message : "Unable to load payment details."))
      .finally(() => setLoading(false));
  }, [paymentId]);

  async function submitClaim(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    if (!paymentId || !payment) return setError("Open this page from the checkout that received the payment.");
    if (!isTxHash(transactionHash)) return setError("Enter a valid 66-character transaction hash.");
    if (!network) return setError("Select the network used for the transfer.");
    if (!isAddress(refundAddress)) return setError("Enter a valid EVM refund address.");
    if (!window.ethereum) return setError("A browser wallet is required to verify ownership of the transfer.");

    setSubmitting(true);
    try {
      setSubmitLabel("Connecting wallet…");
      const accounts = (await window.ethereum.request({ method: "eth_requestAccounts" })) as string[];
      const originatingWallet = accounts?.[0];
      if (!originatingWallet) throw new Error("No wallet account was selected.");

      setSubmitLabel("Creating claim…");
      const createResponse = await fetch("/api/claims", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          payment_id: paymentId,
          transaction_hash: transactionHash.trim(),
          actual_chain: network,
          actual_asset: actualAsset,
          originating_wallet: originatingWallet,
          recovery_destination: refundAddress.trim(),
          explanation: `Refund requested for ${transactionHash.trim()} on ${networkName[network] ?? network}.`,
        }),
      });
      const claim = (await createResponse.json()) as ClaimResult;
      if (!createResponse.ok) throw new Error(claim.error || "The claim could not be created.");
      if (!claim.wallet_challenge) throw new Error("The server did not return a wallet challenge.");

      setSubmitLabel("Sign in wallet…");
      const signature = (await window.ethereum.request({
        method: "personal_sign",
        params: [claim.wallet_challenge, originatingWallet],
      })) as string;

      setSubmitLabel("Submitting claim…");
      const authorizeResponse = await fetch(`/api/claims/${claim.id}/authorize`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ wallet_signature: signature }),
      });
      const authorized = (await authorizeResponse.json()) as ClaimResult;
      if (!authorizeResponse.ok) throw new Error(authorized.error || "Wallet verification failed.");
      setSubmittedClaim({ ...claim, ...authorized });
    } catch (cause: unknown) {
      setError(cause instanceof Error ? cause.message : "The claim could not be submitted.");
    } finally {
      setSubmitting(false);
      setSubmitLabel("Submit refund claim");
    }
  }

  if (loading) return <main className="claim-loading">Loading payment details…</main>;

  if (submittedClaim) {
    return (
      <main className="claim-shell">
        <header className="claim-header"><Brand /><LanguageButton /></header>
        <section className="claim-success simple-claim-success">
          <span className="claim-success-icon"><CheckIcon /></span>
          <p className="eyebrow">Claim received</p>
          <h1>Your refund request was submitted</h1>
          <p>We verified your wallet and recorded the claim. Keep this reference for support.</p>
          <code>{submittedClaim.id}</code>
          <button type="button" className="primary-button" onClick={() => router.push(`/pay/${paymentId}`)}>Return to checkout</button>
        </section>
      </main>
    );
  }

  return (
    <main className="claim-shell">
      <header className="claim-header"><Brand /><LanguageButton /></header>
      <div className="claim-stage single-claim-layout">
        <button type="button" className="back-payment simple-claim-back" onClick={() => router.back()}><ArrowLeftIcon /> Back to checkout</button>
        <section className="claim-panel simple-claim-card">
          <div className="simple-claim-head">
            <p className="eyebrow">Wrong asset recovery</p>
            <h1>Create refund claim</h1>
            <p>Enter the transfer and the address that should receive the refund.</p>
          </div>
          {payment && <div className="simple-payment-reference"><span>Payment</span><strong>{payment.id}</strong></div>}
          <form className="simple-claim-form" onSubmit={submitClaim}>
            <label className="simple-field">
              <span>Transaction hash</span>
              <input value={transactionHash} onChange={(event) => setTransactionHash(event.target.value)} placeholder="0x…" autoComplete="off" spellCheck={false} />
            </label>
            <label className="simple-field">
              <span>Network used</span>
              <select value={network} onChange={(event) => setNetwork(event.target.value)}>
                <option value="">Select network</option>
                {networks.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
              </select>
            </label>
            <label className="simple-field">
              <span>Refund address</span>
              <input value={refundAddress} onChange={(event) => setRefundAddress(event.target.value)} placeholder="0x…" autoComplete="off" spellCheck={false} />
              <small>Refunds will be sent to this address after the claim is approved.</small>
            </label>
            <div className="simple-claim-note"><InfoIcon /><p><strong>Wallet verification</strong>Your wallet will open once when you submit. Sign the message to prove you sent the transfer.</p></div>
            {error && <div className="form-error simple-form-error" role="alert"><InfoIcon /> {error}</div>}
            <button type="submit" className="primary-button simple-submit" disabled={submitting}><LockIcon /> {submitLabel}</button>
          </form>
        </section>
      </div>
    </main>
  );
}
