import {LoginForm} from "../components/AuthForms";

export const dynamic="force-dynamic";

export default async function LoginPage({
  searchParams,
}:{
  searchParams:Promise<{next?:string}>;
}){
  const {next}=await searchParams;
  const target=next&&next.startsWith("/")?next:null;
  return <LoginForm next={target}/>;
}
