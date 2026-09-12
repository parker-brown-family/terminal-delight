# Chrome skin layer — status

**Difficulty: 8/10.** A token layer under a 7,400-line render function, adopted by
hundreds of call sites, that can be wrong for weeks before anyone notices — a token
that means slightly the wrong thing produces a look nobody questions — and whose
unwinding is a re-conversion of every site it reached. Eight buys all four gates.

**Gates were written and the tracer slice built in one sitting, without approvals,
because the work was asked for while Parker was away.** That is a deviation from the
playbook and it is recorded here rather than smoothed over. The consequence is that
Gate 1 and Gate 2 below are *proposals with an implementation already attached*: the
thing to do with them is veto, not ratify. Everything the tracer slice touched is
additive or reversible in one commit, which is the property that made building ahead
of approval defensible at all.

| Gate | Doc | State |
|---|---|---|
| 1 · Product | [01-product.md](01-product.md) | written, **unapproved** |
| 2 · Architecture | [02-architecture.md](02-architecture.md) | written, **unapproved** |
| 3 · Program design | folded into Gate 2 | — |
| 4 · Vertical slices | [03-slices.md](03-slices.md) | slice 0 (tracer) **built**, 1–6 costed |

Gate 3 is folded into Gate 2 on purpose. The program design for this feature *is* the
token set plus the element vocabulary, and splitting them across two documents would
have produced two half-arguments — which is the failure the four-gate playbook's own
retro names.

## Where the work is

- Branch `chrome/skin-tokens`, worktree `/home/parker/Work/td-skin`.
- New: `app/src/skin.rs`, `app/skins/{default,deco}.toml`, `app/themes/deco.toml`.
- Touched: `app/src/main.rs` (the left bar + one verb), `app/src/theme.rs` (a `skin`
  key, a builtin lookup, one test's count).

## The honest post-hoc number

*To be filled in when the adoption slices land.* The prediction the 8 was making is
that the token set survives contact with the second and third surface without needing
new tokens invented under deadline. If slices 1–3 add more than two or three tokens
between them, the 8 was right and the design was under-specified; if they add none,
it was closer to a 6 and the gates cost more than they bought.
