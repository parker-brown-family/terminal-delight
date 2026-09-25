/**
 * Rebuild the final dossiers from every round's result file, with research-delight's
 * own non-destructive merge, and write them where the Jev passes and the page read them.
 *
 *   bun run docs/research/terminal-core/scripts/dossiers.ts
 *   → docs/research/terminal-core/jev/dossiers.json
 *
 * research-delight's run.json keeps rounds, scores and bubbles but not the dossiers,
 * so this replays the rounds rather than re-implementing the merge: a later weak value
 * never overwrites a cited one, exactly as the harness decided.
 */
import { readdirSync } from "node:fs";
import { applyUpdates } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/merge.ts";
import { classify } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/provenance.ts";
import plan, { rubric, subjects, applyDerived } from "../terminal-core.plan.ts";

const run = `${import.meta.dir}/../run`;
const rounds = readdirSync(run)
  .map((f) => /^round-(\d+)\.result\.json$/.exec(f)?.[1])
  .filter(Boolean)
  .map(Number)
  .sort((a, b) => a - b);

let dossiers = subjects.map((s) => ({ id: s.id, subjectType: rubric.subjectType, label: s.label, group: s.group, fields: {} }));
const bubbles: any[] = [];
for (const r of rounds) {
  const res = JSON.parse(await Bun.file(`${run}/round-${r}.result.json`).text());
  dossiers = applyUpdates(dossiers as any, res.updates ?? [], rubric.subjectType) as any;
  bubbles.push(...(res.bubbles ?? []));
}
dossiers = applyDerived(dossiers as any) as any;
for (const d of dossiers as any[]) for (const f of Object.values(d.fields) as any[]) f.status = classify(f);

await Bun.write(`${import.meta.dir}/../jev/dossiers.json`, JSON.stringify({ rounds, dossiers, bubbles }, null, 2));
console.log(`rounds ${rounds.join(", ") || "none"} → ${dossiers.length} dossiers, ${bubbles.length} bubbles → jev/dossiers.json`);
void plan;
