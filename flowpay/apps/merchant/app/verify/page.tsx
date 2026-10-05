import {VerifyForm} from "../components/AuthForms";

export const dynamic="force-dynamic";

export default async function VerifyPage({
  searchParams,
}:{
  searchParams:Promise<{email?:string;notice?:string;purpose?:string;next?:string}>;
}){
  const {email,notice,purpose,next}=await searchParams;
  return <VerifyForm email={email??""} initialNotice={notice??null} purpose={purpose} next={next??null}/>;
}
