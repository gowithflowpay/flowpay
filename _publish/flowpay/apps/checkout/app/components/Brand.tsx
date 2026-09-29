import Image from "next/image";

export function Brand({compact=false}:{compact?:boolean}){
  return <a href="/" className={`brand-lockup${compact?" compact":""}`} aria-label="FlowPay home">
    <Image src="/assets/flowpay-mark.svg" width={compact?38:46} height={compact?38:46} alt="" priority/>
    <span>FlowPay</span>
  </a>
}

export function LanguageButton(){
  return <button className="language-button" type="button" aria-label="Language: English" title="English is the current supported language">
    <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M3.6 9h16.8M3.6 15h16.8M12 3c2.6 2.4 4 5.4 4 9s-1.4 6.6-4 9c-2.6-2.4-4-5.4-4-9s1.4-6.6 4-9Z"/></svg>
    <span>English</span>
    <svg className="chevron" viewBox="0 0 24 24" aria-hidden="true"><path d="m8 10 4 4 4-4"/></svg>
  </button>
}
