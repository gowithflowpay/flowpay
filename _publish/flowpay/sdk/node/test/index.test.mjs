import test from "node:test";
import assert from "node:assert/strict";
import {FlowPay,FlowPayError} from "../dist/index.js";

test("create sends idempotency and api headers",async()=>{let seen;const fake=async(input,init)=>{seen={input,init};return new Response(JSON.stringify({id:"pay_1",address:"0x1",amount:"10",amount_atomic:"10000000",asset:"USDC",chain:"base",status:"WAITING",expires_at:"x",checkout_url:"http://checkout/pay/pay_1"}),{status:201,headers:{"content-type":"application/json"}})};const c=new FlowPay({apiKey:"fp.test",baseUrl:"http://flowpay.test",fetch:fake});await c.payments.create({amount:"10",asset:"USDC",chain:"base"},{idempotencyKey:"idem-12345"});assert.equal(seen.init.headers["idempotency-key"],"idem-12345");assert.equal(seen.init.headers["x-flowpay-api-key"],"fp.test");});

test("structured errors are surfaced",async()=>{const fake=async()=>new Response(JSON.stringify({error:{code:"unsupported_asset",message:"nope",request_id:"req_1"}}),{status:400});const c=new FlowPay({apiKey:"x",fetch:fake});await assert.rejects(()=>c.payments.get("pay_1"),e=>e instanceof FlowPayError&&e.code==="unsupported_asset"&&e.requestId==="req_1");});
