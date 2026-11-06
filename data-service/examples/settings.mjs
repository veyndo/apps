// Existing Veyndo SDK and ordinary Fetch configure the same admitted installed App; use only a synthetic app for this example.
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {createClient}=await import(pathToFileURL(process.env.APP_DATA_SDK_MODULE).href);
const base=process.env.APP_DATA_URL;
const token=process.env.APP_DATA_TOKEN;
const client=createClient({baseUrl:base,token});
await client.initialize();
const current=await client.tool('apps.settings.read',{app:'advice'});
assert.equal(current.revision,4);
assert.equal('key' in current.values,false);
const saved=await client.tool('apps.settings.save',{app:'advice',digest:current.digest,revision:current.revision,approve:true,values:{label:'SDK settings'}});
assert.equal(saved.revision,5);
const response=await fetch(base+'/v1/settings/read',{method:'POST',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},body:JSON.stringify({app:'advice'})});
assert.equal(response.status,200);
const read=await response.json();
assert.equal(read.values.label,'SDK settings');
assert.equal('key' in read.values,false);
console.log('PASS existing SDK and Fetch use the same encrypted, secret-safe settings workflow');
