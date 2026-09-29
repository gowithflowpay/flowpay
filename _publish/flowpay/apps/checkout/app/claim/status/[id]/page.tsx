import {ClaimStatusClient} from "./ClaimStatusClient";
export default async function ClaimStatusPage({params}:{params:Promise<{id:string}>}){const {id}=await params;return <ClaimStatusClient claimId={id}/>}
