"use client";

import Image from "next/image";
import {useCallback,useEffect,useMemo,useRef,useState} from "react";
import {Brand,LanguageButton} from "../../components/Brand";
import {
  ArrowLeftIcon,ClockIcon,CopyIcon,HeadphonesIcon,
  InfoIcon,LifebuoyIcon,ShieldIcon
} from "../../components/Icons";
import {QrCode} from "../../components/QrCode";

export type Payment={
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

export type Deposit={amount_atomic?:string;asset_symbol?:string;asset?:string;confirmation_status?:string};

type ChainMeta={label:string;asset:string};
const chainMeta:Record<string,ChainMeta>={
  base:{label:"Base",asset:"/assets/base.svg"},
  base_sepolia:{label:"Base Sepolia",asset:"/assets/base.svg"},
  bsc:{label:"BNB Smart Chain",asset:"/assets/bsc.svg"},
};
const terminal=new Set(["COMPLETED","RECOVERED","EXPIRED","FAILED","CANCELLED","ESCALATED"]);
const stableAssets=new Set(["USDC","USDT"]);

function shortAddress(value:string){return value.length>18?`${value.slice(0,7)}…${value.slice(-5)}`:value;}
function merchantName(payment:Payment){return payment.merchant_name?.trim()||"FlowPay merchant";}
function tokenIcon(asset:string){const symbol=asset.toUpperCase();if(symbol==="ETH")return "/assets/ethereum.svg";return symbol==="USDT"?"/assets/usdt.svg":"/assets/usdc.svg";}
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
function amountDisplay(amount:string,asset:string){const value=Number(amount);return stableAssets.has(asset.toUpperCase())&&Number.isFinite(value)?value.toFixed(2):amount;}
function expiryTime(value:string){
  const normalized=value.replace(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}(?:\.\d+)?) ([+-]\d{2}:\d{2}):\d{2}$/,"$1T$2$3");
  const parsed=new Date(normalized).getTime();
  return Number.isNaN(parsed)?Date.now():parsed;
}
export function CheckoutClient({paymentId,home=false,suppressOutcome=false,initialPayment=null,initialDeposits=[]}:{paymentId:string;home?:boolean;suppressOutcome?:boolean;initialPayment?:Payment|null;initialDeposits?:Deposit[]}){
  const [payment,setPayment]=useState<Payment|null>(initialPayment);
  const [deposits,setDeposits]=useState<Deposit[]>(initialDeposits);
  const [error,setError]=useState("");
  const [copied,setCopied]=useState(false);
  const [remaining,setRemaining]=useState(0);
  const loading=useRef(false);

  const load=useCallback(async()=>{
    if(loading.current)return;
    loading.current=true;
    try{
      const [response,depositsResponse]=await Promise.all([
        fetch(`/api/payment/${encodeURIComponent(paymentId)}`,{cache:"no-store"}),
        fetch(`/api/payment/${encodeURIComponent(paymentId)}/deposits`,{cache:"no-store"})
      ]);
      const body=await response.json();
      if(!response.ok)throw new Error(body?.error||"Unable to load payment");
      setPayment(body);
      setError("");
      if(depositsResponse.ok){
        const depositBody=await depositsResponse.json();
        setDeposits(Array.isArray(depositBody.data)?depositBody.data:[]);
      }
    }catch(err){
      setError(err instanceof Error?err.message:"Unable to load payment");
    }finally{
      loading.current=false;
    }
  },[paymentId]);

  useEffect(()=>{
    void load();
    const refresh=()=>{
      if(document.visibilityState==="visible"&&(!payment||!terminal.has(payment.status)))void load();
    };
    const timer=window.setInterval(refresh,750);
    window.addEventListener("focus",refresh);
    document.addEventListener("visibilitychange",refresh);
    return()=>{
      window.clearInterval(timer);
      window.removeEventListener("focus",refresh);
      document.removeEventListener("visibilitychange",refresh);
    };
  },[load,payment?.status]);

  useEffect(()=>{
    if(!payment)return;
    const tick=()=>setRemaining(Math.max(0,Math.floor((expiryTime(payment.expires_at)-Date.now())/1000)));
    tick();
    const timer=window.setInterval(tick,1000);
    return()=>window.clearInterval(timer);
  },[payment]);

  useEffect(()=>{
    const status=payment?.status;
    if(!suppressOutcome&&(status==="COMPLETED"||status==="RECOVERED"))window.location.replace(`/pay/${encodeURIComponent(paymentId)}/success`);
  },[payment?.status,paymentId,suppressOutcome]);

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
  const isDone=status==="COMPLETED"||status==="RECOVERED";
  // Show claim link for any non-terminal state, including expired — users who sent funds need recovery.
  const canClaim=!isDone;

  return <main className={`checkout-shell${home?" checkout-preview":""}`}>
    <header className="payment-header reference-header"><Brand/><span/><div className="checkout-header-actions"><LanguageButton/><button className="checkout-theme" type="button" aria-label="Display settings"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.42 1.42M17.65 17.65l1.42 1.42M2 12h2M20 12h2M4.93 19.07l1.42-1.42M17.65 6.35l1.42-1.42"/></svg></button></div></header>
    <section className="reference-checkout" aria-labelledby="payment-title">
      <div className="reference-summary">
        <div className="reference-summary-inner"><span className="paying-label">Paying {merchantName(payment)}</span><h1 id="payment-title"><strong>{amountDisplay(payment.amount,payment.asset)}</strong><small>{payment.asset}</small></h1>{stableAssets.has(payment.asset.toUpperCase())?<p>≈ {dollarDisplay(payment.amount,payment.asset)} USD</p>:null}
          <details className="reference-network"><summary><Image src={network.asset} width={24} height={24} alt=""/><strong>{network.label}</strong><span className="network-chevron">⌄</span></summary><div className="network-menu"><div className="network-option"><Image src={network.asset} width={24} height={24} alt=""/><span><strong>{network.label}</strong><small>Required for this payment</small></span><b aria-label="Selected">✓</b></div></div></details>
          <div className="secure-copy"><ShieldIcon/><p><strong>Your payment is secure and encrypted.</strong><span>We never store your funds.</span></p></div><div className="summary-rule"/>
          <div className="expiry-copy"><ClockIcon/><p><span>Payment expires in</span><strong>{time}</strong></p></div>
        </div>
      </div>
      <div className="reference-payment"><div className="reference-payment-inner"><h2>Send <strong>{amountDisplay(payment.amount,payment.asset)} {payment.asset}</strong> to the address below</h2><div className="reference-qr"><div className="reference-qr-frame"><QrCode value={payment.address}/><div className="reference-qr-brand"><Image src="/assets/flowpay-mark.svg" width={40} height={40} alt="FlowPay"/></div></div></div>
        <div className="reference-address" title={payment.address}><code>{shortAddress(payment.address)}</code><button type="button" onClick={()=>void copy()} aria-label="Copy payment address"><CopyIcon/></button></div>
        <div className="reference-warning"><InfoIcon/><p><strong>Only send {payment.asset} on {network.label}.</strong><span>Other assets or networks may be lost.</span></p></div>
      </div></div>
    </section>
    <footer className="reference-footer"><a href="#"><ArrowLeftIcon/>Cancel payment</a><a href="mailto:support@flowpay.dev"><HeadphonesIcon/>Contact support</a></footer>
    {canClaim?<a className="checkout-claim-fab" href={`/claim?payment_id=${encodeURIComponent(payment.id)}`}><LifebuoyIcon/><span>Create a claim</span><i/></a>:null}
  </main>;
}
