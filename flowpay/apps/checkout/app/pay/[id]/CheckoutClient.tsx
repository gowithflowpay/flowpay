"use client";

import Image from "next/image";
import {useCallback,useEffect,useMemo,useRef,useState} from "react";
import {Brand,LanguageButton} from "../../components/Brand";
import {
  ArrowLeftIcon,CheckIcon,ClockIcon,CopyIcon,HeadphonesIcon,
  InfoIcon,ShieldIcon,StoreIcon
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
  payment_method?:string|null;
  bank_name?:string|null;
  account_name?:string|null;
  account_expires_at?:string|null;
  merchant_amount?:string|null;
  platform_fee?:string|null;
  estimated_provider_fee?:string|null;
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
  "custom:flutterwave_ngn":{label:"Bank transfer",asset:"/assets/flowpay-mark.svg"},
};
const terminal=new Set(["COMPLETED","RECOVERED","EXPIRED","FAILED","CANCELLED","ESCALATED"]);
const outcomeStates=new Set(["CONFIRMED","SETTLING","COMPLETED","RECOVERED"]);
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
type ChatMessage={role:string;content:string};
type RecoveryDetails={source_chain?:string;destination?:string;recovery_tx?:string;execution_status?:string;asset?:string;amount_atomic?:string};

export function CheckoutClient({paymentId,home=false,suppressOutcome=false,initialPayment=null,initialDeposits=[]}:CheckoutClientProps){
  const [payment,setPayment]=useState<Payment|null>(initialPayment);
  const [deposits,setDeposits]=useState<Deposit[]>(initialDeposits);
  const [error,setError]=useState("");
  const [copied,setCopied]=useState(false);
  const [remaining,setRemaining]=useState(0);
  const [windowTotal,setWindowTotal]=useState(0);
  const [showSuccess,setShowSuccess]=useState(false);
  const [showWrongAsset,setShowWrongAsset]=useState(false);
  const [chatOpen,setChatOpen]=useState(false);
  const [chatMessages,setChatMessages]=useState<ChatMessage[]>([]);
  const [chatInput,setChatInput]=useState("");
  const [chatLoading,setChatLoading]=useState(false);
  const [chatEmail,setChatEmail]=useState("");
  const [chatSessionId,setChatSessionId]=useState("");
  const [chatClaimId,setChatClaimId]=useState("");
  const [recoveryDetails,setRecoveryDetails]=useState<RecoveryDetails|null>(null);
  const successHandled=useRef(false);

  useEffect(()=>{
    const storageKey=`flowpay-agent-v3:${paymentId}`;
    try{
      const saved=JSON.parse(window.localStorage.getItem(storageKey)||"null");
      const sessionId=typeof saved?.sessionId==="string"?saved.sessionId:window.crypto.randomUUID();
      setChatSessionId(sessionId);
      setChatEmail(typeof saved?.email==="string"?saved.email:"");
      setChatClaimId(typeof saved?.claimId==="string"?saved.claimId:"");
      if(saved?.recovery&&typeof saved.recovery==="object")setRecoveryDetails(saved.recovery);
      if(Array.isArray(saved?.messages))setChatMessages(saved.messages);
      document.cookie=`flowpay_agent_session=${encodeURIComponent(sessionId)}; Path=/pay/${encodeURIComponent(paymentId)}; Max-Age=2592000; SameSite=Lax; Secure`;
    }catch{setChatSessionId(window.crypto.randomUUID())}
  },[paymentId]);

  useEffect(()=>{
    if(!chatSessionId)return;
    window.localStorage.setItem(`flowpay-agent-v3:${paymentId}`,JSON.stringify({sessionId:chatSessionId,email:chatEmail,claimId:chatClaimId,messages:chatMessages,recovery:recoveryDetails}));
  },[chatClaimId,chatEmail,chatMessages,chatSessionId,paymentId,recoveryDetails]);

  useEffect(()=>{
    if(!chatClaimId)return;
    const poll=async()=>{
      try{
        const response=await fetch(`/api/payment/${encodeURIComponent(paymentId)}/agent/status?claim_id=${encodeURIComponent(chatClaimId)}`,{cache:"no-store"});
        if(!response.ok)return;
        const result=await response.json();
        if(result.recovery&&typeof result.recovery==="object")setRecoveryDetails(result.recovery);
        if(!result.verdict)return;
        const marker=`Internal investigation verdict: ${result.verdict}.`;
        setChatMessages(current=>current.some(message=>message.content.startsWith("Internal investigation verdict:"))?current:[...current,{role:"system",content:marker}]);
      }catch{}
    };
    void poll();
    const timer=window.setInterval(()=>void poll(),4000);
    return()=>window.clearInterval(timer);
  },[chatClaimId,paymentId]);

  const load=useCallback(async()=>{
    try{
      const response=await fetch(`/api/payment/${encodeURIComponent(paymentId)}`,{cache:"no-store"});
      const raw=await response.text();
      let body:any={};
      try{body=raw?JSON.parse(raw):{}}catch{body={error:response.ok?"Checkout returned an invalid response":"Payment is unavailable in this environment"}}
      if(!response.ok)throw new Error(body?.error||"Unable to load payment");
      setPayment(body);
      setError("");
      const depositsResponse=await fetch(`/api/payment/${encodeURIComponent(paymentId)}/deposits`,{cache:"no-store"});
      if(depositsResponse.ok){
        const depositText=await depositsResponse.text();
        let depositBody:any={};
        try{depositBody=depositText?JSON.parse(depositText):{}}catch{}
        setDeposits(Array.isArray(depositBody.data)?depositBody.data:[]);
      }
    }catch(err){
      setError(err instanceof Error?err.message:"Unable to load payment");
    }
  },[paymentId]);

  const paymentStatus=payment?.status;
  useEffect(()=>{
    if(suppressOutcome||successHandled.current||!paymentStatus||!outcomeStates.has(paymentStatus))return;
    successHandled.current=true;
    setShowSuccess(true);
  },[paymentStatus,suppressOutcome]);

  useEffect(()=>{
    if(paymentStatus==="WRONG_ASSET")setShowWrongAsset(true);
  },[paymentStatus]);

  useEffect(()=>{
    if(!showSuccess)return;
    if(paymentStatus==="RECOVERED")return;
    const timer=window.setTimeout(()=>window.location.assign(`/pay/${encodeURIComponent(paymentId)}/success`),3500);
    return()=>window.clearTimeout(timer);
  },[paymentId,paymentStatus,showSuccess]);

  useEffect(()=>{
    void load();
    const timer=window.setInterval(()=>{
      if(!payment||!terminal.has(payment.status))void load();
    },1000);
    return()=>window.clearInterval(timer);
  },[load,payment?.status]);

  useEffect(()=>{
    if(!payment)return;
    const deadline=expiryTime(payment.account_expires_at||payment.expires_at);
    setWindowTotal(Math.max(0,Math.floor((deadline-Date.now())/1000)));
    const tick=()=>setRemaining(Math.max(0,Math.floor((deadline-Date.now())/1000)));
    tick();
    const timer=window.setInterval(tick,1000);
    return()=>window.clearInterval(timer);
  },[payment]);

  const time=useMemo(()=>`${String(Math.floor(remaining/60)).padStart(2,"0")}:${String(remaining%60).padStart(2,"0")}`,[remaining]);
  const countdownPercent=windowTotal>0?Math.min(100,Math.max(0,Math.round(((windowTotal-remaining)/windowTotal)*100))):0;

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
        body:JSON.stringify({payment_id:payment.id,session_id:chatSessionId,email:chatEmail,messages:updated}),
      });
      const body=await resp.json();
      if(typeof body.email==="string")setChatEmail(body.email);
      const agentMsg={role:"agent",content:body.reply||"I'm here to help."};
      setChatMessages([...updated,agentMsg]);
      if(body.status==="CLAIM_CREATED"){
        setChatClaimId(body.claim_id);
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
      setChatMessages([{role:"agent",content:"Hi, I'm FlowPay Agent. What would you like help with today?"}]);
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
  const isRefund=status==="RECOVERED";
  const recoveryNetwork=recoveryDetails?.source_chain?chainMeta[recoveryDetails.source_chain]??{label:recoveryDetails.source_chain.replaceAll("_"," "),asset:"/assets/ethereum.svg"}:null;
  const isBankTransfer=payment.payment_method==="bank_transfer";
  const statusCopy:[string,string]=isBankTransfer&&status==="WAITING"?["Waiting for your transfer","Confirmed automatically once the bank settles."]:[statusTitle,statusText];
  // Show claim link for any non-terminal state, including expired — users who sent funds need recovery.

  return <main className={`checkout-shell${home?" checkout-preview":""}${isBankTransfer?" bank-transfer":""}`}>
    <header className="payment-header reference-header"><Brand/><span/><div className="checkout-header-actions"><button className="checkout-store" type="button"><StoreIcon/>{isBankTransfer?"FlowPay Checkout":merchantName(payment)} <span>⌄</span></button></div></header>
    <section className="reference-checkout" aria-labelledby="payment-title">
      <div className="reference-summary"><div className="reference-summary-inner">
        <span className="checkout-kicker">{isBankTransfer?"FlowPay Checkout":"Checkout"}</span>{isBankTransfer?null:<span className="paying-label"><StoreIcon/>{merchantName(payment)}</span>}
        <h2 className="complete-title">{isBankTransfer?"Pay by bank transfer":"Complete payment"}</h2><h1 id="payment-title"><strong>{amountDisplay(payment.amount,payment.asset)}</strong><small>{payment.asset}</small></h1>
        {isBankTransfer?<p className="ngn-amount-note">Nigerian Naira · one-time transfer</p>:null}
        {isBankTransfer&&payment.merchant_amount?<ul className="ngn-breakdown"><li><span>Merchant amount</span><b>{amountDisplay(payment.merchant_amount,payment.asset)}</b></li>{Number(payment.platform_fee??0)>0?<li><span>Service fee</span><b>{amountDisplay(payment.platform_fee??"0",payment.asset)}</b></li>:null}{Number(payment.estimated_provider_fee??0)>0?<li><span>Payment processing</span><b>{amountDisplay(payment.estimated_provider_fee??"0",payment.asset)}</b></li>:null}<li className="ngn-breakdown-total"><span>Total to transfer</span><b>{amountDisplay(payment.amount,payment.asset)}</b></li></ul>:null}{stableAssets.has(payment.asset.toUpperCase())?<p>≈ {dollarDisplay(payment.amount,payment.asset)} USD</p>:null}
        {isBankTransfer?null:<details className="reference-network"><summary><Image src={network.asset} width={24} height={24} alt=""/><strong>{network.label}</strong><span className="network-chevron">⌄</span></summary></details>}
        <p className="send-instruction">{isRefund?"This payment link is inactive because the asset was refunded.":isBankTransfer?`Transfer exactly ${amountDisplay(payment.amount,payment.asset)} NGN to the account below`:`Send exactly ${amountDisplay(payment.amount,payment.asset)} ${payment.asset} to the address below`}</p>
        {isBankTransfer?<div className="ngn-account-card"><div className="ngn-account-head"><em>{payment.bank_name}</em></div><div className="ngn-account-number"><strong>{payment.address}</strong><button type="button" onClick={()=>void copy()} aria-label="Copy account number" disabled={isRefund}>{copied?<CheckIcon/>:<CopyIcon/>}</button></div>{copied?<span className="ngn-copied" role="status">Account number copied</span>:null}</div>:null}
        {isBankTransfer?null:<div className={`reference-address${isRefund?" reference-address-inactive":""}`} title={payment.address}><code>{shortAddress(payment.address)}</code><button type="button" onClick={()=>void copy()} aria-label="Copy payment address" disabled={isRefund}><CopyIcon/></button></div>}
        {copied&&!isBankTransfer?<span className="reference-copied" role="status">Address copied</span>:null}
        {isBankTransfer?null:<div className="reference-warning"><InfoIcon/><p><strong>{`Use ${network.label} network only`}</strong><span>Other assets or networks may be lost.</span></p></div>}
        {isBankTransfer?<div className="ngn-countdown" role="timer"><div className="ngn-ring" style={{background:`conic-gradient(#6553e9 ${countdownPercent}%,#e9e6f4 0)`}}><span>{time}</span></div><div className="ngn-countdown-copy"><strong>{remaining>0?"Complete this transfer before time runs out":"This payment window has closed"}</strong><span>Keep the exact amount — confirmation is automatic.</span></div></div>:null}
        <span className="payment-status-label">{isBankTransfer?"Transfer status":"Payment status"}</span>
        <div className={`reference-status status-${status.toLowerCase()}`} role="status" aria-live="polite"><span>{isDone?<CheckIcon/>:<ClockIcon/>}</span><p><strong>{statusCopy[0]}</strong><small>{statusCopy[1]}</small></p>{isBankTransfer?null:<b>{time} left</b>}</div>
        <div className="secure-copy"><ShieldIcon/><p><strong>Your payment is secure and encrypted.</strong><span>We never store your funds.</span></p></div>
        <div className="summary-rule"/>
        <div className="expiry-copy"><ClockIcon/><p><span>Payment expires in</span><strong>{time}</strong></p></div>
      </div></div>
      <div className="reference-payment"><div className="reference-payment-inner">
        <h2 className={isBankTransfer?"ngn-steps-title":undefined}>{isBankTransfer?"Transfer in 3 steps":<>Send <strong>{amountDisplay(payment.amount,payment.asset)} {payment.asset}</strong> to the address below</>}</h2>
        {isBankTransfer?<ol className="ngn-steps"><li><span>1</span>Open your bank app or dial your bank USSD code.</li><li><span>2</span>Transfer <strong>{amountDisplay(payment.amount,payment.asset)} NGN</strong> to the account shown.</li><li><span>3</span>Keep the exact amount — FlowPay confirms it automatically.</li></ol>:<div className="reference-qr"><div className="reference-qr-frame"><QrCode value={payment.address}/><div className="reference-qr-brand"><Image src="/assets/flowpay-mark.svg" width={40} height={40} alt="FlowPay"/></div></div></div>}
        {isBankTransfer?<p className="ngn-note">Use this temporary account for this payment only.</p>:<p className="qr-caption">Scan to pay with <strong>{network.label}</strong></p>}
      </div></div>

    </section>

    {showSuccess?<div className="payment-success-backdrop" role="presentation">
      <section className={`payment-success-modal${isRefund?" payment-refund-modal":""}`} role="dialog" aria-modal="true" aria-labelledby="payment-success-title" aria-describedby="payment-success-copy">
        <div className="payment-success-check"><CheckIcon/><i/><i/><i/><i/></div>
        <h2 id="payment-success-title">{isRefund?"Asset refunded":"Payment received!"}</h2>
        <p id="payment-success-copy">{isRefund?(recoveryDetails?.destination?`${payment.amount} ${payment.asset} was refunded on ${recoveryNetwork?.label??"the source network"} to ${recoveryDetails.destination}.`:"The recovery transaction is confirmed. Loading the refund network and destination…"):`${payment.amount} ${payment.asset} was received and confirmed on-chain.`}</p>
        <div className="payment-success-summary">
          <span>Amount <strong>{payment.amount} {payment.asset}</strong></span>
          <span>Network <strong>{isRefund?(recoveryNetwork?.label??"Loading…"):network.label}</strong></span>
          {isRefund&&recoveryDetails?.destination?<span>Refund destination <strong title={recoveryDetails.destination}>{shortAddress(recoveryDetails.destination)}</strong></span>:null}
          {isRefund&&recoveryDetails?.recovery_tx?<span>Recovery transaction <strong title={recoveryDetails.recovery_tx}>{shortAddress(recoveryDetails.recovery_tx)}</strong></span>:null}
          <span>Payment ID <strong>{payment.id}</strong></span>
        </div>
        <button type="button" onClick={()=>isRefund?setShowSuccess(false):window.location.assign(`/pay/${encodeURIComponent(paymentId)}/success`)}>{isRefund?"Close":"View confirmation"}</button>
        <small>{isRefund?"This payment link is now inactive.":"Redirecting securely…"}</small>
      </section>
    </div>:null}

    <footer className="reference-footer"><a href="/"><ArrowLeftIcon/>Cancel payment</a><a href="mailto:support@flowpay.dev"><HeadphonesIcon/>Contact support</a></footer>
    {/* Agent support chat widget */}
    {chatOpen?<div className="agent-chat-backdrop" onClick={()=>setChatOpen(false)}/>:null}
    {chatOpen?<div className="agent-chat-panel">
      <div className="agent-chat-header">
        <img src="/assets/recovery-bot.svg" alt="" />
        <div><h3>FlowPay Agent</h3><small><i />Online</small></div>
        <button type="button" onClick={()=>setChatOpen(false)} aria-label="Close chat">×</button>
      </div>
      <div className="agent-chat-messages">
        {chatMessages.map((msg,i)=><div key={i} className={`agent-chat-msg ${msg.role}`}>{msg.content}</div>)}
        {chatLoading?<div className="agent-chat-msg agent" style={{opacity:.6}}>Thinking...</div>:null}
      </div>
      <div className="agent-chat-input">
        <input type="text" value={chatInput} onChange={e=>setChatInput(e.target.value)} onKeyDown={e=>{if(e.key==="Enter")void sendChat()}} placeholder="Type your message..." disabled={chatLoading}/>
        <button type="button" onClick={()=>void sendChat()} disabled={!chatInput.trim()||!chatSessionId||chatLoading}>Send</button>
      </div>
    </div>:null}

    {showWrongAsset?<div className="payment-success-backdrop wrong-asset-backdrop" role="presentation">
      <section className="wrong-asset-modal" role="alertdialog" aria-modal="true" aria-labelledby="wrong-asset-title" aria-describedby="wrong-asset-copy">
        <div className="wrong-asset-icon"><InfoIcon/></div>
        <h2 id="wrong-asset-title">Wrong asset detected</h2>
        <p id="wrong-asset-copy">FlowPay detected a different asset from the one requested by this checkout. The payment cannot be completed, but the agent can investigate and recover eligible funds.</p>
        <div className="wrong-asset-actions">
          <button type="button" onClick={()=>setShowWrongAsset(false)}>Dismiss</button>
          <button type="button" onClick={()=>{setShowWrongAsset(false);openChat()}}>Start recovery</button>
        </div>
      </section>
    </div>:null}
    <button type="button" className={`agent-chat-fab${chatOpen?" agent-chat-fab-close":""}`} onClick={()=>chatOpen?setChatOpen(false):openChat()} aria-label="Support agent">
      {chatOpen?<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>:<img src="/assets/recovery-bot.svg" alt="" />}
    </button>
  </main>;
}
