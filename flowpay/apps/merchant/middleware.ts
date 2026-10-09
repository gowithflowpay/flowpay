import {NextRequest,NextResponse} from "next/server";
import {ONBOARDED_COOKIE,SESSION_COOKIE} from "./lib/cookies";

// "/" is the public marketing page, so a signed-out visitor can read it.
// Checkout and claim pages are public too: they are what a customer opens.
const PUBLIC_PATHS=[
  "/docs",
  "/api/auth",
  "/api/payment",
  "/api/claims",
  "/pay",
  "/claim",
  "/_next",
  "/assets",
  "/favicon.ico",
  "/robots.txt",
];
// Matched on whole path segments so the public "/pay" and "/api/payment"
// proxies never leak onto the merchant "/payments" and "/api/payments" routes.
function isPublic(pathname:string){
  return PUBLIC_PATHS.some(base=>pathname===base||pathname.startsWith(`${base}/`));
}
// Pages that only make sense when signed out.
const AUTH_PAGES=["/login","/signup","/verify"];

export function middleware(request:NextRequest){
  // Passkey responses must come from the origin registered by the API.
  if(request.nextUrl.hostname==="www.pixuno.xyz"){
    const canonical=request.nextUrl.clone();
    canonical.hostname="pixuno.xyz";
    canonical.protocol="https:";
    canonical.port="";
    return NextResponse.redirect(canonical,308);
  }
  const {pathname}=request.nextUrl;
  if(pathname==="/")return NextResponse.next();
  if(isPublic(pathname))return NextResponse.next();

  const session=request.cookies.get(SESSION_COOKIE)?.value;
  const onboarded=request.cookies.get(ONBOARDED_COOKIE)?.value==="1";

  if(AUTH_PAGES.includes(pathname)){
    if(session)return NextResponse.redirect(new URL(onboarded?"/dashboard":"/onboarding",request.url));
    return NextResponse.next();
  }

  if(pathname.startsWith("/onboarding")){
    if(!session)return NextResponse.redirect(new URL("/login",request.url));
    if(onboarded)return NextResponse.redirect(new URL("/dashboard",request.url));
    return NextResponse.next();
  }

  if(!session){
    // API callers get a machine-readable refusal; browsers are sent to the sign-in page.
    if(pathname.startsWith("/api/"))return NextResponse.json({error:{message:"Authentication required."}},{status:401});
    const target=new URL("/login",request.url);
    target.searchParams.set("next",pathname);
    return NextResponse.redirect(target);
  }
  return NextResponse.next();
}

export const config={
  matcher:["/((?!_next/static|_next/image).*)"],
};
