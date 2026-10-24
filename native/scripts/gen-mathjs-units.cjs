const {create,all}=require(require('path').join(__dirname, '../../node_modules/mathjs'));
const math=create(all,{number:'number'});
const P=math.Unit.PREFIXES, U=math.Unit.UNITS;
const setName=(p)=>Object.keys(P).find(k=>P[k]===p);
const f=(x0)=>{ const x=typeof x0==='number'?x0:(x0&&typeof x0.valueOf()==='number'?x0.valueOf():NaN); if(Number.isNaN(x)) return 'f64::NAN'; if(x===0) return '0.0'; const s=String(x); return /[.e]/.test(s)?s:s+'.0'};
let out='';
out+='//! Generated from mathjs 15.2.0 (`math.Unit.PREFIXES` and `math.Unit.UNITS` in number mode); do not edit by hand.\n//! Regenerate: `node native/scripts/gen-mathjs-units.cjs > native/core/src/units/mathjs_table.rs`.\n\n';
out+='#![allow(clippy::unreadable_literal, clippy::excessive_precision, clippy::approx_constant)]\n\n';
out+='/// Prefix sets by name: each prefix and its factor.\n';
out+='#[rustfmt::skip]\n';
out+='pub const PREFIX_SETS: &[(&str, &[(&str, f64)])] = &[\n';
for (const k of Object.keys(P)) out+=`    ("${k}", &[${Object.entries(P[k]).map(([n,p])=>`("${n}", ${f(p.value)})`).join(', ')}]),\n`;
out+='];\n\n/// (name, value, offset, dimensions, prefix set), in mathjs\'s own order.\n';
out+='#[rustfmt::skip]\n';
out+='pub const UNITS: &[(&str, f64, f64, [f64; 9], &str)] = &[\n';
for (const n of Object.keys(U)) { const u=U[n]; out+=`    ("${n}", ${f(u.value)}, ${f(u.offset)}, [${u.dimensions.map(f).join(', ')}], "${setName(u.prefixes)}"),\n`; }
out+='];\n';
process.stdout.write(out);
