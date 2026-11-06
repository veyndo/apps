// Actual independent App Data host through the existing generic SDK and ordinary Fetch.
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {createClient}=await import(pathToFileURL(process.env.APP_DATA_SDK_MODULE).href);
const baseUrl=process.env.APP_DATA_URL,token=process.env.APP_DATA_TOKEN;
const input={app:'advice',action:'list_notes',query:{limit:1}};
const client=createClient({baseUrl,token});
await client.initialize();
assert((await client.tools()).tools.some(t=>t.name==='apps.records.read'));
const result=await client.tool('apps.records.read',input);
assert.equal(result.elements[0].title,'Alpha0');
const http=await fetch(baseUrl+'/v1/records/query',{method:'POST',headers:{authorization:`Bearer ${token}`,'content-type':'application/json'},body:JSON.stringify(input)});
assert(http.ok);assert.deepEqual(await http.json(),result);
console.log('PASS existing SDK and Fetch use the same independent record page');
