import {api} from "../../../lib/api";
import {CheckoutClient,type Deposit,type Payment} from "./CheckoutClient";

export const dynamic="force-dynamic";

export default async function PaymentPage({params,searchParams}:{params:Promise<{id:string}>;searchParams:Promise<{receipt?:string}>}){
  const {id}=await params;
  const query=await searchParams;
  let initialPayment:Payment|null=null;
  let initialDeposits:Deposit[]=[];
  try{
    const [payment,deposits]=await Promise.all([
      api(`/v1/payments/${encodeURIComponent(id)}`),
      api(`/v1/payments/${encodeURIComponent(id)}/deposits`),
    ]);
    initialPayment=payment as Payment;
    initialDeposits=Array.isArray(deposits?.data)?deposits.data as Deposit[]:[];
  }catch{
    // The client retries transient backend failures without delaying the page shell.
  }
  return <CheckoutClient paymentId={id} suppressOutcome={query.receipt==="1"} initialPayment={initialPayment} initialDeposits={initialDeposits}/>;
}
