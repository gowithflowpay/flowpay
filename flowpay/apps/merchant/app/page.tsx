import {api} from "../lib/api";
import {moneyFromStableBalances,statusTone} from "../lib/format";
import {AlertIcon,ArrowRightIcon,CircleCheckIcon,ClockIcon,LinkIcon,PaymentIcon,WalletIcon} from "./components/Icons";

const terminal=new Set(["COMPLETED","FAILED","EXPIRED","CANCELLED","RECOVERED"]);

export default async function Dashboard(){
  let payments:any[]=[];
  let balance="—";
  let unavailable=false;
  try{
    const [overview,result]=await Promise.all([api("/v1/merchant/overview"),api("/v1/payments?limit=100")]);
    payments=result?.data??[];
    balance=moneyFromStableBalances(overview?.balances??[]);
  }catch{unavailable=true}

  const recent=payments.slice(0,5);
  const stats=monthlyStats(payments);

  return <div className="dashboard-page">
    <section className="welcome-row">
      <div><h1>Welcome back, Acme Store <span aria-hidden="true">👋</span></h1><p>Here&apos;s what&apos;s happening with your business today.</p></div>
      <a className="create-payment" href="/payments/new"><LinkIcon/>Create payment link</a>
    </section>
    {unavailable?<p className="data-notice" role="status">Live payment data is unavailable. The API on port 8080 is offline.</p>:null}

    <div className="dashboard-summary-grid">
      <section className="balance-card">
        <div className="balance-copy"><span>Available balance</span><strong>{balance}</strong><p><img src="/assets/usdc.svg" alt=""/>{balance==="—"?"Unable to load USDC balance":`≈ ${balance.replace("$","")} USDC`}</p></div>
        <div className="balance-actions"><a href="/claims">Transfer funds</a><a href="/payments">View balance details</a></div>
      </section>
      <section className="payments-total-card">
        <div className="payments-total-heading"><span>Total payments</span><i title="Completed payments received this month">i</i></div>
        <button type="button">This month <span>⌄</span></button>
        <div className="payments-total-content">
          <div><strong>{formatMoney(stats.current)}</strong><p className={stats.change>=0?"positive":"negative"}>{stats.change>=0?"↑":"↓"} {Math.abs(stats.change).toFixed(1)}% <span>from last month</span></p></div>
          <svg className="payments-chart" viewBox="0 0 190 100" role="img" aria-label="Payments trend">
            <defs><linearGradient id="chartFill" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#6c5ce7" stopOpacity=".22"/><stop offset="1" stopColor="#6c5ce7" stopOpacity="0"/></linearGradient></defs>
            <path className="chart-area" d="M4 88 C26 86 29 65 49 64 S72 75 89 52 S112 49 124 35 S148 35 160 13 S178 13 186 5 L186 100 L4 100 Z"/>
            <path className="chart-line" d="M4 88 C26 86 29 65 49 64 S72 75 89 52 S112 49 124 35 S148 35 160 13 S178 13 186 5"/>
            <circle cx="186" cy="5" r="4"/>
          </svg>
        </div>
      </section>
    </div>

    <section className="activity-card">
      <header><h2>Recent activity</h2><a href="/payments">View all <ArrowRightIcon/></a></header>
      <div className="activity-tabs"><b>Recent transfers</b><a href="/payments">Recent payments</a></div>
      <div className="activity-table"><div className="activity-head"><span>Type</span><span>Description</span><span>Status</span><span>Date</span><span>Amount</span></div>
        {recent.map((p:any)=>{const status=String(p.status??"WAITING");return <a className="activity-row" href={`/payments/${p.id}`} key={p.id}><span className="activity-icon">{terminal.has(status)?<PaymentIcon/>:<WalletIcon/>}</span><strong>{p.reference||`${p.asset} payment`}</strong><span className={`activity-status ${statusTone(status)}`}>{statusIcon(status)}{humanize(status)}</span><time>{formatDate(p.updated_at??p.created_at)}</time><b>{p.amount} {p.asset}</b></a>})}
        {recent.length===0?<div className="activity-empty">{unavailable?"Transfers will appear when the FlowPay API is running.":"No payment activity yet."}</div>:null}
      </div>
      <a className="activity-footer" href="/payments">View all activity <ArrowRightIcon/></a>
    </section>
  </div>
}

function humanize(v:string){return v.replaceAll("_"," ").toLowerCase().replace(/(^|\s)\S/g,x=>x.toUpperCase())}
function statusIcon(status:string){if(["COMPLETED","RECOVERED","CONFIRMED"].includes(status))return <CircleCheckIcon/>;if(["EXPIRED","FAILED","CANCELLED"].includes(status))return <AlertIcon/>;return <ClockIcon/>}
function formatDate(v:any){if(!v)return "—";const d=new Date(v);if(Number.isNaN(d.getTime()))return "—";return <>{new Intl.DateTimeFormat("en-US",{month:"short",day:"numeric",year:"numeric"}).format(d)}<small>{new Intl.DateTimeFormat("en-US",{hour:"numeric",minute:"2-digit"}).format(d)}</small></>}
function formatMoney(amount:number){return new Intl.NumberFormat("en-US",{style:"currency",currency:"USD",minimumFractionDigits:2}).format(amount)}
function monthlyStats(payments:any[]){
  const now=new Date();
  const currentStart=new Date(now.getFullYear(),now.getMonth(),1).getTime();
  const previousStart=new Date(now.getFullYear(),now.getMonth()-1,1).getTime();
  let current=0,previous=0;
  for(const payment of payments){
    if(!["COMPLETED","RECOVERED","CONFIRMED"].includes(String(payment.status??"")))continue;
    const when=new Date(payment.updated_at??payment.created_at??0).getTime();
    const amount=Number(payment.amount??0);
    if(!Number.isFinite(when)||!Number.isFinite(amount))continue;
    if(when>=currentStart)current+=amount;else if(when>=previousStart)previous+=amount;
  }
  const change=previous>0?((current-previous)/previous)*100:current>0?100:0;
  return {current,change};
}
