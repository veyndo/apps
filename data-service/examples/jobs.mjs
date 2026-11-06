// Actual generic SDK supervisor journey against an existing synthetic queued job; no producer, worker or financial authority is created.
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {createClient}=await import(pathToFileURL(process.env.APP_DATA_SDK_MODULE).href);
const client=createClient({baseUrl:process.env.APP_DATA_URL,token:process.env.APP_DATA_TOKEN});
await client.initialize();
const app=process.env.APP_JOB_APP||'app_jobs',id=process.env.APP_JOB_ID;
const page=await client.tool('apps.jobs.list',{app,limit:20});
const job=await client.tool('apps.jobs.detail',{app,id});
assert.equal(job.status,'queued');
await client.tool('apps.jobs.control',{app,id,operation:'cancel',revision:job.revision,packageDigest:page.packageDigest,approve:true});
assert.equal((await client.tool('apps.jobs.detail',{app,id})).status,'cancelled');
console.log('PASS unchanged SDK reads and explicitly cancels an existing job on the independent supervisor');
