#!/usr/bin/env python3
"""Translate upstream's expanded folded WAT to native Rust (no Wasm runtime).
Usage: translate.py EXPANDED_WAT METADATA_JSON OUTPUT_ROOT
Unsupported instructions fail closed. Operands are evaluated into temporaries
in Wasm order before mutation, preserving side effects and wrapping arithmetic.
"""
import json
import re
import sys
from pathlib import Path

TYPES = {'i32': 'u32', 'i64': 'u64', 'v128': 'V128'}
def ident(s):
    return 'w_' + re.sub(r'\W', '_', s.lstrip('$'))
def parse(text):
    tokens = re.findall(r';;[^\n]*|"(?:\\.|[^"\\])*"|[()]|[^\s()]+', text)
    stack, result = [], []
    for token in tokens:
        if token.startswith(';;'):
            continue
        if token == '(':
            stack.append([])
        elif token == ')':
            node = stack.pop()
            (stack[-1] if stack else result).append(node)
        else:
            stack[-1].append(token)
    assert not stack and len(result) == 1
    return result[0]
def number(s, typ):
    return str(int(s, 0) % (1 << (64 if typ == 'u64' else 32))) + typ

def declarations(f, kind):
    return [x for x in f if isinstance(x, list) and x[0] == kind]

def optimize_scan_masks(f):
    """Keep the existing NEON narrowing fast path only for opaque scan masks.

    Masks used in arithmetic, popcount, or bitwise filtering retain exact Wasm
    bits. Safe masks may only be tested for zero or queried for their first bit.
    """
    assignments, uses = {}, {}
    def walk(n, parent=None):
        if not isinstance(n, list):
            return
        if n[0] in ('local.set', 'local.tee'):
            assignments.setdefault(n[1], []).append(n[2] if len(n) > 2 else None)
        if n[0] == 'local.get':
            uses.setdefault(n[1], []).append(parent[0])
        for child in n:
            walk(child, n)
    walk(f)
    def mask(n):
        return isinstance(n, list) and (n[0] == 'i8x16.bitmask' or
            (n[0] == 'i32.xor' and n[1][0] == 'i8x16.bitmask' and
             n[2][0] == 'i32.const' and int(n[2][1], 0) == 65535))
    safe = {name for name, values in assignments.items()
            if all(mask(v) for v in values) and
            set(uses.get(name, [])) <= {'if', 'i32.eqz', 'i32.ctz'}}
    def rewrite(n):
        if not isinstance(n, list):
            return n
        if n[0] == 'local' and n[1] in safe:
            return ['local', n[1], 'i64']
        if n[0] in ('local.set', 'local.tee') and n[1] in safe:
            value = n[2]
            return [n[0], n[1], ['scan.mask', rewrite(value[1])] if value[0] == 'i8x16.bitmask'
                    else ['scan.mask_not', rewrite(value[1][1])]]
        if n[0] == 'i32.ctz' and n[1][0] == 'local.get' and n[1][1] in safe:
            return ['scan.first', n[1]]
        return [rewrite(c) for c in n]
    return rewrite(f)

