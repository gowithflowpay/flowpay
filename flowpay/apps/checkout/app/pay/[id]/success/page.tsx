import Image from "next/image";
import Link from "next/link";
import {redirect} from "next/navigation";
import {api} from "../../../../lib/api";
import {Brand} from "../../../components/Brand";
import {AmountIcon,CheckIcon,ExternalIcon,NetworkIcon,QuestionIcon,ReceiptIcon} from "../../../components/Icons";

type Payment={id:string;amount:string;asset:string;chain:string;status:string;merchant_name?:string|null};
const networks:Record<string,{label:string;asset:string}>={base:{label:"Base",asset:"/assets/base.svg"},bsc:{label:"BNB Smart Chain",asset:"/assets/bsc.svg"}};

export const dynamic="force-dynamic";

export default async function PaymentSuccessPage({params}:{params:Promise<{id:string}>}){
  const {id}=await params;
  let payment:Payment|null=null;
  try{payment=await api(`/v1/payments/${encodeURIComponent(id)}`) as Payment;}catch{}
  if(!payment||!new Set(["COMPLETED","RECOVERED","CONFIRMED","SETTLING"]).has(payment.status))redirect(`/pay/${encodeURIComponent(id)}?receipt=1`);
  const network=networks[payment.chain]??{label:payment.chain||"—",asset:"/assets/base.svg"};


  return <main className="success-page">
    <header className="success-header"><Brand/><div className="success-header-right"><a href="mailto:support@flowpay.dev"><span>?</span> Need help?</a><i/><b>{(payment.merchant_name?.trim()||"FP").slice(0,2).toUpperCase()}</b><strong>{payment.merchant_name?.trim()||"FlowPay merchant"}</strong></div></header>
    <section className="success-receipt" aria-live="polite">
      <div className="receipt-check"><CheckIcon/><i/><i/><i/><i/></div>
      <h1>Payment successful!</h1>
      <p>Your payment has been received and confirmed.</p>
      <div className="receipt-rule"/>
      <dl>
        <div><dt><b><AmountIcon/></b> Amount</dt><dd>{payment.amount} {payment.asset}</dd></div>
        <div><dt><b><NetworkIcon/></b> Network</dt><dd><Image src={network.asset} width={20} height={20} alt=""/>{network.label}</dd></div>
        <div><dt><b><ReceiptIcon/></b> Payment ID</dt><dd>{payment.id}</dd></div>
        <div><dt><b><QuestionIcon/></b> Status</dt><dd>{payment.status==="RECOVERED"?"Recovered":payment.status==="SETTLING"?"Settling":payment.status==="COMPLETED"?"Completed":"Confirmed"}</dd></div>
      </dl>
      <Link className="receipt-primary" href={`/pay/${encodeURIComponent(id)}?receipt=1`}>View payment details <ExternalIcon/></Link>
    </section>
    <p className="success-powered">Powered by <strong>FlowPay</strong></p>
  </main>;
}
