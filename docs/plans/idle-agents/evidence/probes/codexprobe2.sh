#!/usr/bin/env bash
# Does Codex's shared app-server daemon spawn a per-thread MCP set for a new TUI, and reap it when the TUI exits?
D=3807336
kids(){ pgrep -P $D | sort; }
k0=$(kids); echo "daemon=$D kids_before=$(echo "$k0"|wc -l)"
tmux -L codexprobe -f /dev/null new-session -d -s c -x 160 -y 40 "cd /tmp && exec env -u TD_PANE_ID -u TD_SESSION -u TD_TAG /home/parker/.local/share/mise/installs/codex/latest/bin/codex"
sleep 25
k1=$(kids); new=$(comm -13 <(echo "$k0") <(echo "$k1")); echo "after_tui_start kids=$(echo "$k1"|wc -l) new: $(for p in $new; do printf '%s:%s ' $p $(cat /proc/$p/comm); done)"
tmux -L codexprobe capture-pane -p -t c | grep -v '^\s*$' | tail -6
TUI=$(pgrep -P $(tmux -L codexprobe list-panes -F '#{pane_pid}' -t c) 2>/dev/null | head -1); [ -z "$TUI" ] && TUI=$(tmux -L codexprobe list-panes -F '#{pane_pid}' -t c)
echo "tui=$TUI comm=$(cat /proc/$TUI/comm)"
kill -TERM $TUI; for w in 30 60 90 120 180; do sleep $(( w==30?30:w==180?60:30 )); echo "${w}s_after_exit alive_from_new: $(for p in $new; do kill -0 $p 2>/dev/null && printf "%s:%s " $p $(cat /proc/$p/comm); done)"; done; sleep 0
tmux -L codexprobe kill-server 2>/dev/null
