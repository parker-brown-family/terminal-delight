/**
 * Assemble one round's GatherResult from the per-cluster part files.
 *
 *   bun run docs/research/terminal-core/scripts/merge-parts.ts <round>
 *
 * Round 1 was gathered by six agents in parallel, one per cluster part. Each wrote
 * run/round-<N>.part-<X>.json in the GatherResult shape. This concatenates them
 * into run/round-<N>.result.json, the file ManualGatherer ingests. It resolves no
 * conflicts itself: duplicate subject ids stay separate updates so research-delight's
 * non-destructive merge decides, and each duplicate is reported. It also reports
 * every sourced value whose note breaks the quote rule, because claim-check can only
 * check a value that carries its quote.
 */
import { readdirSync } from "node:fs";
import { rubric } from "../terminal-core.plan.ts";

const round = Number(process.argv[2] ?? "1");
const dir = `${import.meta.dir}/../run`;
const parts = readdirSync(dir)
  .filter((f) => f.startsWith(`round-${round}.part-`) && f.endsWith(".json"))
  .sort();
if (!parts.length) throw new Error(`no part files for round ${round} in ${dir}`);

const SOURCE_CHECKED = new Set(rubric.fields.filter((f) => f.verify && f.verify !== "none" && f.requirement !== "derived").map((f) => f.key));

type F = { value: unknown; provenance?: { source?: string }; flagged?: boolean; note?: string };
const updates: any[] = [];
const bubbles: any[] = [];
let tokens = 0;
const seen = new Map<string, string>();
const problems: string[] = [];
let quoted = 0, needQuote = 0;

for (const p of parts) {
  const res = JSON.parse(await Bun.file(`${dir}/${p}`).text());
  for (const u of res.updates ?? []) {
    if (seen.has(u.id)) problems.push(`duplicate subject ${u.id} in ${p} (also in ${seen.get(u.id)}) — harness merge decides`);
    else seen.set(u.id, p);
    for (const [k, f] of Object.entries(u.fields ?? {}) as [string, F][]) {
      const empty = f.value === null || f.value === undefined || (typeof f.value === "string" && !f.value.trim());
      if (empty) problems.push(`${p} ${u.id}.${k}: empty value`);
      if (SOURCE_CHECKED.has(k) && !f.provenance?.source && !f.flagged) problems.push(`${p} ${u.id}.${k}: unsourced and unflagged — the harness will count it unverified`);
      if (f.provenance?.source) {
        needQuote++;
        if (/^\s*quote:\s*"/.test(f.note ?? "")) quoted++;
        else problems.push(`${p} ${u.id}.${k}: sourced but its note does not open with quote: "…"`);
      }
    }
    updates.push(u);
  }
  bubbles.push(...(res.bubbles ?? []));
  tokens += Number(res.tokens ?? 0);
  console.log(`${p}: ${res.updates?.length ?? 0} subjects, ${res.bubbles?.length ?? 0} bubbles, ${res.tokens ?? 0} tok`);
}

await Bun.write(`${dir}/round-${round}.result.json`, JSON.stringify({ updates, bubbles, tokens }, null, 2));
console.log(`→ run/round-${round}.result.json: ${updates.length} updates (${seen.size} subjects), ${bubbles.length} bubbles, ${tokens} tok`);
console.log(`  quote rule: ${quoted} of ${needQuote} sourced values open with a quote`);
for (const x of problems) console.log(`  ! ${x}`);
