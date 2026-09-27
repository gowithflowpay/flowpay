import {redirect} from "next/navigation";
import {OnboardingForm} from "../components/AuthForms";
import {requireMerchant} from "../../lib/session";

export const dynamic="force-dynamic";

export default async function OnboardingPage(){
  const merchant=await requireMerchant();
  if(merchant.onboarding_completed)redirect("/");
  return <OnboardingForm
    defaultName={merchant.business_name}
    defaultContact={merchant.contact_name}
  />;
}
