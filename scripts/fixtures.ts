// Refresh raw UTF-8 reference records using upstream Wasm, independently of Rust.
import {readFileSync,writeFileSync} from 'node:fs';
import {gzipSync,gunzipSync} from 'node:zlib';
import {pathToFileURL} from 'node:url';
const upstream = process.argv[2];
if (!upstream) throw Error('Usage: bun scripts/fixtures.ts /path/to/pierre/packages/highlights');
const root = new URL('../', import.meta.url);
const {wat2wasm} = await import('./.cache/build.ts');
const {samples} = await import(pathToFileURL(upstream + '/test/_samples.ts').href);
const x = {
 code: readFileSync(new URL('.cache/expanded.wat', import.meta.url), 'utf8'),
 ...JSON.parse(readFileSync(new URL('.cache/metadata.json', import.meta.url), 'utf8')),
};
const start=x.code.indexOf('(func $recLinesPost');
let end=start,depth=0;
do { if(x.code[end]==='(')depth++; if(x.code[end]===')')depth--; end++; }while(depth);
const code=x.code.slice(0,start)+'(func $recLinesPost)'+x.code.slice(end);
const wasm=wat2wasm('raw-record-reference',code);
let memory: WebAssembly.Memory;
const decoder=new TextDecoder();
const id=(p:number,n:number,re:RegExp)=> +re.test(decoder.decode(new Uint8Array(memory.buffer,p,n)));
const {instance}=await WebAssembly.instantiate(wasm,{env:{is_id_start:(p:number,n:number)=>id(p,n,/^\p{ID_Start}$/u),is_id_continue:(p:number,n:number)=>id(p,n,/^\p{ID_Continue}$/u)}});
memory=instance.exports.memory as WebAssembly.Memory;
const highlight=instance.exports.highlight as Function;
const cases: any[]=[];
const seen=new Set<string>();
function add(language:string,name:string,source:string){
 const key=language+'\0'+source;if(seen.has(key))return;seen.add(key);
 const lang=x.languages[language];if(lang===undefined)throw Error(language);
 const bytes=new TextEncoder().encode(source);
 const required=81920+bytes.length+96;
 if(memory.buffer.byteLength<required)memory.grow(Math.ceil((required-memory.buffer.byteLength)/65536));
 let b=new Uint8Array(memory.buffer);b.set(bytes,81920);b.fill(0,81920+bytes.length,81920+bytes.length+32);
 let dv=new DataView(memory.buffer);b[0]=lang;b[1]=3;dv.setUint32(2,bytes.length,true);
 highlight();dv=new DataView(memory.buffer);
 const out=dv.getUint32(6,true),len=dv.getUint32(10,true);
 const expected=Array.from({length:len/4},(_,i)=>dv.getUint32(out+i*4,true));
 cases.push({lang,language,name,source,expected});
}
const old=gunzipSync(readFileSync(new URL('tests/data/parity.jsonl.gz',root))).toString().trim().split('\n').map(s=>JSON.parse(s));
for(const c of old)add(c.language??c.name.split('/')[0],c.name,c.source);
const edges=['','\0','a\0b','λ😀','\r\n','a\r\nb\rc\n','/* unterminated','"unterminated','<script>const x = 1</script>','\\\n','\u2028\u2029','// comment\n','{}[]()','𝒳 = café + á','a'.repeat(257)];
let seed=0x12345678;
const alphabet=Array.from('abcXYZ_09 \t\r\n"\'`/*#<>[]{}():;=+-.$\\λ😀\0');
for(const [language,lang] of Object.entries(x.enums['$Language'])){
 for(let i=0;i<edges.length;i++)add(language,`${language}/upstream-edge${i}`,edges[i]);
 const sample=samples[language]?.code;
 if(sample){
  add(language,`${language}/upstream-sample`,sample);
  add(language,`${language}/upstream-crlf`,sample.replaceAll('\n','\r\n'));
  const chars=Array.from(sample);
  for(let i=1;i<chars.length;i+=Math.max(1,Math.floor(chars.length/100)))add(language,`${language}/upstream-prefix${i}`,chars.slice(0,i).join(''));
 }
 for(let i=0;i<100;i++){
  let source='';for(let j=0;j<i+1;j++){seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;source+=alphabet[(seed>>>0)%alphabet.length];}
  add(language,`${language}/upstream-fuzz${i}`,source);
 }
}
writeFileSync(new URL('tests/data/parity.jsonl.gz',root),gzipSync(cases.map(c=>JSON.stringify(c)).join('\n')+'\n',{level:9}));
console.log({cases:cases.length,languages:new Set(cases.map(c=>c.lang)).size});
