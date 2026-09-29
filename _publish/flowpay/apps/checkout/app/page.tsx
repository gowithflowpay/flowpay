import {redirect} from "next/navigation";

export default async function Home({searchParams}:{searchParams:Promise<{payment_id?:string}>}){
  const query=await searchParams;
  if(query.payment_id)redirect(`/pay/${encodeURIComponent(query.payment_id)}`);
  redirect(process.env.FLOWPAY_MERCHANT_BASE_URL?.trim()||"http://localhost:3000");
}