class Translator:
    def __init__(self, module):
        self.globals = {x[1]: TYPES[x[2][1] if isinstance(x[2], list) else x[2]] for x in module[1:] if x[0] == 'global'}
        self.funcs = {}
        for x in module[1:]:
            if x[0] == 'import':
                x = x[3]
            if x[0] == 'func':
                if isinstance(x[1], list):
                    x.insert(1, '$' + json.loads(x[1][1]))
                self.funcs[x[1]] = x
        self.results = {n: TYPES[declarations(f, 'result')[0][1]] if declarations(f, 'result') else None for n, f in self.funcs.items()}
        self.table = next(x for x in module[1:] if x[0] == 'table')[-1][1:]
    def emit(self, line):
        self.lines.append(line)
    def temp(self, expression, typ):
        if typ is None:
            self.emit(expression + ';')
            return None
        name = 't' + str(self.counter)
        self.counter += 1
        self.emit(f'let {name}: {typ} = {expression};')
        return name, typ
    def branch(self, label, value=None):
        target = next(t for t in reversed(self.labels) if t[0] == label) if label.startswith('$') else self.labels[-1-int(label)]
        return ('continue' if target[2] == 'loop' else 'break') + " '" + target[1] + ((' ' + value) if value else '')
    def seq(self, nodes):
        stack = []
        for node in nodes:
            if node[0] == 'local.set' and len(node) == 2:
                self.emit(f'{ident(node[1])} = {stack.pop()[0]};')
            else:
                value = self.expr(node)
                if value is not None:
                    stack.append(value)
        return stack[-1] if stack else None
    def expr(self, node):
        assert isinstance(node, list), node
        op, *args = node
        if op.startswith('scan.'):
            val = self.expr(args[0])[0]
            method = {'scan.mask': 'scan_mask', 'scan.mask_not': 'scan_mask_not', 'scan.first': 'mask_first'}[op]
            return self.temp(f'V128::{method}({val})', 'u32' if op == 'scan.first' else 'u64')
        if op in ('block', 'loop'):
            label = args.pop(0) if args and isinstance(args[0], str) else '$anonymous'
            typ = TYPES[args.pop(0)[1]] if args and args[0][0] == 'result' else None
            rustlabel = 'b' + str(self.counter)
            self.counter += 1
            dest = 'r' + str(self.counter)
            self.labels.append((label, rustlabel, op))
            self.emit((f'let {dest}: {typ} = ' if typ else '') + f"'{rustlabel}: " + ('loop {' if op == 'loop' else '{'))
            result = self.seq(args)
            if typ and result:
                self.emit(('break ' if op == 'loop' else '') + result[0] + (';' if op == 'loop' else ''))
            elif typ:
                self.emit('unreachable!("validated WAT result");')
            elif op == 'loop':
                self.emit('break;')
            self.emit('};')
            self.labels.pop()
            return (dest, typ) if typ else None
        if op == 'if':
            typ = TYPES[args.pop(0)[1]] if args[0][0] == 'result' else None
            cond = self.expr(args.pop(0))[0]
            dest = 'r' + str(self.counter)
            self.counter += 1
            self.emit((f'let {dest}: {typ} = ' if typ else '') + f'if {cond} != 0 {{')
            then = self.seq(args[0][1:])
            if typ and then:
                self.emit(then[0])
            if len(args) > 1:
                self.emit('} else {')
                other = self.seq(args[1][1:])
                if typ and other:
                    self.emit(other[0])
            self.emit('};')
            return (dest, typ) if typ else None
        if op in ('br', 'br_if', 'br_table'):
            labels = [a for a in args if isinstance(a, str)]
            vals = [self.expr(a)[0] for a in args if isinstance(a, list)]
            if op == 'br':
                self.emit(self.branch(labels[0], vals[0] if vals else None) + ';')
            elif op == 'br_if':
                self.emit(f'if {vals[-1]} != 0 {{ ' + self.branch(labels[0], vals[0] if len(vals)>1 else None) + '; }')
            else:
                self.emit(f'match {vals[-1]} {{')
                for i, label in enumerate(labels):
                    self.emit(f'{i if i < len(labels)-1 else "_"} => ' + self.branch(label) + ',')
                self.emit('};')
            return None
        if op == 'return':
            value = self.expr(args[0])[0] if args else ''
            self.emit('return ' + (value if self.function_result else '') + ';')
            return None
        if op in ('local.get', 'global.get'):
            return self.temp(('self.' if op == 'global.get' else '') + ident(args[0]), (self.globals if op=='global.get' else self.locals)[args[0]])
        if op in ('local.set', 'global.set', 'local.tee'):
            val = self.expr(args[1])
            dest = ('self.' if op == 'global.set' else '') + ident(args[0])
            self.emit(f'{dest} = {val[0]};')
            return val if op == 'local.tee' else None
        if op in ('i32.const', 'i64.const'):
            typ = TYPES[op.split('.')[0]]
            return number(args[0], typ), typ
        if op == 'call':
            vals = [self.expr(a)[0] for a in args[1:]]
            return self.temp('self.' + ident(args[0]) + '(' + ', '.join(vals) + ')', self.results[args[0]])
        if op == 'call_indirect':
            vals = [self.expr(a)[0] for a in args if a[0] not in ('type','param','result')]
            return self.temp(f'self.dispatch({vals[-1]})', None)
        if op == 'drop':
            self.expr(args[0])
            return None
        if op == 'unreachable':
            self.emit('unreachable!("upstream unreachable");')
            return None
        if op == 'memory.size':
            return self.temp('(self.memory.len() / 65536) as u32', 'u32')
        vals = [self.expr(a) for a in args if isinstance(a, list)]
        v = [a[0] for a in vals]
        if op == 'select':
            return self.temp(f'if {v[2]} != 0 {{ {v[0]} }} else {{ {v[1]} }}', vals[0][1])
        if op in ('memory.copy','memory.fill','memory.grow'):
            return self.temp('self.' + {'memory.copy':'copy','memory.fill':'fill','memory.grow':'grow'}[op] + '(' + ', '.join(v) + ')', 'u32' if op == 'memory.grow' else None)
        base, action = op.split('.')
        typ = TYPES.get(base, 'V128')
        if action.startswith(('load', 'store')):
            offset = next((a.split('=')[1] for a in args if isinstance(a,str) and a.startswith('offset=')), '0')
            address = f'({v[0]} as usize + {offset})'
            return self.temp(f'self.{base}_{action}({address}' + (', ' + v[1] if action.startswith('store') else '') + ')', None if action.startswith('store') else typ)
        if base in ('v128','i8x16'):
            return self.temp('V128::' + action + '(' + ', '.join(v) + ')', 'u32' if action in ('bitmask','any_true') else 'V128')
        if action in ('add','sub','mul','shl'):
            expr = f'{v[0]}.wrapping_{action}({v[1]}' + (' as u32' if action == 'shl' else '') + ')'
        elif action == 'rotl':
            expr = f'{v[0]}.rotate_left({v[1]} as u32)'
        elif action in ('shr_u','shr_s'):
            expr = (f'({v[0]} as i{typ[1:]}).wrapping_shr({v[1]} as u32) as {typ}' if action=='shr_s' else f'{v[0]}.wrapping_shr({v[1]} as u32)')
        elif action in ('and','or','xor'):
            expr = f'({v[0]} ' + {'and':'&','or':'|','xor':'^'}[action] + f' {v[1]})'
        elif action in ('ctz','clz','popcnt'):
            expr = f'{v[0]}.' + {'ctz':'trailing_zeros','clz':'leading_zeros','popcnt':'count_ones'}[action] + f'() as {typ}'
        elif action in ('wrap_i64','extend_i32_u'):
            expr = f'{v[0]} as {typ}'
        elif action == 'eqz':
            expr, typ = f'({v[0]} == 0) as u32', 'u32'
        elif action.split('_')[0] in ('eq','ne','lt','le','gt','ge'):
            operator = {'eq':'==','ne':'!=','lt':'<','le':'<=','gt':'>','ge':'>='}[action.split('_')[0]]
            a, b = v
            if action.endswith('_s'):
                a, b = f'({a} as i{typ[1:]})', f'({b} as i{typ[1:]})'
            expr, typ = f'({a} {operator} {b}) as u32', 'u32'
        else:
            raise ValueError(op)
        return self.temp(expr, typ)
    def function(self, name, f):
        params = declarations(f, 'param')
        locals_ = declarations(f, 'local')
        for i, d in enumerate(params + locals_):
            if len(d) == 2:
                d.insert(1, str(i))
        self.locals = {d[1]: TYPES[d[2]] for d in params + locals_}
        self.lines, self.labels, self.counter = [], [], 0
        result = self.results[name]
        self.function_result = result
        self.emit('#[inline]')
        self.emit('pub(crate) fn ' + ident(name) + '(&mut self' + ''.join(f', mut {ident(p[1])}: {TYPES[p[2]]}' for p in params) + ')' + (f' -> {result}' if result else '') + ' {')
        if name == '$recLinesPost':
            self.emit('if self.raw_records { return; }')
        for d in locals_:
            self.emit(f'let mut {ident(d[1])}: {TYPES[d[2]]} = ' + ('V128::ZERO' if d[2]=='v128' else '0') + ';')
        body = [x for x in f[2:] if isinstance(x,list) and x[0] not in ('param','result','local','export')]
        val = self.seq(body)
        if result:
            self.emit(val[0] if val else 'unreachable!("validated WAT result")')
        self.emit('}')
        return '\n'.join(self.lines)

