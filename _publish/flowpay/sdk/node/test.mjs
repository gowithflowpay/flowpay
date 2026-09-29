import assert from 'node:assert/strict';
import {FlowPay,FlowPayError} from './dist/index.js';
let seen; const fake=async(input,init)=>{seen={input,init};return new Response(JSON.stringify({id:'pay_1',address:'0x1',amount:'10',amount_atomic:'10000000',asset:'USDC',chain:'base',status:'WAITING',expires_at:'x'}),{status:201});};
const c=new FlowPay({apiKey:'fp.test',baseUrl:'http://flowpay.test',fetch:fake}); await c.payments.create({amount:'10',asset:'USDC',chain:'base'},{idempotencyKey:'idem-12345'}); assert.equal(seen.init.headers['idempotency-key'],'idem-12345');
const bad=new FlowPay({apiKey:'x',fetch:async()=>new Response(JSON.stringify({error:{code:'unsupported_asset',message:'nope',request_id:'req_1'}}),{status:400})});try{await bad.payments.get('pay_1');assert.fail('expected');}catch(e){assert.ok(e instanceof FlowPayError);assert.equal(e.code,'unsupported_asset');}
console.log('SDK smoke tests passed');
