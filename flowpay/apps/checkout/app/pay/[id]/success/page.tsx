import Image from "next/image";
import Link from "next/link";
import {redirect} from "next/navigation";
import {api} from "../../../../lib/api";
import {Brand} from "../../../components/Brand";
import {AmountIcon,CheckIcon,ExternalIcon,NetworkIcon,QuestionIcon,ReceiptIcon} from "../../../components/Icons";

type Payment={id:string;amount:string;asset:string;chain:string;status:string;merchant_name?:string|null};
const networks:Record<string,{label:string;asset:string}>={
  monad_testnet:{label:"Monad Testnet",asset:"/assets/monad.svg"},
  base:{label:"Base",asset:"/assets/base.svg"},
  base_sepolia:{label:"Base Sepolia",asset:"/assets/base.svg"},
  bsc:{label:"BNB Smart Chain",asset:"/assets/bsc.svg"},
  bsc_testnet:{label:"BNB Smart Chain Testnet",asset:"/assets/bsc.svg"},
  ethereum:{label:"Ethereum",asset:"/assets/ethereum.svg"},
  ethereum_sepolia:{label:"Ethereum Sepolia",asset:"/assets/ethereum.svg"},
  arbitrum:{label:"Arbitrum",asset:"/assets/ethereum.svg"},
  arbitrum_sepolia:{label:"Arbitrum Sepolia",asset:"/assets/ethereum.svg"},
};

export const dynamic="force-dynamic";

export default async function PaymentSuccessPage({params}:{params:Promise<{id:string}>}){
  const {id}=await params;
  let payment:Payment|null=null;
  try{payment=await api(`/v1/public/payments/${encodeURIComponent(id)}`) as Payment;}catch{}
  if(payment?.status==="RECOVERED")redirect(`/pay/${encodeURIComponent(id)}`);
  if(!payment||!new Set(["COMPLETED","RECOVERED","CONFIRMED","SETTLING"]).has(payment.status))redirect(`/pay/${encodeURIComponent(id)}?receipt=1`);
  const network=networks[payment.chain]??{label:payment.chain||"—",asset:"/assets/base.svg"};


  const refunded=payment.status==="RECOVERED";

  return <main className="success-page">
    <header className="success-header"><Brand/><div className="success-header-right"><a href="mailto:support@flowpay.dev"><span>?</span> Need help?</a><i/><b>{(payment.merchant_name?.trim()||"FP").slice(0,2).toUpperCase()}</b><strong>{payment.merchant_name?.trim()||"FlowPay merchant"}</strong></div></header>
    <section className={`success-receipt${refunded?" refund-receipt":""}`} aria-live="polite">
      <div className="receipt-check"><CheckIcon/><i/><i/><i/><i/></div>
      <h1>{refunded?"Payment cancelled":"Payment successful!"}</h1>
      <p>{refunded?"The wrong-asset payment was recovered and refunded to its originating wallet.":"Your payment has been received and confirmed."}</p>
      <div className="receipt-rule"/>
      <dl>
        <div><dt><b><AmountIcon/></b> Amount</dt><dd>{payment.amount} {payment.asset}</dd></div>
        <div><dt><b><NetworkIcon/></b> Network</dt><dd><Image src={network.asset} width={20} height={20} alt=""/>{network.label}</dd></div>
        <div><dt><b><ReceiptIcon/></b> Payment ID</dt><dd>{payment.id}</dd></div>
        <div><dt><b><QuestionIcon/></b> Status</dt><dd>{payment.status==="RECOVERED"?"Refunded — cancelled":payment.status==="SETTLING"?"Settling":payment.status==="COMPLETED"?"Completed":"Confirmed"}</dd></div>
      </dl>
      <Link className="receipt-primary" href={`/pay/${encodeURIComponent(id)}?receipt=1`}>{refunded?"View refund details":"View payment details"} <ExternalIcon/></Link>
    </section>
    <p className="success-powered">Powered by <strong>FlowPay</strong></p>
  </main>;
}
