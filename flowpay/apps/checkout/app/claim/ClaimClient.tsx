"use client";

import Image from "next/image";
import {useEffect,useMemo,useRef,useState} from "react";
import {useRouter,useSearchParams} from "next/navigation";
import {Brand,LanguageButton} from "../components/Brand";
import {
  AmountIcon,ArrowLeftIcon,ArrowRightIcon,CalendarIcon,CheckIcon,CoinsIcon,
  EditIcon,ExternalIcon,FileIcon,InfoIcon,LockIcon,NetworkIcon,QuestionIcon,
  ReceiptIcon,ShieldIcon,SparklesIcon,UploadIcon,WalletIcon
} from "../components/Icons";

declare global {
  interface Window {
    ethereum?: {
      request:(args:{method:string;params?:unknown[]})=>Promise<unknown>;
      isMetaMask?:boolean;
      isCoinbaseWallet?:boolean;
      isTrust?:boolean;
    };
  }
}

type Payment={
  id:string;address:string;amount:string;asset:string;chain:string;status:string;
  expires_at:string;reference?:string|null;merchant_name?:string|null;
};
type Issue="wrong_asset"|"wrong_network"|"not_detected"|"wrong_amount"|"other";
type ClaimResult={id:string;status:string;wallet_challenge?:{message?:string;wallet?:string}|null};

type IssueItem={id:Issue;title:string;copy:string;icon:"asset"|"network"|"receipt"|"amount"|"other"};
const issues:IssueItem[]=[
  {id:"wrong_asset",title:"Wrong asset",copy:"I sent a different token",icon:"asset"},
  {id:"wrong_network",title:"Wrong network",copy:"I used the wrong blockchain",icon:"network"},
  {id:"not_detected",title:"Payment not detected",copy:"My transfer was not found",icon:"receipt"},
  {id:"wrong_amount",title:"Wrong amount",copy:"I sent the wrong amount",icon:"amount"},
  {id:"other",title:"Other issue",copy:"Something else happened",icon:"other"},
];

const networkLabel:Record<string,string>={
  base:"Base",bsc:"BNB Smart Chain",bsc_testnet:"BNB Smart Chain Testnet",
  ethereum_sepolia:"Ethereum Sepolia",base_sepolia:"Base Sepolia",
  arbitrum_sepolia:"Arbitrum Sepolia",optimism_sepolia:"Optimism Sepolia",polygon_amoy:"Polygon Amoy"
};
const assetIcon=(asset:string)=>asset.toUpperCase()==="USDT"?"/assets/usdt.svg":"/assets/usdc.svg";
const networkIcon=(chain:string)=>chain.startsWith("bsc")?"/assets/bsc.svg":"/assets/base.svg";
const isEvmAddress=(value:string)=>/^0x[a-fA-F0-9]{40}$/.test(value);
const isTxHash=(value:string)=>/^0x[a-fA-F0-9]{64}$/.test(value);
const isAmount=(value:string)=>/^(?:0|[1-9]\d*)(?:\.\d+)?$/.test(value)&&!/^0(?:\.0+)?$/.test(value);
const short=(value:string)=>value.length>18?`${value.slice(0,7)}…${value.slice(-5)}`:value;
const fileToBase64=(file:File)=>new Promise<string>((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(String(reader.result).split(",")[1]??"");reader.onerror=()=>reject(reader.error);reader.readAsDataURL(file);});

function IssueIcon({kind}:{kind:IssueItem["icon"]}){
  if(kind==="asset")return <CoinsIcon/>;
  if(kind==="network")return <NetworkIcon/>;
  if(kind==="receipt")return <ReceiptIcon/>;
  if(kind==="amount")return <AmountIcon/>;
  return <QuestionIcon/>;
}

