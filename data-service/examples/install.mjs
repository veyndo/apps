// Actual complete private data App installation through the unchanged generic SDK and shared host operations.
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {createClient}=await import(pathToFileURL(process.env.APP_DATA_SDK_MODULE).href);
const client=createClient({baseUrl:process.env.APP_DATA_URL,token:process.env.APP_DATA_TOKEN});
await client.initialize();
const manifest={id:'app_sdk_notes',version:'0.1.0',coreApi:'1',runtime:'declarative',name:{en:'SDK notes',de:'SDK Notizen'},permissions:['data.read','data.write'],entities:[{name:'notes',fields:[{name:'title',kind:'string',required:true}]}],actions:[{name:'list_notes',description:'Read notes',handler:'list',entity:'notes',permission:'catalog.read',mcp:true,inputSchema:{type:'object',additionalProperties:false,properties:{id:{type:'string'},limit:{type:'integer'},after:{type:'string'}}}},{name:'save_note',description:'Save note',handler:'save',entity:'notes',permission:'catalog.write',mcp:true,inputSchema:{type:'object',additionalProperties:false,properties:{id:{type:'string'},revision:{type:'integer'},fields:{type:'object'}}}}]};
const review=await client.tool('apps.packages.review',{manifest});
assert.equal(review.revision,0);
const installed=await client.tool('apps.packages.install',{manifest,digest:review.digest,revision:review.revision,permissions:review.permissions,approve:true});
assert.equal(installed.revision,1);
const saved=await client.tool('apps.records.write',{app:manifest.id,action:'save_note',packageDigest:installed.digest,approve:true,input:{id:'one',revision:0,fields:{title:'Installed through the SDK'}}});
assert.equal(saved.revision,1);
const page=await client.tool('apps.records.read',{app:manifest.id,action:'list_notes',query:{}});
assert.equal(page.elements[0].title,'Installed through the SDK');
console.log('PASS unchanged SDK reviews/installs a complete App and writes/reads its newly installed model');
