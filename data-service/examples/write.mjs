// Existing generic SDK uses the same approved, current-package CAS contract as HTTP.
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {createClient}=await import(pathToFileURL(process.env.APP_DATA_SDK_MODULE));
const client=createClient({baseUrl:process.env.APP_DATA_URL,token:process.env.APP_DATA_TOKEN});
await client.initialize();
const page=await client.tool('apps.records.read',{app:'cases',action:'list_cases',query:{}});
const change={app:'cases',action:'save_case',packageDigest:page.packageDigest,approve:true,input:{id:'sdk_case',revision:0,fields:{title:'SDK case',reference:'sdk',status:'new'}}};
const saved=await client.tool('apps.records.write',change);
assert.equal(saved.revision,1);
const history=await client.tool('apps.records.history',{app:'cases',action:'list_cases',id:'sdk_case'});
assert.equal(history.versions[0].revision,1);
assert.equal(history.versions[0].fields.title,'SDK case');
console.log('PASS existing SDK write and retained history on independent managed host');
