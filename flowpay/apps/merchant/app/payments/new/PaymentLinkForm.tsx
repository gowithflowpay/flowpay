"use client";
import {FormEvent,useState} from "react";
import {ArrowUpRightIcon,CheckIcon,CopyIcon,PlusIcon} from "../../components/Icons";
import {SelectField} from "./SelectField";

const assets=[{value:"USDC",label:"USDC",detail:"USD Coin",icon:"/assets/usdc.svg"},{value:"ETH",label:"ETH",detail:"Native Ether",icon:"/assets/ethereum.svg"}];
const networks=[{value:"base_sepolia",label:"Base",detail:"Base Sepolia",icon:"/assets/base.svg"},{value:"ethereum_sepolia",label:"Ethereum",detail:"Ethereum Sepolia",icon:"/assets/ethereum.svg"},{value:"arbitrum_sepolia",label:"Arbitrum",detail:"Arbitrum Sepolia",icon:"/assets/arbitrum.svg"},{value:"bsc_testnet",label:"BNB Chain",detail:"BSC Testnet",icon:"/assets/bsc.svg"}];

export function PaymentLinkForm(){
  const [name,setName]=useState("Web development payment");
  const [created,setCreated]=useState<{id:string;checkout_url:string}|null>(null);
  const [submitting,setSubmitting]=useState(false);
  const [error,setError]=useState("");
  async function submit(event:FormEvent<HTMLFormElement>){
    event.preventDefault();setSubmitting(true);setError("");
    try{
      const response=await fetch("/api/payments",{method:"POST",headers:{accept:"application/json"},body:new FormData(event.currentTarget)});
      const result=await response.json();
      if(!response.ok)throw new Error(result?.error?.message??"Unable to create payment link");
      setCreated({id:result.id,checkout_url:result.checkout_url});
    }catch(reason){setError(reason instanceof Error?reason.message:"Unable to create payment link")}finally{setSubmitting(false)}
  }
  return <div className="simple-link-page">
    <div className="simple-link-heading"><h1>Create payment link</h1><p>Create a simple checkout link and share it with your customer.</p></div>
    <form action="/api/payments" method="post" className="simple-link-card" onSubmit={submit}>
      <label className="simple-field"><span>Amount</span><input name="amount" defaultValue="100.00" inputMode="decimal" required/></label>
      <SelectField name="asset" label="Currency" options={assets}/>
      <SelectField name="chain" label="Network" options={networks}/>
      <label className="simple-field"><span>Link name <em>(optional)</em></span><input name="reference" value={name} onChange={event=>setName(event.target.value)} maxLength={160}/></label>
      <div className="generated-link-field"><span>Payment link</span><div><code>https://pay.flowpay.io/links/new</code><button type="button" aria-label="Copy payment link" onClick={()=>navigator.clipboard?.writeText("https://pay.flowpay.io/links/new")}><CopyIcon/></button></div><small><i>i</i>This is how your customer will pay</small></div>
      {error?<div className="create-link-error">{error}</div>:null}<button className="simple-create-button" type="submit" disabled={submitting}><PlusIcon/>{submitting?"Creating link…":"Create payment link"}</button>
    </form>
    {created?<div className="payment-success-backdrop" role="presentation" onMouseDown={event=>{if(event.target===event.currentTarget)setCreated(null)}}><section className="payment-success-modal" role="dialog" aria-modal="true" aria-labelledby="payment-created-title">
      <button className="success-close" type="button" onClick={()=>setCreated(null)} aria-label="Close">×</button><div className="success-confetti"><i/><i/><i/><i/><span><CheckIcon/></span></div>
      <h2 id="payment-created-title">Payment link created!</h2><p>Your payment link is ready to share with your customers.</p><div className="success-rule"/>
      <label>Payment link</label><div className="success-link"><code>{created.checkout_url}</code><button type="button" aria-label="Copy link" onClick={()=>navigator.clipboard?.writeText(created.checkout_url)}><CopyIcon/></button></div>
      <div className="success-actions"><a href={created.checkout_url} target="_blank" rel="noreferrer"><ArrowUpRightIcon/>Open link</a><button type="button" onClick={()=>navigator.clipboard?.writeText(created.checkout_url)}><CopyIcon/>Copy link</button></div>
    </section></div>:null}
  </div>;
}