def main():
    wat, meta, root = map(Path, sys.argv[1:])
    module = parse(wat.read_text())
    t = Translator(module)
    out = ['// Translated from Pierre Highlights 0.4.0; regenerate with scripts/translate.py.', '// Copyright 2025 Pierre Computer Company; Apache-2.0. See LICENSE and NOTICE.md.', '#![allow(non_snake_case, unused_variables, unused_mut, unused_labels, unreachable_code, dead_code, unused_parens, unused_assignments, clippy::all)]', 'use crate::simd::V128;', 'pub(crate) struct Engine { pub(crate) memory: Vec<u8>, pub(crate) raw_records: bool,']
    out += [f'{ident(n)}: {typ},' for n, typ in t.globals.items()]
    out += ['}', 'impl Engine {', 'pub(crate) fn new() -> Self { let mut this = Self { memory: vec![0; 3*65536], raw_records: false,']
    for g in module[1:]:
        if g[0] == 'global':
            out.append(f'{ident(g[1])}: {number(g[3][1], t.globals[g[1]])},')
    out += ['}; this.memory[..81920].copy_from_slice(include_bytes!("initial-memory.bin")); this }', 'fn dispatch(&mut self, language: u32) { match language {']
    out += [f'{i} => self.{ident(n)}(),' for i,n in enumerate(t.table)]
    out += ['_ => unreachable!("invalid language"), } }']
    for name, f in t.funcs.items():
        if name in ('$is_id_start','$is_id_continue'):
            continue
        try:
            out.append(t.function(name, optimize_scan_masks(f)))
        except Exception:
            print('Failed in', name, file=sys.stderr)
            raise
    out += ['}']
    (root/'src/generated.rs').write_text('\n'.join(out)+'\n')
    memory = bytearray(81920)
    for d in module[1:]:
        if d[0] != 'data':
            continue
        offset = int(d[1][1],0)
        data = bytearray()
        for s in d[2:]:
            s=s[1:-1]
            while s:
                if s[0]=='\\':
                    data.append(int(s[1:3],16)); s=s[3:]
                else:
                    data.extend(s[0].encode()); s=s[1:]
        assert offset + len(data) <= len(memory)
        memory[offset:offset+len(data)] = data
    (root/'src/initial-memory.bin').write_bytes(memory)
    metadata=json.loads(meta.read_text())
    (root/'tests/data/aliases.json').write_text(json.dumps(metadata['languages'],indent=2)+'\n')
    catalog=['// Language and token identifiers from Pierre Highlights 0.4.0.']
    for name, enum in [('Language','$Language'),('TokenKind','$Token')]:
        entries=metadata['enums'][enum]
        count=len(entries)
        catalog += ['#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]', f'pub struct {name}(u8);',f'impl {name} {{', f'pub const ALL: [Self; {count}] = [' + ','.join(f'Self({i})' for i in range(count)) + '];', f'pub const fn from_id(id: u8) -> Option<Self> {{ if id < {count} {{ Some(Self(id)) }} else {{ None }} }}', 'pub const fn id(self) -> u8 { self.0 }', "pub const fn name(self) -> &'static str { match self.0 {"]
        catalog += [f'{i} => {json.dumps(n)},' for n,i in entries.items()]
        catalog += ['_ => unreachable!(), } }']
        if name=='Language':
            catalog += ['pub fn from_name(name: &str) -> Option<Self> { match name.to_ascii_lowercase().as_str() {']
            catalog += [f'{json.dumps(n)} => Some(Self({i})),' for n,i in metadata['languages'].items()]
            catalog += ['_ => None, } }']
        catalog += ['}']
    (root/'src/catalog.rs').write_text('\n'.join(catalog)+'\n')
    print(f'Translated {len(t.funcs)} functions, {len(t.table)} languages')

if __name__ == '__main__':
    main()
