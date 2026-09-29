"use client";
import {Dialog} from "@base-ui/react/dialog";
import {Field} from "@base-ui/react/field";
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
      const responseText=await response.text();
      let result:any={};
      try{result=responseText?JSON.parse(responseText):{}}catch{}
      if(!response.ok)throw new Error(result?.error?.message??"Unable to create payment link");
      setCreated({id:result.id,checkout_url:result.checkout_url});
    }catch(reason){setError(reason instanceof Error?reason.message:"Unable to create payment link")}finally{setSubmitting(false)}
  }
  return <div className="simple-link-page reference-link-page">
    <div className="simple-link-heading"><h1>Create payment</h1><p>Create a payment request in a few clicks.</p></div>
    <form action="/api/payments" method="post" className="simple-link-card" onSubmit={submit}>
      <Field.Root className="simple-field" name="customer"><Field.Label>Customer or Business</Field.Label><Field.Control placeholder="Name or email (optional)"/></Field.Root>
      <SelectField name="asset" label="Asset" options={assets}/>
      <Field.Root className="simple-field amount-field" name="amount"><Field.Label>Amount</Field.Label><div><Field.Control placeholder="0.00" inputMode="decimal" min="0.000000000000000001" step="any" required/></div><Field.Error match="valueMissing">Enter an amount.</Field.Error><Field.Error match="rangeUnderflow">Enter an amount greater than zero.</Field.Error></Field.Root>
      <SelectField name="chain" label="Network" options={networks}/>
      <Field.Root className="simple-field" name="reference"><Field.Label>Description</Field.Label><Field.Control placeholder="What is this payment for?" value={name} onChange={event=>setName(event.target.value)} maxLength={160}/></Field.Root>
      <Field.Root className="simple-field" name="expiry"><Field.Label>Expiry</Field.Label><Field.Control render={<select defaultValue="7"><option value="1">1 day</option><option value="7">7 days</option><option value="30">30 days</option></select>}/></Field.Root>
      {error?<div className="create-link-error">{error}</div>:null}<button className="simple-create-button" type="submit" disabled={submitting}>{submitting?"Generating link…":"Generate payment link"}</button>
    </form>
    <Dialog.Root open={!!created} onOpenChange={(open)=>{if(!open)setCreated(null)}}>
      <Dialog.Portal>
        <Dialog.Backdrop className="payment-success-backdrop"/>
        <Dialog.Popup className="payment-success-modal">
      {created?<><Dialog.Close className="success-close" aria-label="Close">×</Dialog.Close><div className="success-confetti"><i/><i/><i/><i/><span><CheckIcon/></span></div>
      <Dialog.Title id="payment-created-title">Payment link created!</Dialog.Title><Dialog.Description>Your payment link is ready to share with your customers.</Dialog.Description><div className="success-rule"/>
      <label>Payment link</label><div className="success-link"><code>{created.checkout_url}</code><button type="button" aria-label="Copy link" onClick={()=>navigator.clipboard?.writeText(created.checkout_url)}><CopyIcon/></button></div>
      <div className="success-actions"><a href={created.checkout_url} target="_blank" rel="noreferrer"><ArrowUpRightIcon/>Open link</a><button type="button" onClick={()=>navigator.clipboard?.writeText(created.checkout_url)}><CopyIcon/>Copy link</button></div></>:null}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  </div>;
}
