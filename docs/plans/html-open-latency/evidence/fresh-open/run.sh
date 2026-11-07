#!/bin/bash
# run.sh <tag> <round>: alternating native fresh-open runs, old vs new, per brief
C=/home/parker/Work/td-fresh; R=$C/native; S=/tmp/claude-1000/-home-parker-Work-terminal-delight/88a0c77e-35f4-447c-b33a-b2b5feb7f8f9/scratchpad
declare -A BIN=([old]=/home/parker/.local/lib/terminal-delight/td-f32edeb3-wave [new]=$C/bins/${NEW:-stage2})
for round in $(seq 1 ${ROUNDS:-2}); do for brief in a-fresh b-built; do for v in old new; do
  mkdir -p $R/$1; out=$R/$1/$v-$brief-r$round; rm -rf $out
  PROFILE_FRESH_FILES=1 PROFILE_BETWEEN=0.6 timeout 300 python3 $R/native.py --bin ${BIN[$v]} --brief $S/corpus/$brief.html --out $out --cycles ${CYCLES:-8} --mode reader > $out.log 2>&1 || echo "FAILED $out: $(tail -3 $out.log)"
done; done; done
