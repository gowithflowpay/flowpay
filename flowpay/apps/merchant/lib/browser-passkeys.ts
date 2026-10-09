function decode(value:string):ArrayBuffer{
  const text=atob(value.replace(/-/g,"+").replace(/_/g,"/"));
  return Uint8Array.from(text,c=>c.charCodeAt(0)).buffer;
}
function encode(value:ArrayBuffer):string{
  return btoa(String.fromCharCode(...new Uint8Array(value))).replace(/\+/g,"-").replace(/\//g,"_").replace(/=+$/,"");
}
function supported(){
  if(!window.isSecureContext||!window.PublicKeyCredential)throw new Error("Open FlowPay in a browser that supports passkeys over HTTPS.");
}
function serialize(credential:PublicKeyCredential){
  const response=credential.response;
  const base={id:credential.id,rawId:encode(credential.rawId),type:credential.type,extensions:credential.getClientExtensionResults()};
  if("attestationObject" in response){
    const attestation=response as AuthenticatorAttestationResponse;
    // Older passkey browsers can create credentials without exposing transport hints.
    const transports=typeof attestation.getTransports==="function"?attestation.getTransports():[];
    return {...base,response:{attestationObject:encode(attestation.attestationObject),clientDataJSON:encode(attestation.clientDataJSON),transports}};
  }
  const assertion=response as AuthenticatorAssertionResponse;
  return {...base,response:{authenticatorData:encode(assertion.authenticatorData),clientDataJSON:encode(assertion.clientDataJSON),signature:encode(assertion.signature),userHandle:assertion.userHandle?encode(assertion.userHandle):null}};
}
export async function createPasskey(options:any){
  supported();
  const key=options.publicKey;
  const credential=await navigator.credentials.create({publicKey:{...key,challenge:decode(key.challenge),user:{...key.user,id:decode(key.user.id)},excludeCredentials:key.excludeCredentials?.map((c:any)=>({...c,id:decode(c.id)}))}}) as PublicKeyCredential|null;
  if(!credential)throw new Error("Passkey setup was cancelled. Try again when you are ready.");
  return serialize(credential);
}
export async function authenticatePasskey(options:any){
  supported();
  const key=options.publicKey;
  const credential=await navigator.credentials.get({publicKey:{...key,challenge:decode(key.challenge),allowCredentials:key.allowCredentials?.map((c:any)=>({...c,id:decode(c.id)}))}}) as PublicKeyCredential|null;
  if(!credential)throw new Error("Passkey sign-in was cancelled. Try again when you are ready.");
  return serialize(credential);
}
export function passkeyError(error:unknown){
  if(error instanceof DOMException&&error.name==="NotAllowedError")return "Passkey request was cancelled or timed out. Try again and approve it on your device.";
  return error instanceof Error?error.message:"Passkey verification failed. Please try again.";
}
