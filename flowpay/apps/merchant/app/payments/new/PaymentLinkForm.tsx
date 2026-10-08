"use client";
import {Dialog} from "@base-ui/react/dialog";
import {Field} from "@base-ui/react/field";
import {FormEvent,useState} from "react";
import {ArrowUpRightIcon,CheckIcon,CopyIcon,PlusIcon} from "../../components/Icons";
import {SelectField} from "./SelectField";

const assets=[{value:"USDC",label:"USDC",detail:"USD Coin",icon:"/assets/usdc.svg"},{value:"ETH",label:"ETH",detail:"Native Ether",icon:"/assets/ethereum.svg"}];
const networks=[{value:"base_sepolia",label:"Base",detail:"Base Sepolia",icon:"/assets/base.svg"},{value:"ethereum_sepolia",label:"Ethereum",detail:"Ethereum Sepolia",icon:"/assets/ethereum.svg"},{value:"arbitrum_sepolia",label:"Arbitrum",detail:"Arbitrum Sepolia",icon:"/assets/arbitrum.svg"},{value:"bsc_testnet",label:"BNB Chain",detail:"BSC Testnet",icon:"/assets/bsc.svg"}];
// Native ETH is only enabled on Base and Ethereum, so the picker must not offer
// a network where the selected asset is not an enabled payment asset.
const nativeEthNetworks=new Set(["base_sepolia","ethereum_sepolia"]);

export function PaymentLinkForm(){
  const [name,setName]=useState("");
  const [created,setCreated]=useState<{id:string;checkout_url:string}|null>(null);
  const [submitting,setSubmitting]=useState(false);
  const [error,setError]=useState("");
  const [amount,setAmount]=useState("");
  const [asset,setAsset]=useState("USDC");
  const allowedNetworks=asset.toUpperCase()==="ETH"?networks.filter(option=>nativeEthNetworks.has(option.value)):networks;
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
  return <div className="simple-link-page reference-link-page payment-builder">
    <div className="simple-link-heading"><span>Crypto checkout</span><h1>Create payment</h1><p>Configure the asset and network. FlowPay generates a hosted checkout and watches the chain.</p></div>
    <div className="payment-builder-grid"><form action="/api/payments" method="post" className="simple-link-card" onSubmit={submit}>
      <SelectField name="asset" label="Asset" options={assets} onValueChange={option=>setAsset(option.value)}/>
      <Field.Root className="simple-field amount-field" name="amount"><Field.Label>Amount</Field.Label><div><Field.Control placeholder="0.00" value={amount} onChange={event=>setAmount(event.target.value)} inputMode="decimal" min="0.000000000000000001" step="any" required/></div><Field.Error match="valueMissing">Enter an amount.</Field.Error><Field.Error match="rangeUnderflow">Enter an amount greater than zero.</Field.Error></Field.Root>
      <SelectField key={asset} name="chain" label="Network" options={allowedNetworks}/>
      <Field.Root className="simple-field" name="reference"><Field.Label>Description</Field.Label><Field.Control placeholder="What is this payment for?" value={name} onChange={event=>setName(event.target.value)} maxLength={160}/></Field.Root>
      <Field.Root className="simple-field" name="expiry"><Field.Label>Expiry</Field.Label><Field.Control render={<select defaultValue="7"><option value="1">1 day</option><option value="7">7 days</option><option value="30">30 days</option></select>}/></Field.Root>
      {error?<div className="create-link-error">{error}</div>:null}<button className="simple-create-button" type="submit" disabled={submitting}>{submitting?"Generating link…":"Generate payment link"}</button>
    </form><aside className="payment-preview"><span>Live preview</span><div className="preview-orb"><img src={assets.find(option=>option.value===asset)?.icon} alt=""/></div><small>Customer pays</small><strong>{amount||"0.00"} {asset}</strong><p>{allowedNetworks.length} supported network{allowedNetworks.length===1?"":"s"} available</p><ul><li><b>1</b>Share secure checkout</li><li><b>2</b>Customer sends crypto</li><li><b>3</b>FlowPay verifies on-chain</li></ul></aside></div>
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
