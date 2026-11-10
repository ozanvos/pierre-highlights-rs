// Bun is used only for maintenance; the published crate has no runtime dependencies.
import {copyFileSync, mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
const upstream = resolve(process.argv[2] ?? '');
if (!process.argv[2]) throw Error('Usage: bun scripts/sync.ts /path/to/pierre/packages/highlights');
const root = new URL('../', import.meta.url);
const pin = JSON.parse(readFileSync(new URL('upstream.json', import.meta.url), 'utf8'));
const tree = spawnSync('git', ['-C', upstream, 'rev-parse', 'HEAD:packages/highlights'], {encoding:'utf8'});
if (tree.status !== 0 || tree.stdout.trim() !== pin.packageTree) {
 throw Error(`Expected Highlights ${pin.version} at package tree ${pin.packageTree}; review and update the pin before syncing another revision.`);
}
if (JSON.parse(readFileSync(upstream + '/package.json', 'utf8')).version !== pin.version) {
 throw Error('Upstream package version does not match the pin');
}
const cache = new URL('.cache/', import.meta.url);
mkdirSync(cache, {recursive:true});
// Resolve build dependencies from scripts/node_modules without modifying upstream.
copyFileSync(upstream + '/scripts/build.ts', new URL('build.ts', cache));
const {transformWat,wat2wasm} = await import(new URL('build.ts',cache).href);
const x = transformWat(upstream + '/src/highlights.wat');
// Compile first: invalid/unsupported WAT must never become a Rust update.
wat2wasm('upstream-highlights', x.code);
writeFileSync(new URL('expanded.wat',cache), x.code);
writeFileSync(new URL('metadata.json',cache), JSON.stringify({languages:x.languages,enums:Object.fromEntries(x.enumMap)},null,2));
function run(cmd:string,args:string[]) {
 const r=spawnSync(cmd,args,{cwd:root,stdio:'inherit'});
 if(r.error)throw r.error;
 if(r.status!==0)throw Error(`${cmd} failed: ${r.status}`);
}
run('python3',['scripts/translate.py','scripts/.cache/expanded.wat','scripts/.cache/metadata.json','.']);
run('bun',['scripts/fixtures.ts',upstream]);
run('cargo',['fmt']);
