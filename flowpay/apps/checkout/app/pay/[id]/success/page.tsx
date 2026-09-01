import Image from "next/image";
import {notFound,redirect} from "next/navigation";
import {api} from "../../../../lib/api";
import {Brand} from "../../../components/Brand";
import {CheckIcon,ExternalIcon} from "../../../components/Icons";
import type {Payment} from "../CheckoutClient";

export const dynamic="force-dynamic";

const networkMeta:Record<string,{label:string;icon:string}>={
  base:{label:"Base",icon:"/assets/base.svg"},base_sepolia:{label:"Base",icon:"/assets/base.svg"},
  ethereum:{label:"Ethereum",icon:"/assets/ethereum.svg"},ethereum_sepolia:{label:"Ethereum",icon:"/assets/ethereum.svg"},
  bsc:{label:"BNB Chain",icon:"/assets/bsc.svg"},bsc_testnet:{label:"BNB Chain",icon:"/assets/bsc.svg"},
};

export default async function PaymentSuccessPage({params}:{params:Promise<{id:string}>}){
  const {id}=await params;let payment:Payment;
  try{payment=await api(`/v1/payments/${encodeURIComponent(id)}`) as Payment}catch{notFound()}
  if(!["COMPLETED","RECOVERED"].includes(payment.status))redirect(`/pay/${encodeURIComponent(id)}`);
  const network=networkMeta[payment.chain]??{label:payment.chain.replaceAll("_"," "),icon:"/assets/base.svg"};
  const merchantBase=(process.env.FLOWPAY_MERCHANT_BASE_URL?.trim()||"/").replace(/\/$/,"");
  const completedAt=new Intl.DateTimeFormat("en-US",{dateStyle:"medium",timeStyle:"short",timeZone:"Africa/Lagos"}).format(new Date());
  return <main className="success-page"><header className="success-header"><Brand/><div className="success-header-right"><a href="mailto:support@flowpay.dev"><span>?</span>Need help?</a><i/><b>{(payment.merchant_name??"Urban Tech").split(/\s+/).map(word=>word[0]).join("").slice(0,2).toUpperCase()}</b><strong>{payment.merchant_name??"Urban Tech"}</strong><span>⌄</span></div></header><section className="success-receipt"><div className="receipt-check"><CheckIcon/><i/><i/><i/><i/></div><h1>Payment successful!</h1><p>Your payment has been received and confirmed.</p><div className="receipt-rule"/><dl><div><dt><b>$</b>Amount</dt><dd>{Number(payment.amount).toFixed(2)} {payment.asset}</dd></div><div><dt><b>⌁</b>Network</dt><dd><Image src={network.icon} width={20} height={20} alt=""/>{network.label}</dd></div><div><dt><b>▤</b>Payment ID</dt><dd>{payment.id}</dd></div><div><dt><b>◷</b>Date</dt><dd>{completedAt}</dd></div></dl><a className="receipt-primary" href={merchantBase||"/"}>Go back to merchant</a><a className="receipt-secondary" href={`/pay/${encodeURIComponent(payment.id)}?receipt=1`}>View payment details <ExternalIcon/></a></section><footer className="success-powered">Powered by <strong>FlowPay</strong></footer></main>;
}
