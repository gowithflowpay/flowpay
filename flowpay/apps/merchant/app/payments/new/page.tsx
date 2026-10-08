import {PaymentLinkForm} from "./PaymentLinkForm";
import {apiPublic} from "../../../lib/api";
import {PaymentAsset} from "../../../lib/payment-assets";

export default async function NewPayment(){
  try{
    const result=await apiPublic("/v1/payment-assets",{signal:AbortSignal.timeout(10000)});
    const catalog:PaymentAsset[]=Array.isArray(result.data)?result.data:[];
    if(!catalog.length)throw new Error("No enabled payment assets");
    return <PaymentLinkForm catalog={catalog}/>;
  }catch{
    return <div className="simple-link-page payment-builder"><div className="simple-link-heading"><span>Crypto checkout</span><h1>Create payment</h1><p>Supported assets could not be loaded. Refresh this page to try again.</p></div><a href="/payments/new" className="primary-button">Try again</a></div>;
  }
}
