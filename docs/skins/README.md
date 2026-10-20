# Author a Terminal Delight skin

The `td-skin` skill guides an agent from a reference to a validated TOML file and a native review. The running TD binary supplies the contract, inventory and capture runner. A skill does not carry a second inventory that can drift behind the app.

```sh
terminal-delight skin --list
terminal-delight skin --template chalk > personal-skin.toml
terminal-delight skin --catalogue > skin-surfaces.json
terminal-delight skin --catalogue-md > skin-surfaces.md
terminal-delight skin --atlas-script > skin-atlas.py
terminal-delight skin --check --skin personal-skin.toml --theme hacker
python3 skin-atlas.py --binary /path/to/terminal-delight --skin personal-skin.toml --out review/usage --scene usage-choices --verify-reload
```

`--template default` documents the optional metrics. Bundled templates demonstrate the finite strategies. [Field Notes](examples/field-notes.toml) combines existing strategies into a second stock material look without changing Rust. It uses a monospace UI, restrained grain and lift, plain notes and a glass terminal tube. Chalk keeps its handwriting, flower and stock tube.

## Boundaries

Palette supplies colours. Design supplies terminal type and CRT effects. Grade supplies the current viewing adjustments. Skin supplies native furniture, UI families, optical fitting and material. A paper material does not imply a handwriting correction. Older Caveat files retain their previous fitting fallback; new authors should declare optical settings explicitly.

The public `Skin` vocabulary includes regions, choices, actions, captions, meters, scrubbers and frames. Application renderers retain handlers, ids, semantics and hit targets. They may not branch on the paper material or import its backend. Stateless palette/switch parts live under `skin/parts.rs`; material dispatch and painting stay in the skin subtree and its backend.

An existing effect can be composed through data. A new silhouette, shader or asset-loading strategy needs a shared renderer contribution. The TOML is declarative and never executes a plugin. Terminal cells, document bodies, media and semantic state colours retain their own meaning; their native controls still follow the skin.

## Inventory and Proof

Edit `app/skin-surfaces.json`, then regenerate `docs/spec/skin-surface-inventory.md` with the binary. Each stable entry declares its owner, scope, material, boundary, emphasis, typography, colour policy and optional fixture. Owner/fixture tests catch stale references. Mixed facets remain mixed until their owner is migrated. This map describes source routing; it is not a visual coverage percentage.

The native runner launches only a new private session. The default capture backend requires Hyprland 0.56 and grim; `--visible --hold` provides a private window on another desktop. `--capture` takes a JSON argv array for a native capture utility. No normal window, config or session host is adopted.

A receipt records the build, skin and runner hashes; requested and observed geometry; requested palette and grade; live resolved tokens; capture hash; and optional reload or idle sample. `visual_verdict` and `states_reviewed` start absent. Inspect the pictures before filling those fields. Missing fixtures, untested interaction states and stale captures remain unverified.

Review dark and pale palettes, narrow geometry, intended scale and CRT grade, a non-English label set, zero and unavailable readings, document controls, workbench and sticky notes. Hover and disabled states require their own interaction review. A static selected-state capture does not certify them.

## Apply and Restore

Identify the target window before changing it. Record `ctl --pid <window-pid> skin status` and `skin snapshot`, the custom-file path, its original bytes or absence, and the previous explicit/theme-following selection. `TD_SKIN` overrides the normal file location and wins over a saved selection on startup. The runner uses its private standard config path when testing persistence.

Back up the previous file, write the validated file and select `custom` on that window. The 300 ms watcher keeps the previous valid skin when an edit cannot parse. `skin snapshot` reads the live registry rather than parsing the saved file again. A valid edit, invalid edit and exact restoration can be exercised safely with the runner's `--verify-reload` flag.

Restore the previous file bytes (or remove the newly created file), then restore the recorded selection. `skin theme` returns to following the design. A local skin change needs no restart of a normal TD session.
