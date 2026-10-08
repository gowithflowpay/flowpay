import {api} from "../../lib/api";
import {requireMerchant} from "../../lib/session";
import {moneyFromStableBalances,networkLabel,statusTone} from "../../lib/format";
import {ArrowRightIcon,LinkIcon,PaymentIcon,WalletIcon} from "../components/Icons";
import {RefreshData} from "../components/RefreshData";
const completed=new Set(["COMPLETED","RECOVERED","CONFIRMED"]);
const closed=new Set(["COMPLETED","RECOVERED","CONFIRMED","FAILED","EXPIRED","CANCELLED"]);
export default async function Dashboard(){
 const merchant=await requireMerchant();
 const [overview,result]=await Promise.allSettled([api("/v1/merchant/overview",{signal:AbortSignal.timeout(10000)}),api("/v1/payments?limit=100",{signal:AbortSignal.timeout(10000)})]);
 const payments:any[]=result.status==="fulfilled"?result.value?.data??[]:[];
 const stable=overview.status==="fulfilled"?(overview.value?.balances??[]).filter((b:any)=>["USDC","USDT"].includes(String(b.symbol).toUpperCase())):[];
 const balanceAvailable=stable.length>0&&stable.every((b:any)=>!b.error&&b.amount_atomic!==undefined);
 let balance:string|null=null;if(balanceAvailable){try{balance=moneyFromStableBalances(stable);}catch{}}
 const recent=payments.slice(0,5);const settled=payments.filter(p=>completed.has(p.status));
 const totals=new Map<string,number>();for(const p of settled){const amount=Number(p.amount);if(Number.isFinite(amount))totals.set(p.asset,(totals.get(p.asset)??0)+amount);}
 const counts=overview.status==="fulfilled"?overview.value?.payments:null;
 return <div className="workspace-page">
  <header className="workspace-heading"><div><span className="workspace-eyebrow">YOUR WORKSPACE</span><h1>Overview</h1><p>Welcome back, {merchant.business_name}. Here is your payment activity.</p></div><a className="workspace-primary" href="/payments/new"><LinkIcon/>Create payment link</a></header>
  <div className="overview-stats">
   <section className="workspace-card metric"><div className="metric-label"><span>Settlement wallet balance</span><WalletIcon/></div><strong>{balance??"\u2014"}</strong><p>{balance?"Combined USDC and USDT across supported networks":"Wallet balance is currently unavailable."}</p><a href="/settings">{merchant.settlement_address?"View settlement wallet":"Connect settlement wallet"}<ArrowRightIcon/></a></section>
   <section className="workspace-card metric"><div className="metric-label"><span>Total payments</span><PaymentIcon/></div><strong>{counts?.total??(result.status==="fulfilled"?payments.length:"\u2014")}</strong><p>{counts?"All payment requests":result.status==="fulfilled"?"In the latest 100 requests":"Payment data could not be loaded."}</p><a href="/history">Explore history<ArrowRightIcon/></a></section>
   <section className="workspace-card metric"><div className="metric-label"><span>Completed payments</span><span className="metric-dot"/></div><strong>{counts?.completed??(result.status==="fulfilled"?settled.length:"\u2014")}</strong><p>{totals.size?[...totals].map(([asset,amount])=>`${amount.toLocaleString("en-US",{maximumFractionDigits:6})} ${asset}`).join(" \u00b7 "):result.status==="fulfilled"?"Completed volume appears after your first payment.":"Payment data could not be loaded."}</p><span className="metric-footnote">Volume from the latest 100 requests</span></section>
  </div>
  <section className="workspace-card recent-panel"><header className="workspace-section-heading"><div><h2>Recent payments</h2><p>Your latest requests and their current status.</p></div><a href="/history">View history<ArrowRightIcon/></a></header>
  {result.status==="rejected"?<div className="workspace-empty" role="status"><PaymentIcon/><h3>Payments could not be loaded</h3><p>Try again to retrieve your latest activity.</p><RefreshData/></div>:recent.length===0?<div className="workspace-empty"><PaymentIcon/><h3>Your first payment starts here</h3><p>Create a payment link and share it with your customer.</p><a className="workspace-primary" href="/payments/new">Create payment link<ArrowRightIcon/></a></div>:<div className="workspace-table-wrap"><table className="workspace-table"><thead><tr><th>Payment</th><th>Network</th><th>Status</th><th className="align-right">Amount</th></tr></thead><tbody>{recent.map(p=><tr key={p.id}><td><a href={`/payments/${encodeURIComponent(p.id)}`}><strong>{p.reference||"Payment request"}</strong><small>{p.id}</small></a></td><td>{networkLabel(p.chain)}</td><td><span className={`workspace-status ${statusTone(p.status)}`}>{String(p.status).replaceAll("_"," ").toLowerCase()}</span></td><td className="align-right"><strong>{p.amount} {p.asset}</strong></td></tr>)}</tbody></table></div>}
  </section><div className="overview-bottom"><section className="workspace-card workspace-shortcut"><div><span className="workspace-eyebrow">SETTLEMENT</span><h2>Your funds, your wallet</h2><p>{merchant.settlement_address?"Payments settle to your configured wallet. Review your address and account details in Settings.":"Add a settlement wallet to start receiving crypto payments."}</p></div><a href="/settings">Manage wallet<ArrowRightIcon/></a></section><section className="workspace-card workspace-shortcut"><div><span className="workspace-eyebrow">IN PROGRESS</span><h2>{result.status==="fulfilled"?payments.filter(p=>!closed.has(p.status)).length:"\u2014"} active requests</h2><p>Monitor pending payments and follow up on requests that need attention.</p></div><a href="/payments">View payments<ArrowRightIcon/></a></section></div>
 </div>;
}
