import { adminApi } from "../lib/api";

export const dynamic = "force-dynamic";

type LedgerAccount = { account: string; balance_atomic: string | null };
type Revenue = {
  environment?: string;
  platform_fee_atomic?: string;
  provider_fee_bps?: string;
  provider_fee_vat_bps?: string;
  ledger?: LedgerAccount[];
  ngn?: {
    completed_payments?: number;
    collected_atomic?: string;
    merchant_credited_atomic?: string;
    platform_fee_atomic?: string;
    provider_fee_atomic?: string;
  };
};

const ATOMIC_PER_NAIRA = 100;

function naira(atomic: string | null | undefined) {
  if (atomic === null || atomic === undefined || atomic === "") return "—";
  const value = Number(atomic);
  if (!Number.isFinite(value)) return "—";
  return `₦${(value / ATOMIC_PER_NAIRA).toLocaleString("en-NG", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

function ledgerBalance(revenue: Revenue, account: string) {
  const match = (revenue.ledger ?? []).find((entry) => entry.account === account);
  return match?.balance_atomic ?? null;
}

export default async function Admin() {
  let revenue: Revenue | null = null;
  let error = "";
  try {
    revenue = await adminApi<Revenue>("/v1/admin/revenue");
  } catch (cause) {
    error = cause instanceof Error ? cause.message : "Unable to load platform revenue";
  }

  if (error) {
    return (
      <main className="shell">
        <header className="head">
          <span className="mark">FlowPay</span>
          <h1>Platform admin</h1>
        </header>
        <section className="panel">
          <p className="error">{error}</p>
          <p className="hint">
            Set <code>FLOWPAY_ADMIN_KEY</code> on this project to the value in{" "}
            <code>/etc/flowpay.env</code>.
          </p>
        </section>
      </main>
    );
  }

  const ngn = revenue?.ngn ?? {};
  const platformRevenue = ledgerBalance(revenue!, "platform:revenue");
  const providerFees = ledgerBalance(revenue!, "platform:payment-fees");
  const feePercent = ((Number(revenue?.provider_fee_bps ?? 0) / 100) || 0).toFixed(2);
  const vatPercent = ((Number(revenue?.provider_fee_vat_bps ?? 0) / 100) || 0).toFixed(2);

  return (
    <main className="shell">
      <header className="head">
        <span className="mark">FlowPay</span>
        <h1>Platform admin</h1>
        <span className="env">{revenue?.environment ?? "—"}</span>
      </header>

      <section className="cards">
        <article className="card primary">
          <span>Platform revenue</span>
          <strong>{naira(platformRevenue ?? ngn.platform_fee_atomic)}</strong>
          <small>Collected on completed bank transfers</small>
        </article>
        <article className="card">
          <span>Payment processing</span>
          <strong>{naira(providerFees ?? ngn.provider_fee_atomic)}</strong>
          <small>Absorbed by the customer, not the merchant</small>
        </article>
        <article className="card">
          <span>Completed payments</span>
          <strong>{ngn.completed_payments ?? 0}</strong>
          <small>NGN bank transfers settled</small>
        </article>
      </section>

      <section className="panel">
        <h2>Collected so far</h2>
        <dl className="ledger">
          <div>
            <dt>Customers paid</dt>
            <dd>{naira(ngn.collected_atomic)}</dd>
          </div>
          <div>
            <dt>Credited to merchants</dt>
            <dd>{naira(ngn.merchant_credited_atomic)}</dd>
          </div>
          <div>
            <dt>Platform fee</dt>
            <dd>{naira(ngn.platform_fee_atomic)}</dd>
          </div>
          <div>
            <dt>Flutterwave fee</dt>
            <dd>{naira(ngn.provider_fee_atomic)}</dd>
          </div>
        </dl>
      </section>

      <section className="panel">
        <h2>Ledger accounts</h2>
        <table className="table">
          <thead>
            <tr>
              <th>Account</th>
              <th>Balance</th>
            </tr>
          </thead>
          <tbody>
            {(revenue?.ledger ?? []).map((entry) => (
              <tr key={entry.account}>
                <td className="mono">{entry.account}</td>
                <td>{entry.balance_atomic ? naira(entry.balance_atomic) : "No entries yet"}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="hint">
          Ledger balances appear once the first grossed-up payment is collected through
          Formance.
        </p>
      </section>

      <footer className="foot">
        <span>Platform fee per payment: {naira(revenue?.platform_fee_atomic)}</span>
        <span>
          Provider fee: {feePercent}% + {vatPercent}% VAT on the fee
        </span>
        <span>Charged to the customer, so merchants receive their full amount</span>
      </footer>
    </main>
  );
}
