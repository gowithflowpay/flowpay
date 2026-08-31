"use client";

import Image from "next/image";
import {useCallback,useEffect,useMemo,useState} from "react";
import {Brand,LanguageButton} from "../../components/Brand";
import {
  ArrowRightIcon,CheckIcon,ClockIcon,CopyIcon,
  InfoIcon,LifebuoyIcon,LockIcon
} from "../../components/Icons";
import {QrCode} from "../../components/QrCode";

type Payment={
  id:string;
  address:string;
  amount:string;
  amount_atomic?:string;
  asset:string;
  chain:string;
  status:string;
  expires_at:string;
  reference?:string|null;
  merchant_name?:string|null;
  checkout_url?:string;
};

type Deposit={amount_atomic?:string;asset_symbol?:string;asset?:string;confirmation_status?:string};

type ChainMeta={label:string;asset:string};
const chainMeta:Record<string,ChainMeta>={
  base:{label:"Base",asset:"/assets/base.svg"},
  bsc:{label:"BNB Smart Chain",asset:"/assets/bsc.svg"},
};
const terminal=new Set(["COMPLETED","RECOVERED","EXPIRED","FAILED","CANCELLED","ESCALATED"]);
const stableAssets=new Set(["USDC","USDT"]);

function shortAddress(value:string){return value.length>18?`${value.slice(0,7)}…${value.slice(-5)}`:value;}
function merchantName(payment:Payment){return payment.merchant_name?.trim()||"FlowPay merchant";}
function tokenIcon(asset:string){return asset.toUpperCase()==="USDT"?"/assets/usdt.svg":"/assets/usdc.svg";}
function dollarDisplay(amount:string,asset:string){
  if(!stableAssets.has(asset.toUpperCase()))return amount;
  const raw=amount.trim().replace(/^\+/,"");
  const [wholeRaw="0",fractionRaw=""]=raw.split(".",2);
  const sign=wholeRaw.startsWith("-")?"-":"";
  const whole=wholeRaw.replace("-","").replace(/^0+(?=\d)/,"")||"0";
  const grouped=whole.replace(/\B(?=(\d{3})+(?!\d))/g,",");
  const fraction=(fractionRaw+"00").slice(0,2);
  return `${sign}$${grouped}.${fraction}`;
}
function stateCopy(status:string){
  switch(status){
    case "DETECTED":return ["Payment detected","We found your transaction and are verifying it on-chain."];
    case "CONFIRMING":return ["Confirming payment","Waiting for the required blockchain confirmations."];
    case "PARTIALLY_PAID":return ["Partial payment received","Send the remaining amount to the same checkout address."];
    case "CONFIRMED":return ["Payment confirmed","Your payment has enough confirmations."];
    case "SETTLING":return ["Finalizing payment","FlowPay is settling the confirmed payment to the merchant."];
    case "COMPLETED":return ["Payment complete","The merchant has been notified successfully."];
    case "RECOVERED":return ["Funds recovered","The approved recovery was verified on-chain."];
    case "OVERPAID":return ["Overpayment detected","Your payment was received. The merchant overpayment policy is being applied."];
    case "WRONG_ASSET":return ["Different asset detected","This checkout did not receive the expected token. You can open a recovery claim below."];
    case "WRONG_CHAIN_CLAIMED":return ["Wrong network reported","Your claim is being investigated against the reported network."];
    case "CLAIM_PENDING":return ["Claim in progress","FlowPay is investigating your payment exception."];
    case "RECOVERY_AVAILABLE":return ["Recovery available","A constrained recovery plan is available and requires approval before execution."];
    case "RECOVERY_PENDING":return ["Recovery pending","The approved recovery is being verified and executed."];
    case "ESCALATED":return ["Manual review required","FlowPay could not safely resolve this case automatically."];
    case "EXPIRED":return ["Invoice expired","Do not send funds to this checkout. Request a new payment from the merchant."];
    case "FAILED":return ["Payment could not complete","Contact the merchant or create a claim if you already sent funds."];
    case "CANCELLED":return ["Payment cancelled","This checkout is no longer accepting payment."];
    default:return ["Waiting for payment","Send the exact asset on the exact network shown above."];
  }
}

