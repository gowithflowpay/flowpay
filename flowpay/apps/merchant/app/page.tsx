import type {ReactNode} from "react";
import {api} from "../lib/api";
import {moneyFromStableBalances,short,statusTone,tokenAsset} from "../lib/format";
import {ArrowRightIcon,CircleCheckIcon,ClockIcon,LinkIcon,PlusIcon,VolumeIcon} from "./components/Icons";

const terminalPaymentStatuses=new Set(["COMPLETED","FAILED","EXPIRED","CANCELLED"]);
const terminalClaimStatuses=new Set(["RECOVERED","REJECTED","NOT_RECOVERABLE"]);

export default async function Overview(){
  try{
    const [overview,paymentsResult,claimsResult]=await Promise.all([api("/v1/merchant/overview"),api("/v1/payments?limit=6"),api("/v1/claims?limit=4").catch(()=>({data:[]}))]);
    const payments=paymentsResult?.data??[],claims=claimsResult?.data??[],balances=overview?.balances??[];
    const completed=Number(overview?.payments?.completed??0);
    const pending=payments.filter((payment:any)=>!terminalPaymentStatuses.has(String(payment?.status??""))).length;
    const links=payments.filter((payment:any)=>payment?.checkout_url||payment?.address).length;
    const attentionClaims=claims.filter((claim:any)=>!terminalClaimStatuses.has(String(claim?.status??"")));
    return <div className="overview-page">
      <header className="overview-titlebar"><div><h1>Overview</h1><p>Monitor payments, links, and recovery activity.</p></div><a className="btn primary overview-cta" href="/payments/new"><PlusIcon/>Create payment link</a></header>
      <section className="overview-metrics" aria-label="Payment summary">
        <Metric icon={<VolumeIcon/>} label="Total volume" value={moneyFromStableBalances(balances)}/>
        <Metric icon={<CircleCheckIcon/>} label="Successful payments" value={completed}/>
        <Metric icon={<ClockIcon/>} label="Pending payments" value={pending}/>
        <Metric icon={<LinkIcon/>} label="Active payment links" value={links}/>
      </section>
      <div className="overview-content">
        <section className="overview-card payments-card"><div className="overview-card-head"><div><h2>Recent payments</h2><p>Your latest payment activity</p></div><a href="/payments">View all <ArrowRightIcon/></a></div>
          <div className="overview-payment-list"><div className="overview-payment-header"><span>Payment</span><span>Reference</span><span>Amount</span><span>Status</span><span>Date</span></div>
            {payments.map((payment:any)=>{const status=String(payment?.status??"UNKNOWN");return <a className="overview-payment-row" href={`/payments/${payment?.id}`} key={payment?.id}><span className="payment-primary"><span className="asset-mark"><img src={tokenAsset(payment?.asset??"USDC")} alt=""/></span><span><strong>{short(payment?.id,8,4)}</strong><small>{payment?.asset??"Asset"} payment</small></span></span><span className="payment-reference">{payment?.reference??"No reference"}</span><strong className="payment-amount">{payment?.amount??"—"} <small>{payment?.asset??""}</small></strong><span><span className={`status ${statusTone(status)}`}>{humanize(status)}</span></span><time>{formatDate(payment?.updated_at??payment?.created_at??payment?.expires_at)}</time></a>})}
            {payments.length===0?<div className="overview-empty"><strong>No payments yet</strong><span>Create a payment link to start accepting payments.</span></div>:null}
          </div>
        </section>
        <aside className="overview-card claims-card"><div className="overview-card-head"><div><h2>Recent claims</h2><p>Recovery activity</p></div><a href="/claims">View all <ArrowRightIcon/></a></div><div className="claims-summary"><strong>{attentionClaims.length}</strong><span>requiring attention</span></div>
          <div className="claim-list">{claims.slice(0,3).map((claim:any)=>{const status=String(claim?.status??"UNKNOWN");return <a href={`/claims/${claim?.id}`} className="claim-row" key={claim?.id}><span className="claim-status-mark" data-tone={statusTone(status)}/><span><strong>{claim?.actual_asset??"Asset"} on {humanize(claim?.actual_chain??"Unknown network")}</strong><small>{short(claim?.id,7,4)}</small></span><span className={`status ${statusTone(status)}`}>{humanize(status)}</span></a>})}{claims.length===0?<div className="claims-empty"><span className="claims-check"><CircleCheckIcon/></span><strong>No claims need attention</strong><p>Recovery cases will appear here when action is required.</p></div>:null}</div>
          <a className="claims-footer" href="/claims">Open claims center <ArrowRightIcon/></a>
        </aside>
      </div>
    </div>;
  }catch(error){return <div className="overview-page"><header className="overview-titlebar"><div><h1>Overview</h1><p>{error instanceof Error?error.message:"Failed to load overview."}</p></div><a className="btn primary" href="/payments/new"><PlusIcon/>Create payment link</a></header></div>}
}

function Metric({icon,label,value}:{icon:ReactNode,label:string,value:string|number}){return <article className="overview-metric"><div className="overview-metric-top"><span className="overview-metric-label">{label}</span><span className="overview-metric-icon">{icon}</span></div><strong className="overview-metric-value">{value}</strong></article>}
function humanize(value:string){return value.replaceAll("_"," ").toLowerCase().replace(/(^|\s)\S/g,letter=>letter.toUpperCase())}
function formatDate(value:any){if(!value)return "—";const date=new Date(value);return Number.isNaN(date.getTime())?"—":new Intl.DateTimeFormat("en-US",{month:"short",day:"numeric",year:"numeric"}).format(date)}
