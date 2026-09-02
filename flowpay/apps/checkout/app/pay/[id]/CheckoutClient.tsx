"use client";

import Image from "next/image";
import {useCallback,useEffect,useMemo,useState} from "react";
import {Brand,LanguageButton} from "../../components/Brand";
import {
  ArrowLeftIcon,CheckIcon,ClockIcon,CopyIcon,HeadphonesIcon,
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
  bsc_testnet:{label:"BNB Smart Chain Testnet",asset:"/assets/bsc.svg"},
  ethereum:{label:"Ethereum",asset:"/assets/ethereum.svg"},
  ethereum_sepolia:{label:"Ethereum Sepolia",asset:"/assets/ethereum.svg"},
  arbitrum:{label:"Arbitrum",asset:"/assets/ethereum.svg"},
  arbitrum_sepolia:{label:"Arbitrum Sepolia",asset:"/assets/ethereum.svg"},
};
const terminal=new Set(["COMPLETED","RECOVERED","EXPIRED","FAILED","CANCELLED","ESCALATED"]);
const successStates=new Set(["CONFIRMED","SETTLING","COMPLETED","RECOVERED"]);
const stableAssets=new Set(["USDC","USDT"]);

function shortAddress(value:string){return value.length>18?`${value.slice(0,7)}…${value.slice(-5)}`:value;}
function merchantName(payment:Payment){return payment.merchant_name?.trim()||"FlowPay merchant";}
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
function expiryTime(value:string){const normalized=value.replace(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}(?:\.\d+)?) ([+-]\d{2}:\d{2}):\d{2}$/,"$1T$2$3");const parsed=new Date(normalized).getTime();return Number.isNaN(parsed)?Date.now():parsed;}
function stateCopy(status:string){
  switch(status){
    case "DETECTED":return ["Payment detected","We found your transaction and are verifying it on-chain."];
    case "CONFIRMING":return ["Confirming payment","Waiting for the required blockchain confirmations."];
    case "PARTIALLY_PAID":return ["Partial payment received","Send the remaining amount to the same checkout address."];
    case "CONFIRMED":return ["Payment confirmed!","Your payment was received and verified. Finalizing now..."];
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

type CheckoutClientProps={paymentId:string;home?:boolean;suppressOutcome?:boolean;initialPayment?:Payment|null;initialDeposits?:Deposit[]};

export function CheckoutClient({paymentId,home=false,suppressOutcome=false,initialPayment=null,initialDeposits=[]}:CheckoutClientProps){
  const [payment,setPayment]=useState<Payment|null>(initialPayment);
  const [deposits,setDeposits]=useState<Deposit[]>(initialDeposits);
  const [error,setError]=useState("");
  const [copied,setCopied]=useState(false);
  const [remaining,setRemaining]=useState(0);
  const [showSuccess,setShowSuccess]=useState(false);
  const [chatOpen,setChatOpen]=useState(false);
  const [chatMessages,setChatMessages]=useState<{role:string;content:string}[]>([]);
  const [chatInput,setChatInput]=useState("");
  const [chatLoading,setChatLoading]=useState(false);

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

  const paymentStatus=payment?.status;
  useEffect(()=>{
    if(suppressOutcome||!paymentStatus||!successStates.has(paymentStatus))return;
    setShowSuccess(true);
    const timer=window.setTimeout(()=>window.location.replace(`/pay/${encodeURIComponent(paymentId)}/success`),2200);
    return()=>window.clearTimeout(timer);
  },[paymentId,paymentStatus,suppressOutcome]);

  useEffect(()=>{
    void load();
    const timer=window.setInterval(()=>{
      if(!payment||!terminal.has(payment.status))void load();
    },1000);
    return()=>window.clearInterval(timer);
  },[load,payment?.status]);

  useEffect(()=>{
    if(!payment)return;
    const tick=()=>setRemaining(Math.max(0,Math.floor((expiryTime(payment.expires_at)-Date.now())/1000)));
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

  const sendChat=async()=>{
    if(!chatInput.trim()||chatLoading||!payment)return;
    const userMsg={role:"user",content:chatInput.trim()};
    const updated=[...chatMessages,userMsg];
    setChatMessages(updated);
    setChatInput("");
    setChatLoading(true);
    try{
      const resp=await fetch(`/api/payment/${encodeURIComponent(paymentId)}/agent`,{
        method:"POST",
        headers:{"Content-Type":"application/json"},
        body:JSON.stringify({payment_id:payment.id,messages:updated}),
      });
      const body=await resp.json();
      const agentMsg={role:"agent",content:body.reply||"I'm here to help."};
      setChatMessages([...updated,agentMsg]);
      if(body.status==="CLAIM_CREATED"){
        setChatMessages([...updated,agentMsg,{role:"system",content:`Claim ${body.claim_id} created. Our team will investigate and process your refund.`}]);
      }
    }catch{
      setChatMessages([...updated,{role:"agent",content:"Sorry, I couldn't process that. Please try again."}]);
    }
    setChatLoading(false);
  };

  const openChat=()=>{
    setChatOpen(true);
    if(chatMessages.length===0){
      setChatMessages([{role:"agent",content:"Hi! I'm FlowPay's support agent. I can help if you sent the wrong asset or had a payment issue. What's the problem?"}]);
    }
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
  const isDone=status==="COMPLETED"||status==="RECOVERED"||status==="CONFIRMED"||status==="SETTLING";
  // Show claim link for any non-terminal state, including expired — users who sent funds need recovery.
  const canClaim=!isDone;

  return <main className={`checkout-shell${home?" checkout-preview":""}`}>
    <header className="payment-header reference-header"><Brand/><span/><div className="checkout-header-actions"><LanguageButton/><button className="checkout-theme" type="button" aria-label="Display settings">☼</button></div></header>
    <section className="reference-checkout" aria-labelledby="payment-title">
      <div className="reference-summary"><div className="reference-summary-inner">
        <span className="paying-label">Paying {merchantName(payment)}</span>
        <h1 id="payment-title"><strong>{amountDisplay(payment.amount,payment.asset)}</strong><small>{payment.asset}</small></h1>
        {stableAssets.has(payment.asset.toUpperCase())?<p>≈ {dollarDisplay(payment.amount,payment.asset)} USD</p>:null}
        <details className="reference-network"><summary><Image src={network.asset} width={24} height={24} alt=""/><strong>{network.label}</strong><span className="network-chevron">⌄</span></summary></details>
        <div className="secure-copy"><ShieldIcon/><p><strong>Your payment is secure and encrypted.</strong><span>We never store your funds.</span></p></div>
        <div className="summary-rule"/>
        <div className="expiry-copy"><ClockIcon/><p><span>Payment expires in</span><strong>{time}</strong></p></div>
      </div></div>
      <div className="reference-payment"><div className="reference-payment-inner">
        <h2>Send <strong>{amountDisplay(payment.amount,payment.asset)} {payment.asset}</strong> to the address below</h2>
        <div className="reference-qr"><div className="reference-qr-frame"><QrCode value={payment.address}/><div className="reference-qr-brand"><Image src="/assets/flowpay-mark.svg" width={40} height={40} alt="FlowPay"/></div></div></div>
        <div className="reference-address" title={payment.address}><code>{shortAddress(payment.address)}</code><button type="button" onClick={()=>void copy()} aria-label="Copy payment address"><CopyIcon/></button></div>
        {copied?<span className="reference-copied" role="status">Address copied</span>:null}
        <div className="reference-warning"><InfoIcon/><p><strong>Only send {payment.asset} on {network.label}.</strong><span>Other assets or networks may be lost.</span></p></div>
        <div className={`reference-status status-${status.toLowerCase()}`} role="status" aria-live="polite"><span>{isDone?<CheckIcon/>:<ClockIcon/>}</span><p><strong>{statusTitle}</strong><small>{statusText}{deposits.length>0&&status==="PARTIALLY_PAID"?` ${deposits.length} deposits detected.`:""}</small></p></div>
      </div></div>

    </section>

    {showSuccess?<div className="payment-success-backdrop" role="presentation">
      <section className="payment-success-modal" role="dialog" aria-modal="true" aria-labelledby="payment-success-title" aria-describedby="payment-success-copy">
        <div className="payment-success-check"><CheckIcon/><i/><i/><i/><i/></div>
        <h2 id="payment-success-title">Payment received!</h2>
        <p id="payment-success-copy">{payment.amount} {payment.asset} was received and confirmed on-chain.</p>
        <div className="payment-success-summary">
          <span>Amount <strong>{payment.amount} {payment.asset}</strong></span>
          <span>Network <strong>{network.label}</strong></span>
          <span>Payment ID <strong>{payment.id}</strong></span>
        </div>
        <button type="button" onClick={()=>window.location.replace(`/pay/${encodeURIComponent(paymentId)}/success`)}>View confirmation</button>
        <small>Redirecting securely…</small>
      </section>
    </div>:null}

    <footer className="reference-footer"><a href="/"><ArrowLeftIcon/>Cancel payment</a><a href="mailto:support@flowpay.dev"><HeadphonesIcon/>Contact support</a></footer>
    {canClaim?<a className="checkout-claim-fab" href={`/claim?payment_id=${encodeURIComponent(payment.id)}`} aria-label="Create a recovery claim"><LifebuoyIcon/><span>Create a claim</span><i/></a>:null}

    {/* Agent support chat widget */}
    {chatOpen?<div className="agent-chat-backdrop" onClick={()=>setChatOpen(false)}/>:null}
    {chatOpen?<div className="agent-chat-panel">
      <div className="agent-chat-header">
        <h3>FlowPay Support Agent</h3>
        <small>Powered by AI</small>
      </div>
      <div className="agent-chat-messages">
        {chatMessages.map((msg,i)=><div key={i} className={`agent-chat-msg ${msg.role}`}>{msg.content}</div>)}
        {chatLoading?<div className="agent-chat-msg agent" style={{opacity:.6}}>Thinking...</div>:null}
      </div>
      <div className="agent-chat-input">
        <input type="text" value={chatInput} onChange={e=>setChatInput(e.target.value)} onKeyDown={e=>{if(e.key==="Enter")void sendChat()}} placeholder="Type your message..." disabled={chatLoading}/>
        <button type="button" onClick={()=>void sendChat()} disabled={!chatInput.trim()||chatLoading}>Send</button>
      </div>
    </div>:null}
    <button type="button" className={`agent-chat-fab${chatOpen?" agent-chat-fab-close":""}`} onClick={()=>chatOpen?setChatOpen(false):openChat()} aria-label="Support agent">
      {chatOpen?<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>:<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z"/></svg>}
    </button>
  </main>;
}
