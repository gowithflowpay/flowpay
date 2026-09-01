import Link from "next/link";
import {Brand,LanguageButton} from "../../../components/Brand";
import {CheckIcon,LockIcon} from "../../../components/Icons";

export default async function PaymentSuccessPage({params}:{params:Promise<{id:string}>}){
  const {id}=await params;
  return <main className="checkout-shell">
    <header className="payment-header"><span/><Brand/><LanguageButton/></header>
    <section className="checkout-card success-card" aria-live="polite">
      <div className="success-orb"><CheckIcon/></div>
      <span className="eyebrow">Payment confirmed</span>
      <h1>Payment successful</h1>
      <p>Your payment was confirmed and the merchant has been notified.</p>
      <Link className="primary-button inline" href={`/pay/${encodeURIComponent(id)}`}>View payment</Link>
    </section>
    <footer className="public-footer">
      <span><LockIcon/> Secured by FlowPay</span>
      <nav><a href="#">Terms</a><a href="#">Privacy</a><a href="mailto:support@flowpay.dev">Support</a></nav>
    </footer>
  </main>;
}