export function ClaimClient(){
  const search=useSearchParams();
  const router=useRouter();
  const paymentId=search.get("payment_id")??"";
  const [payment,setPayment]=useState<Payment|null>(null);
  const [loading,setLoading]=useState(Boolean(paymentId));
  const [step,setStep]=useState(1);
  const [issue,setIssue]=useState<Issue>("wrong_network");
  const [tx,setTx]=useState("");
  const [chain,setChain]=useState("bsc");
  const [asset,setAsset]=useState("USDT");
  const [amount,setAmount]=useState("");
  const [sentAt,setSentAt]=useState("");
  const [explanation,setExplanation]=useState("");
  const [destination,setDestination]=useState("");
  const [wallet,setWallet]=useState("");
  const [walletName,setWalletName]=useState("Browser wallet");
  const [files,setFiles]=useState<File[]>([]);
  const [error,setError]=useState("");
  const [submitting,setSubmitting]=useState(false);
  const [submittedClaim,setSubmittedClaim]=useState<string|null>(null);
  const inputRef=useRef<HTMLInputElement>(null);

  useEffect(()=>{
    if(!paymentId){setLoading(false);return;}
    let cancelled=false;
    (async()=>{
      try{
        const response=await fetch(`/api/payment/${encodeURIComponent(paymentId)}`,{cache:"no-store"});
        const body=await response.json();
        if(!response.ok)throw new Error(body?.error||"Unable to load payment");
        if(cancelled)return;
        setPayment(body);
        setAmount(body.amount||"");
        setChain(String(body.chain));
        setAsset(String(body.asset).toUpperCase());
      }catch(err){if(!cancelled)setError(err instanceof Error?err.message:"Unable to load payment");}
      finally{if(!cancelled)setLoading(false);}
    })();
    return()=>{cancelled=true;};
  },[paymentId]);

  const reason=useMemo(()=>issues.find(item=>item.id===issue)??issues[0],[issue]);
  const dateDisplay=sentAt?new Date(sentAt).toLocaleString():"Not provided";
  const expectedNetwork=payment?networkLabel[payment.chain]??payment.chain:"—";
  const expectedPayment=payment?`${payment.amount} ${payment.asset} on ${expectedNetwork}`:"Payment details unavailable";

  const canContinue=useMemo(()=>{
    if(step===1)return Boolean(issue)&&isEvmAddress(destination);
    if(step===2)return isTxHash(tx)&&Boolean(chain)&&Boolean(asset)&&isAmount(amount);
    if(step===3)return isEvmAddress(wallet);
    return true;
  },[step,issue,destination,tx,chain,asset,amount,wallet]);

  const providerLabel=()=>{
    if(window.ethereum?.isMetaMask)return "MetaMask";
    if(window.ethereum?.isCoinbaseWallet)return "Coinbase Wallet";
    if(window.ethereum?.isTrust)return "Trust Wallet";
    return "Browser wallet";
  };

  const connect=async()=>{
    setError("");
    try{
      if(!window.ethereum)throw new Error("No EVM wallet was detected in this browser.");
      const accounts=await window.ethereum.request({method:"eth_requestAccounts"}) as string[];
      const address=accounts?.[0];
      if(!address||!isEvmAddress(address))throw new Error("The wallet did not return a valid EVM address.");
      setWallet(address);
      setDestination(current=>current||address);
      setWalletName(providerLabel());
    }catch(err){setError(err instanceof Error?err.message:"Unable to connect wallet");}
  };

  const addFiles=(incoming:FileList|null)=>{
    if(!incoming)return;
    setError("");
    const accepted:Array<File>=[];
    for(const file of Array.from(incoming)){
      if(file.size>5*1024*1024){setError(`${file.name} is larger than the 5 MB evidence limit.`);continue;}
      if(!["image/png","image/jpeg","application/pdf"].includes(file.type)){setError(`${file.name} is not a supported evidence type.`);continue;}
      accepted.push(file);
    }
    setFiles(current=>[...current,...accepted].slice(0,6));
  };

  const next=()=>{
    if(!canContinue)return;
    setError("");
    setStep(current=>Math.min(4,current+1));
    window.scrollTo({top:0,behavior:"smooth"});
  };

  const submit=async()=>{
    if(!paymentId){setError("This claim is not attached to a payment checkout.");return;}
    if(!payment){setError("Payment details are unavailable.");return;}
    if(!isEvmAddress(wallet)||!isEvmAddress(destination)){setError("Connect the sending wallet and provide a valid recovery destination.");return;}
    setSubmitting(true);setError("");
    try{
      const createdResponse=await fetch("/api/claims",{
        method:"POST",
        headers:{"content-type":"application/json"},
        body:JSON.stringify({
          payment_id:paymentId,
          transaction_hash:tx,
          actual_chain:chain,
          actual_asset:asset,
          originating_wallet:wallet,
          recovery_destination:destination,
          explanation:explanation.trim()||`${reason.title}: ${reason.copy}. Customer reported ${amount} ${asset} on ${networkLabel[chain]??chain}.`,
        }),
      });
      const created=await createdResponse.json() as ClaimResult&{error?:string};
      if(!createdResponse.ok)throw new Error(created.error||"Unable to create claim");

      for(const file of files){
        const evidenceResponse=await fetch(`/api/claims/${encodeURIComponent(created.id)}/evidence`,{
          method:"POST",
          headers:{"content-type":"application/json"},
          body:JSON.stringify({
            evidence_type:file.type.startsWith("image/")?"SCREENSHOT":"DOCUMENT",
            filename:file.name,
            content_base64:await fileToBase64(file),
          }),
        });
        const evidenceBody=await evidenceResponse.json();
        if(!evidenceResponse.ok)throw new Error(evidenceBody?.error||`Unable to upload ${file.name}`);
      }

      const challenge=created.wallet_challenge?.message;
      if(!challenge)throw new Error("FlowPay did not return the required wallet authorization challenge.");
      if(!window.ethereum)throw new Error("The connected wallet provider is no longer available.");
      const signature=await window.ethereum.request({method:"personal_sign",params:[challenge,wallet]}) as string;
      const authorizeResponse=await fetch(`/api/claims/${encodeURIComponent(created.id)}/authorize`,{
        method:"POST",
        headers:{"content-type":"application/json"},
        body:JSON.stringify({signature}),
      });
      const authorizeBody=await authorizeResponse.json();
      if(!authorizeResponse.ok)throw new Error(authorizeBody?.error||"Wallet authorization failed");
      setSubmittedClaim(created.id);
      window.scrollTo({top:0,behavior:"smooth"});
    }catch(err){setError(err instanceof Error?err.message:"Unable to submit claim");}
    finally{setSubmitting(false);}
  };

  if(loading)return <div className="claim-loading"><div className="loader-orb"><Image src="/assets/flowpay-mark.svg" width={40} height={40} alt=""/></div><span>Loading your checkout…</span></div>;

  if(submittedClaim)return <main className="claim-shell">
    <header className="claim-header"><Brand/><LanguageButton/></header>
    <section className="claim-success">
      <div className="success-orb"><CheckIcon/></div>
      <span className="violet-label">Claim received</span>
      <h1>Your recovery investigation has started</h1>
      <p>FlowPay will verify the transaction, ownership, counterfactual address relationship, funds, recovery policy and simulation before any recovery can be proposed.</p>
      <div className="claim-id-box"><span>Claim ID</span><code>{submittedClaim}</code><button type="button" onClick={()=>navigator.clipboard.writeText(submittedClaim)}>Copy</button></div>
      <div className="safety-note"><ShieldIcon/><div><strong>Recovery is never automatic just because a claim was submitted.</strong><span>Unsupported or ambiguous cases are escalated. Consequential recovery execution still requires deterministic policy checks, simulation and approval.</span></div></div>
      {paymentId?<a className="primary-button inline" href={`/pay/${encodeURIComponent(paymentId)}`}>Back to payment <ArrowRightIcon/></a>:null}
    </section>
  </main>;

  return <main className="claim-shell">
    <header className="claim-header"><Brand/><LanguageButton/></header>
    <div className="claim-stage">
      <button className="back-payment" type="button" onClick={()=>paymentId?router.push(`/pay/${encodeURIComponent(paymentId)}`):router.back()}><ArrowLeftIcon/> Back to payment</button>

      <div className="claim-layout">
        <aside className="claim-sidebar">
          <Image src="/assets/recovery-bot.svg" width={178} height={178} alt="FlowPay recovery agent" priority/>
          <h2>{step===1?"We're here to help.":"Create a claim"}</h2>
          <p>{step===1?"Our AI agent will investigate your payment and help recover your funds if possible.":"Our AI agent is helping you recover your funds."}</p>
          {payment?<div className="expected-payment-mini"><span>Expected payment</span><strong>{payment.amount} {payment.asset}</strong><small>{expectedNetwork}</small></div>:null}
          <div className="sidebar-divider"/>
          <ol>
            {[1,2,3,4].map(number=><li key={number} className={`${step===number?"active":""} ${step>number?"done":""}`}>
              <span>{step>number?<CheckIcon/>:number}</span>
              <div><strong>{["Claim details","Transaction info","Verify ownership","Review & submit"][number-1]}</strong><small>{["Tell us what happened","Provide transaction details","Verify you control the wallet","Agent investigation begins"][number-1]}</small></div>
            </li>)}
          </ol>
          <div className="data-secure"><ShieldIcon/><div><strong>Your data is secure</strong><span>Evidence is investigation input only. FlowPay independently verifies critical blockchain facts.</span></div></div>
        </aside>

        <section className="claim-panel">
          <span className="violet-label">Create a claim</span>

          {step===1?<>
            <h1>Let&apos;s recover your funds</h1>
            <p className="lead">Fill in the details below and our AI agent will take it from there.</p>
            <div className="panel-divider"/>
            <h3>1. What happened?</h3>
            <div className="issue-grid">
              {issues.map(item=><button type="button" key={item.id} className={issue===item.id?"selected":""} onClick={()=>setIssue(item.id)}>
                <span className="issue-glyph"><IssueIcon kind={item.icon}/></span><strong>{item.title}</strong><small>{item.copy}</small>
              </button>)}
            </div>

            <h3>2. Additional information</h3>
            <label className="full-label"><span>Tell us what happened <small>(optional)</small></span><textarea value={explanation} onChange={event=>setExplanation(event.target.value)} maxLength={500} placeholder="Please describe the issue in detail…"/><em>{explanation.length}/500</em></label>
            <div className="upload-box" onClick={()=>inputRef.current?.click()} onDragOver={event=>event.preventDefault()} onDrop={event=>{event.preventDefault();addFiles(event.dataTransfer.files);}}>
              <UploadIcon/><strong>Drag and drop files here or <u>click to upload</u></strong><span>Screenshots, transaction receipts, chat logs, etc. · JPG, PNG, PDF · max 5 MB each.</span>
              <input ref={inputRef} type="file" accept="image/png,image/jpeg,application/pdf" multiple hidden onChange={event=>addFiles(event.target.files)}/>
            </div>
            {files.length?<div className="file-list">{files.map(file=><span key={`${file.name}-${file.size}`}><FileIcon/>{file.name}<button type="button" aria-label={`Remove ${file.name}`} onClick={()=>setFiles(current=>current.filter(item=>item!==file))}>×</button></span>)}</div>:null}

            <h3>3. Recovery destination</h3>
            <label className="destination-label"><span>Wallet address to receive the recovered funds</span><div><input value={destination} onChange={event=>setDestination(event.target.value.trim())} placeholder="0x…"/><button type="button" onClick={()=>void connect()}><WalletIcon/> Connect wallet</button></div></label>
            <div className="agent-callout"><SparklesIcon/><div><strong>Recovery agent will investigate</strong><span>FlowPay verifies the transaction, deterministic checkout address, token balance, claimant authorization, recovery policy and simulation result before proposing any financial action.</span></div></div>
          </>:null}

          {step===2?<>
            <h1>Transaction information</h1>
            <p className="lead">Provide the transaction details so FlowPay can locate and independently verify it.</p>
            <div className="info-banner"><InfoIcon/><span>You can find this information in your wallet or block explorer.</span></div>
            <div className="expected-strip"><span>Checkout expected</span><strong>{expectedPayment}</strong></div>
            <div className="form-stack">
              <label><span>Transaction hash (TXID)</span><small>Enter the full transaction hash of the payment.</small><input value={tx} onChange={event=>setTx(event.target.value.trim())} placeholder="0x…"/><a href="#" onClick={event=>event.preventDefault()}>How do I find this? <ExternalIcon/></a></label>
              <label><span>Network used</span><small>Select the blockchain network you actually used.</small><select value={chain} onChange={event=>setChain(event.target.value)}>
                <option value="bsc">BNB Smart Chain</option>
                <option value="bsc_testnet">BNB Smart Chain Testnet</option>
                <option value="base">Base</option>
                <option value="base_sepolia">Base Sepolia</option>
                <option value="ethereum_sepolia">Ethereum Sepolia</option>
                <option value="arbitrum_sepolia">Arbitrum Sepolia</option>
                <option value="optimism_sepolia">Optimism Sepolia</option>
                <option value="polygon_amoy">Polygon Amoy</option>
              </select></label>
              <div className="form-grid two">
                <label><span>Asset sent</span><small>Select the token you actually sent.</small><select value={asset} onChange={event=>setAsset(event.target.value)}><option>USDT</option><option>USDC</option></select></label>
                <label><span>Amount sent</span><small>Enter the exact amount you sent.</small><div className="input-suffix"><input value={amount} onChange={event=>setAmount(event.target.value)} inputMode="decimal" placeholder="0.00"/><b>{asset}</b></div></label>
              </div>
              <label><span>Date & time <small>(optional)</small></span><small>When did you send this transaction?</small><div className="date-input"><CalendarIcon/><input type="datetime-local" value={sentAt} onChange={event=>setSentAt(event.target.value)}/></div></label>
            </div>
            <div className="agent-callout"><SparklesIcon/><div><strong>AI investigation tip</strong><span>The network and asset you select are claim inputs, not trusted facts. FlowPay independently checks the transaction on-chain.</span></div></div>
          </>:null}

          {step===3?<>
            <h1>Verify ownership</h1>
            <p className="lead">Connect the self-custody wallet associated with this transaction. FlowPay never receives your private key.</p>
            <div className="info-banner"><InfoIcon/><span>This protects customers and merchants against fraudulent recovery claims.</span></div>

            <h3>1. Connect your wallet</h3>
            <p className="section-copy">Use the wallet that sent the transaction whenever possible.</p>
            <div className="wallet-grid wallet-grid-four">
              <button type="button" onClick={()=>void connect()}><span className="wallet-logo"><Image src="/assets/metamask.svg" width={54} height={54} alt=""/></span><strong>MetaMask</strong></button>
              <button type="button" className="wallet-option-muted" disabled title="WalletConnect SDK is not enabled in this hackathon build"><span className="wallet-logo"><Image src="/assets/walletconnect.svg" width={54} height={54} alt=""/></span><strong>WalletConnect</strong><small>SDK not enabled</small></button>
              <button type="button" onClick={()=>void connect()}><span className="wallet-logo"><Image src="/assets/coinbase-wallet.svg" width={54} height={54} alt=""/></span><strong>Coinbase Wallet</strong></button>
              <button type="button" onClick={()=>void connect()}><span className="wallet-logo"><Image src="/assets/trust-wallet.svg" width={54} height={54} alt=""/></span><strong>Trust Wallet</strong></button>
            </div>
            <div className="private-key-note"><LockIcon/>We never store your private keys, seed phrases, or grant the recovery agent wallet access.</div>

            <h3>2. Sign a message</h3>
            <div className="why-sign"><EditIcon/><div><strong>Why do I need to sign?</strong><span>On submission, FlowPay creates a one-time claim challenge. Your wallet signs that exact message to prove control without moving funds or paying gas.</span></div></div>

            <h3>3. Your wallet address</h3>
            <div className={`connected-wallet ${wallet?"connected":""}`}><WalletIcon/><div><strong>{wallet?short(wallet):"No wallet connected"}</strong>{wallet?<small>{walletName}</small>:null}</div>{wallet?<span>Connected</span>:null}<button type="button" onClick={()=>wallet?setWallet(""):void connect()}>{wallet?"Disconnect":"Connect"}</button></div>
          </>:null}

          {step===4?<>
            <h1>Review & submit</h1>
            <p className="lead">Review your claim before FlowPay starts the recovery investigation.</p>
            <div className="info-banner"><InfoIcon/><span>Incorrect information can delay recovery. Critical blockchain facts are still independently verified.</span></div>
            <h3>Summary of your claim</h3>

            <div className="review-card"><div className="review-icon"><InfoIcon/></div><div><strong>Claim reason</strong><span>{reason.title}</span><small>{reason.copy}</small></div><button type="button" onClick={()=>setStep(1)}><EditIcon/> Edit</button></div>
            <div className="review-card complex"><div className="review-icon"><ReceiptIcon/></div><div className="review-wide"><strong>Transaction details</strong><div className="review-grid"><span><small>Transaction hash</small><b>{short(tx)}</b></span><span><small>Network used</small><b><Image src={networkIcon(chain)} width={22} height={22} alt=""/>{networkLabel[chain]??chain}</b></span><span><small>Asset sent</small><b><Image src={assetIcon(asset)} width={22} height={22} alt=""/>{asset}</b></span><span><small>Amount sent</small><b>{amount} {asset}</b></span><span><small>Date & time</small><b>{dateDisplay}</b></span></div></div><button type="button" onClick={()=>setStep(2)}><EditIcon/> Edit</button></div>
            <div className="review-card"><div className="review-icon"><ShieldIcon/></div><div><strong>Wallet verification</strong><span>{short(wallet)}</span><small>{walletName} · one-time signature requested after you press submit</small></div><button type="button" onClick={()=>setStep(3)}><EditIcon/> Edit</button></div>
            <div className="review-card"><div className="review-icon"><FileIcon/></div><div><strong>Additional information</strong><span>{explanation||"No additional description"}</span><small>{files.length} supporting file{files.length===1?"":"s"} attached · recovery destination {short(destination)}</small></div><button type="button" onClick={()=>setStep(1)}><EditIcon/> Edit</button></div>

            <div className="agent-callout"><SparklesIcon/><div><strong>What happens next?</strong><span>The agent investigates with typed tools. If recovery is possible, it creates a constrained plan; deterministic policy, simulation and human approval remain mandatory before execution.</span></div></div>
          </>:null}

          {error?<div className="form-error"><InfoIcon/><span>{error}</span></div>:null}

          <div className="claim-actions">
            {step>1?<button className="secondary-button" type="button" onClick={()=>setStep(current=>current-1)}><ArrowLeftIcon/> Back</button>:<span/>}
            {step<4?<button className="primary-button" type="button" disabled={!canContinue} onClick={next}>Continue <ArrowRightIcon/></button>:<button className="primary-button" type="button" disabled={submitting} onClick={()=>void submit()}>{submitting?"Waiting for wallet…":"Submit claim"} <ArrowRightIcon/></button>}
          </div>
          {step===4?<div className="secured-line"><LockIcon/> Secured by FlowPay</div>:null}
        </section>
      </div>
    </div>
  </main>;
}
