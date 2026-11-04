import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const directory = mkdtempSync(join(tmpdir(), "jev-flow-test-"));
const log = join(directory, "batches.jsonl");
const preload = join(directory, "fetch.mjs");
const entry = new URL("./flow.mjs", import.meta.url);

writeFileSync(join(directory, "snapshot"), `
const nodes = [1, 2].map(i => ({ref_id:'@s1:e'+i,role:'button',name:'Button '+i,available_actions:['Click']}));
const result = process.argv.includes('--surface')
  ? {ok:false,error:{code:'INVALID_ARGS'}}
  : {ok:true,data:{window:{id:'w-1',title:'Fixture'},tree:{role:'window',children:nodes}}};
console.log(JSON.stringify(result));
`);
writeFileSync(join(directory, "batch"), `
const fs = require('node:fs');
const entries = JSON.parse(process.argv.at(-1));
const before = fs.readFileSync(process.env.JEV_TEST_LOG, 'utf8');
fs.appendFileSync(process.env.JEV_TEST_LOG, JSON.stringify(entries)+'\\n');
const safe = process.env.JEV_TEST_MODE === 'stale_safe';
const error = {code:process.env.JEV_TEST_MODE === 'timeout' ? 'TIMEOUT' : 'STALE_REF',
  disposition:{retry:safe?'safe':'unsafe',delivery:safe?'not_delivered':'delivery_uncertain'}};
console.log(JSON.stringify({ok:true,data:{results:before ? [{ok:true}] : [{ok:true},{ok:false,error}]}}));
`);
writeFileSync(preload, `
globalThis.fetch = async (_url, options) => {
  const request = JSON.parse(options.body);
  const answers = Object.fromEntries(Object.keys(request.questions).map(key => [key,
    key.startsWith('target_')
      ? {type:'choice',choice:'@s1:e'+(Number(key.split('_')[1])+1),confidence:0.99}
      : {type:'noul',noul:0.1}]));
  return {ok:true,json:async()=>({answers})};
};
`);

try {
  for (const mode of ["timeout", "stale_unsafe", "stale_safe"]) {
    writeFileSync(log, "");
    const result = spawnSync(process.execPath, ["--import", pathToFileURL(preload).href,
      fileURLToPath(entry), "--app", "Fixture", "--window-id", "w-1",
      "--execute", "Button 1", "Button 2"], {
      cwd: directory, encoding: "utf8", timeout: 10000,
      env: { ...process.env, TYPESAFE_API_KEY: "fixture", AGENT_DESKTOP_BIN: process.execPath,
        JEV_TEST_LOG: log, JEV_TEST_MODE: mode },
    });
    assert.ifError(result.error);
    const events = result.stdout.trim().split("\n").map(JSON.parse);
    const batches = readFileSync(log, "utf8").trim().split("\n").map(JSON.parse);
    const last = events.at(-1);
    if (mode === "stale_safe") {
      assert.equal(result.status, 0, result.stderr);
      assert.equal(last.done, true);
      assert.equal(batches.length, 2);
      assert.equal(batches[1].length, 1, "a completed step is never replayed");
      assert.equal(batches[1][0].args.ref_id, "@s1:e2");
    } else {
      assert.equal(result.status, 1, mode);
      assert.equal(last.stop, "action_failed", mode);
      assert.equal(batches.length, 1, "partial success does not permit retrying an unsafe error");
    }
  }
} finally {
  rmSync(directory, { recursive: true, force: true });
}

console.log("ok");
