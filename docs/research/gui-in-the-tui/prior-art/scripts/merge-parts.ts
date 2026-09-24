/**
 * Assemble one round's GatherResult from the per-cluster part files.
 *
 *   bun run scripts/merge-parts.ts <round>
 *
 * Round 1 was gathered by several agents in parallel (one per cluster, plus the
 * lead for cluster E, the security check and the three claim verifications).
 * Each wrote run/round-<N>.part-<X>.json in the GatherResult shape. This script
 * concatenates them into run/round-<N>.result.json — the file ManualGatherer
 * ingests. It does not resolve conflicts itself: duplicate subject ids are left
 * as separate updates so research-delight's own non-destructive merge
 * (verified > flagged > unverified, never clobber) decides, and a duplicate is
 * reported here so a human can see it happened.
 */
import { readdirSync } from "node:fs";

const round = Number(process.argv[2] ?? "1");
const dir = `${import.meta.dir}/../run`;
const parts = readdirSync(dir)
  .filter((f) => f.startsWith(`round-${round}.part-`) && f.endsWith(".json"))
  .sort();
if (!parts.length) throw new Error(`no part files for round ${round} in ${dir}`);

const SOURCE_CHECKED = new Set(["what_it_is", "license", "language", "graphics_mechanism", "where_it_draws", "input_model", "activity", "primary_sources"]);

type F = { value: unknown; provenance?: { source?: string }; flagged?: boolean };
const updates: any[] = [];
const bubbles: any[] = [];
let tokens = 0;
const seen = new Map<string, string>();
const problems: string[] = [];

for (const p of parts) {
  const res = JSON.parse(await Bun.file(`${dir}/${p}`).text());
  for (const u of res.updates ?? []) {
    if (seen.has(u.id)) problems.push(`duplicate subject ${u.id} in ${p} (also in ${seen.get(u.id)}) — harness merge decides`);
    else seen.set(u.id, p);
    for (const [k, f] of Object.entries(u.fields ?? {}) as [string, F][]) {
      const empty = f.value === null || f.value === undefined || (typeof f.value === "string" && !f.value.trim());
      if (empty) problems.push(`${p} ${u.id}.${k}: empty value`);
      if (SOURCE_CHECKED.has(k) && !f.provenance?.source && !f.flagged) problems.push(`${p} ${u.id}.${k}: unsourced and unflagged — the harness will count it unverified`);
    }
    updates.push(u);
  }
  bubbles.push(...(res.bubbles ?? []));
  tokens += Number(res.tokens ?? 0);
  console.log(`${p}: ${res.updates?.length ?? 0} subjects, ${res.bubbles?.length ?? 0} bubbles, ${res.tokens ?? 0} tok`);
}

await Bun.write(`${dir}/round-${round}.result.json`, JSON.stringify({ updates, bubbles, tokens }, null, 2));
console.log(`→ run/round-${round}.result.json: ${updates.length} updates (${seen.size} subjects), ${bubbles.length} bubbles, ${tokens} tok`);
for (const x of problems) console.log(`  ! ${x}`);