export function CheckoutClient({paymentId,home=false}:{paymentId:string;home?:boolean}){
  const [payment,setPayment]=useState<Payment|null>(null);
  const [deposits,setDeposits]=useState<Deposit[]>([]);
  const [error,setError]=useState("");
  const [copied,setCopied]=useState(false);
  const [remaining,setRemaining]=useState(0);

  const load=useCallback(async()=>{
    try{
      const response=await fetch(`/api/payment/${encodeURIComponent(paymentId)}`,{cache:"no-store"});
      const body=await response.json();
      if(!response.ok)throw new Error(body?.error||"Unable to load payment");
      setPayment(body);
      setError("");
      const depositsResponse=await fetch(`/api/payment/${encodeURIComponent(paymentId)}/deposits`,{cache:"no-store"});
      if(depositsResponse.ok){
        const depositBody=await depositsResponse.json();
        setDeposits(Array.isArray(depositBody.data)?depositBody.data:[]);
      }
    }catch(err){
      setError(err instanceof Error?err.message:"Unable to load payment");
    }
  },[paymentId]);

  useEffect(()=>{
    void load();
    const timer=window.setInterval(()=>{
      if(!payment||!terminal.has(payment.status))void load();
    },3500);
    return()=>window.clearInterval(timer);
  },[load,payment?.status]);

  useEffect(()=>{
    if(!payment)return;
    const tick=()=>setRemaining(Math.max(0,Math.floor((new Date(payment.expires_at).getTime()-Date.now())/1000)));
    tick();
    const timer=window.setInterval(tick,1000);
    return()=>window.clearInterval(timer);
  },[payment]);

  const time=useMemo(()=>`${String(Math.floor(remaining/60)).padStart(2,"0")}:${String(remaining%60).padStart(2,"0")}`,[remaining]);

  const copy=async()=>{
    if(!payment)return;
    await navigator.clipboard.writeText(payment.address);
    setCopied(true);
    window.setTimeout(()=>setCopied(false),1400);
  };

  if(error)return <main className="checkout-shell">
    <header className="payment-header"><span/><Brand/><LanguageButton/></header>
    <section className="checkout-card error-card">
      <div className="icon-bubble"><InfoIcon/></div>
      <h1>Checkout unavailable</h1><p>{error}</p>
      <button className="outline-button" onClick={()=>void load()}>Try again</button>
    </section>
  </main>;

  if(!payment)return <main className="checkout-shell">
    <header className="payment-header"><span/><Brand/><LanguageButton/></header>
    <div className="checkout-card skeleton-card">
      <div className="skeleton circle"/><div className="skeleton line w30"/><div className="skeleton line w55"/><div className="skeleton qr"/>
    </div>
  </main>;

  const network=chainMeta[payment.chain]??{label:payment.chain,asset:"/assets/base.svg"};
  const status=payment.status||"WAITING";
  const [statusTitle,statusText]=stateCopy(status);
  const isDone=status==="COMPLETED"||status==="RECOVERED";
  // Show claim link for any non-terminal state, including expired — users who sent funds need recovery.
  const canClaim=!isDone;

  return <main className={`checkout-shell${home?" checkout-preview":""}`}>
    <header className="payment-header"><span/><Brand/><LanguageButton/></header>

    <section className="checkout-card desktop-checkout live-desktop-checkout" aria-labelledby="payment-title">
      <div className="merchant-block">
        <div className="merchant-icon"><Image src="/assets/storefront.svg" width={78} height={78} alt="" priority/></div>
        <span>Pay to</span>
        <h1 id="payment-title">{merchantName(payment)}</h1>
      </div>

      <div className="soft-divider"/>

      <div className="amount-block">
        <span className="eyebrow">Amount</span>
        <div className="fiat-like">{dollarDisplay(payment.amount,payment.asset)}</div>
        <div className="asset-line"><Image src={tokenIcon(payment.asset)} width={25} height={25} alt=""/><strong>{payment.amount} {payment.asset}</strong></div>
      </div>

      <div className="network-section">
        <span className="eyebrow">Network</span>
        <div className="network-pill"><Image src={network.asset} width={31} height={31} alt=""/><strong>{network.label}</strong><span className="verified-dot">Verified</span></div>
      </div>

      <div className="qr-wrap">
        <div className="qr-frame">
          <QrCode value={payment.address}/>
          <div className="qr-brand"><Image src="/assets/flowpay-mark.svg" width={44} height={44} alt="FlowPay"/></div>
        </div>
      </div>

      <div className="address-row" title={payment.address}>
        <code>{shortAddress(payment.address)}</code>
        <button type="button" onClick={()=>void copy()} aria-label="Copy payment address"><CopyIcon/>{copied?"Copied":"Copy"}</button>
      </div>

      <div className="notice-row">
        <div className="round-icon"><InfoIcon/></div>
        <div><strong>Send only {payment.asset} on {network.label}</strong><span>Sending any other asset or network may prevent this order from completing automatically.</span></div>
      </div>

      <div className={`status-row status-${status.toLowerCase()}`}>
        <div className="round-icon">{isDone?<CheckIcon/>:<ClockIcon/>}</div>
        <div>
          <strong>{statusTitle}</strong>
          <span>{statusText}{status==="WAITING"&&remaining>0?<> This invoice expires in <b>{time}</b>.</>:null}</span>
          {deposits.length>0&&status==="PARTIALLY_PAID"?<small>{deposits.length} deposit{deposits.length===1?"":"s"} detected so far.</small>:null}
        </div>
      </div>

      {canClaim?<div className="checkout-help-grid checkout-help-single">
        <a href={`/claim?payment_id=${encodeURIComponent(payment.id)}`} className="help-tile">
          <div className="round-icon"><LifebuoyIcon/></div>
          <div><strong>Payment problem?</strong><span>If you sent the wrong asset or network, you can try to recover your funds.</span><b>Recover funds <ArrowRightIcon/></b></div>
        </a>
      </div>:null}

      {isDone?<div className="complete-panel"><div className="complete-check"><CheckIcon/></div><div><strong>You&apos;re all set</strong><span>You can safely return to {merchantName(payment)}.</span></div></div>:null}
    </section>

    <footer className="public-footer">
      <span><LockIcon/> Secured by FlowPay</span>
      <nav><a href="#">Terms</a><a href="#">Privacy</a><a href="mailto:support@flowpay.dev">Support</a></nav>
    </footer>
  </main>;
}
