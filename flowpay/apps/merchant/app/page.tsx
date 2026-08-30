import {api} from "../lib/api";
import {moneyFromStableBalances,networkAsset,networkLabel,statusTone,tokenAsset} from "../lib/format";
import {ArrowRightIcon,ArrowUpRightIcon,ClaimIcon,PaymentIcon,PlusIcon,ShieldIcon,WalletIcon} from "./components/Icons";

export default async function Overview(){
  try{
  const [overview,paymentsResult]=await Promise.all([api("/v1/merchant/overview"),api("/v1/payments?limit=8")]);
  const payments=paymentsResult?.data??[],balances=overview?.balances??[];
  const openClaims=Number(overview?.claims?.open??0),actionable=Number(overview?.claims?.actionable??0);
  return <>
    <div className="page-head"><div><h1>Good afternoon</h1><p>Here's what's happening with LumaBot payments today.</p></div><div className="actions"><button className="btn secondary" disabled title="Withdrawals are disabled in local test mode"><ArrowUpRightIcon/>Withdraw</button><a className="btn primary" href="/payments/new"><PlusIcon/>Create payment</a></div></div>

    <section className="balance-card"><div className="balance-label">Available balance</div><div className="balance-number">{moneyFromStableBalances(balances)}</div><div className="balance-meta"><div className="balance-assets">{balances.filter((b:any)=>!b.error).slice(0,4).map((b:any)=><span className="asset-chip" key={`${b.chain}:${b.symbol}`}><img src={tokenAsset(b.symbol)} alt=""/>{b.amount} {b.symbol} · {String(b.chain).toUpperCase()}</span>)}{balances.length===0?<span className="asset-chip">No settled test-token balance yet</span>:null}</div><div className="balance-actions"><button disabled title="Withdrawals are intentionally disabled in this local hackathon build"><WalletIcon/>Withdraw</button><a className="solid" href="/payments/new"><PlusIcon/>Create payment</a></div></div></section>

    <div className="stats-grid"><div className="metric-card"><div className="metric-top"><span>Total payments</span><span className="metric-icon"><PaymentIcon/></span></div><div className="metric-number">{overview?.payments?.total??payments.length}</div><div className="metric-caption">{overview?.payments?.completed??0} completed</div></div><div className="metric-card"><div className="metric-top"><span>Open claims</span><span className="metric-icon"><ClaimIcon/></span></div><div className="metric-number">{openClaims}</div><div className="metric-caption">Exceptions requiring investigation</div></div><div className="metric-card"><div className="metric-top"><span>Recovery available</span><span className="metric-icon"><ShieldIcon/></span></div><div className="metric-number">{actionable}</div><div className="metric-caption">Policy/simulation or approval stage</div></div></div>

    {openClaims>0?<div className="claim-alert"><div className="alert-icon"><ClaimIcon/></div><div><strong>{openClaims} open claim{openClaims===1?"":"s"} need attention</strong><span>{actionable>0?`${actionable} already reached a recovery or approval stage.`:"FlowPay is investigating the submitted exceptions."}</span></div><a href="/claims">Review claims <ArrowRightIcon/></a></div>:null}

    <section className="panel"><div className="panel-head"><h2>Recent payments</h2><a href="/payments">View all</a></div><div className="table-wrap"><table className="table"><thead><tr><th>Payment</th><th>Customer reference</th><th>Amount</th><th>Network</th><th>Status</th></tr></thead><tbody>{payments.map((p:any)=>{const status=p?.status??"UNKNOWN";return <tr key={p?.id}><td><a className="id-link" href={`/payments/${p?.id}`}>{p?.id??"—"}</a></td><td>{p?.reference??"—"}</td><td><span className="asset-cell"><img src={tokenAsset(p?.asset??"USDC")} alt=""/><span className="amount-cell">{p?.amount??"—"} {p?.asset??"—"}</span></span></td><td><span className="network-cell"><img src={networkAsset(p?.chain??"base")} alt=""/>{networkLabel(p?.chain??"base")}</span></td><td><span className={`status ${statusTone(status)}`}>{status.replaceAll("_"," ")}</span></td></tr>;})}</tbody></table>{payments.length===0?<div className="empty-note">No payments yet. Create the first test checkout.</div>:null}</div></section>
  </>;
  }catch(error){return <>
    <div className="page-head"><div><h1>FlowPay Merchant</h1><p>{error instanceof Error?error.message:"Failed to load overview."}</p></div><div className="actions"><a className="btn primary" href="/payments/new"><PlusIcon/>Create payment</a></div></div>
  </>;}
}
