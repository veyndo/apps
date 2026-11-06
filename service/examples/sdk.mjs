// The unchanged generic @veyndo/sdk connects to the standalone runtime, using its default MCP negotiation and shared HTTP transport.
import {deepStrictEqual} from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
const modulePath=process.env.APP_RUNTIME_SDK_MODULE;
if(!modulePath)throw new Error('Set APP_RUNTIME_SDK_MODULE to the checked-out @veyndo/sdk src/index.js');
const {createClient}=await import(pathToFileURL(modulePath));
const client=createClient({baseUrl:process.env.APP_RUNTIME_URL,token:()=>process.env.APP_RUNTIME_TOKEN,timeoutMs:15000});
await client.initialize();
const tools=await client.tools();
if(!tools.tools.some(tool=>tool.name==='apps.evaluate'))throw new Error('Runtime evaluation tool missing');
const args={source:await readFile(process.argv[2],'utf8'),hook:'price',snapshot:{subtotal:{minor:25000,currency:'EUR',scale:2}},context:{}};
const direct=await client.request('/v1/evaluations',{method:'POST',body:args});
const result=await client.tool('apps.evaluate',args);
deepStrictEqual(result,direct);
if(result.effectApplied!==false)throw new Error('Expected a simulation without effects');
console.log(JSON.stringify({mode:result.mode,outcome:result.outcome}));
