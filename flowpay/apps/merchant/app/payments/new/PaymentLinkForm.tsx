"use client";
import {FormEvent,useState} from "react";
import {ArrowUpRightIcon,CheckIcon,CopyIcon,PlusIcon} from "../../components/Icons";
import {SelectField} from "./SelectField";

const assets=[{value:"USDC",label:"USDC",detail:"USD Coin",icon:"/assets/usdc.svg"},{value:"ETH",label:"ETH",detail:"Native Ether",icon:"/assets/ethereum.svg"}];
const networks=[{value:"base_sepolia",label:"Base",detail:"Base Sepolia",icon:"/assets/base.svg"},{value:"ethereum_sepolia",label:"Ethereum",detail:"Ethereum Sepolia",icon:"/assets/ethereum.svg"},{value:"arbitrum_sepolia",label:"Arbitrum",detail:"Arbitrum Sepolia",icon:"/assets/arbitrum.svg"},{value:"bsc_testnet",label:"BNB Chain",detail:"BSC Testnet",icon:"/assets/bsc.svg"}];

export function PaymentLinkForm(){
  const [name,setName]=useState("");
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
  return <div className="simple-link-page reference-link-page">
    <div className="simple-link-heading"><h1>Create payment link</h1><p>Create a payment request in a few clicks.</p></div>
    <form action="/api/payments" method="post" className="simple-link-card" onSubmit={submit}>
      <label className="simple-field"><span>Customer or Business</span><input name="customer" placeholder="Name or email (optional)"/></label>
      <SelectField name="asset" label="Asset" options={assets}/>
      <label className="simple-field amount-field"><span>Amount</span><div><b>$</b><input name="amount" placeholder="0.00" inputMode="decimal" required/></div></label>
      <SelectField name="chain" label="Network" options={networks}/>
      <label className="simple-field"><span>Description</span><input name="reference" placeholder="What is this payment for?" value={name} onChange={event=>setName(event.target.value)} maxLength={160}/></label>
      <label className="simple-field"><span>Expiry</span><select name="expiry" defaultValue="7"><option value="1">1 day</option><option value="7">7 days</option><option value="30">30 days</option></select></label>
      {error?<div className="create-link-error">{error}</div>:null}<button className="simple-create-button" type="submit" disabled={submitting}>{submitting?"Generating link…":"Generate payment link"}</button>
    </form>
    {created?<div className="payment-success-backdrop" role="presentation" onMouseDown={event=>{if(event.target===event.currentTarget)setCreated(null)}}><section className="payment-success-modal" role="dialog" aria-modal="true" aria-labelledby="payment-created-title">
      <button className="success-close" type="button" onClick={()=>setCreated(null)} aria-label="Close">×</button><div className="success-confetti"><i/><i/><i/><i/><span><CheckIcon/></span></div>
      <h2 id="payment-created-title">Payment link created!</h2><p>Your payment link is ready to share with your customers.</p><div className="success-rule"/>
      <label>Payment link</label><div className="success-link"><code>{created.checkout_url}</code><button type="button" aria-label="Copy link" onClick={()=>navigator.clipboard?.writeText(created.checkout_url)}><CopyIcon/></button></div>
      <div className="success-actions"><a href={created.checkout_url} target="_blank" rel="noreferrer"><ArrowUpRightIcon/>Open link</a><button type="button" onClick={()=>navigator.clipboard?.writeText(created.checkout_url)}><CopyIcon/>Copy link</button></div>
    </section></div>:null}
  </div>;
}
