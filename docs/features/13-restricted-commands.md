# Restricted commands

Development feature for 0.5.0. An explicit offline launch gives a command one project directory, read-only system tools, temporary home/tmp and plain-text streams. Standard terminals retain ordinary account authority.

```sh
terminal-delight restricted run --workdir "$HOME/Projects/example" -- sh -c 'make test'
```

The canonical directory must be strictly below the OS account home, with no dot-directory in the selected path. Hidden content already inside a project is granted. System trees, external volumes and hidden roots are refused; copy work into a dedicated project first. Descriptor-relative validation rejects a changed root, nested mounts, special files and hard links. Symlinks do not expose unmounted host paths.

Requires bubblewrap 0.12.0+, Linux 5.11+, native x86_64/aarch64 and working namespaces/seccomp. Unsupported setup refuses launch. This profile supports offline, line-oriented commands; online agents, TUIs, GUI services and automatic delegation remain unavailable.

The optional `--td-note` grant permits only bounded own-pane notes, subject to MCP policy. One access proposal per run appears in the run terminal and status. Outside-only `restricted dismiss RUN` clears it without granting access or resetting its lifetime budget. `restricted revoke RUN` stops future TD effects; it does not stop file work. A changed scope requires an explicit new run.

Secrets supplied through the project, arguments or input are granted content. Output can influence an unrestricted consumer. Complete resource quotas, hostile host users and kernel exploits are outside the guarantee.

The [user page](../../docsite/pages/restricted.html) carries commands and requirements. Implementation: `app/src/restricted.rs`, `restricted/work.rs`, `restricted/seccomp.rs`; real OS probes: `app/tests/restricted.rs`.
