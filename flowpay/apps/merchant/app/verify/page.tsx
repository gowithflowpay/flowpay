import {VerifyForm} from "../components/AuthForms";

export const dynamic="force-dynamic";

export default async function VerifyPage({
  searchParams,
}:{
  searchParams:Promise<{email?:string;notice?:string}>;
}){
  const {email,notice}=await searchParams;
  return <VerifyForm email={email??""} initialNotice={notice??null}/>;
}
