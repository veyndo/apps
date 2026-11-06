// Execute a real synthetic commerce component through HTTP and MCP; server-side tokens never enter a shop browser.
import {deepStrictEqual} from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const base=process.env.APP_RUNTIME_URL;
const token=process.env.APP_RUNTIME_TOKEN;
if (!base || !token || !process.argv[2]) throw new Error('Provide APP_RUNTIME_URL, APP_RUNTIME_TOKEN and a WIT component file');
const input={source:await readFile(process.argv[2],'utf8'),hook:'price',snapshot:{subtotal:{minor:25000,currency:'EUR',scale:2}},context:{}};
async function send(path,value){
 const response=await fetch(new URL(path,base),{method:'POST',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},body:JSON.stringify(value),signal:AbortSignal.timeout(15000)});
 const result=await response.json();
 if(!response.ok)throw new Error(`${response.status}: ${result.code ?? 'runtime request failed'}`);
 return result;
}
const direct=await send('/v1/evaluations',input);
const tool=await send('/mcp',{jsonrpc:'2.0',id:1,method:'tools/call',params:{name:'apps.evaluate',arguments:input}});
if(tool.error || tool.result.isError)throw new Error('MCP evaluation failed');
deepStrictEqual(tool.result.structuredContent,direct);
if(direct.mode!=='simulation' || direct.effectApplied!==false)throw new Error('Expected a simulation without effects');
console.log(JSON.stringify({mode:direct.mode,sourceDigest:direct.sourceDigest,outcome:direct.outcome}));
