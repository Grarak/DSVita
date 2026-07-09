#!/bin/bash
# batch_prof2.sh — corrected unified sweep (prof_one2.sh) over ~/prof_keep.txt.
# Run detached: nohup bash ~/batch_prof2.sh >/dev/null 2>&1 &
set -u
D=~/prof; mkdir -p "$D"
MASTER="$D/master.log"
: > "$MASTER"
TOTAL=$(grep -c . ~/prof_keep.txt)
echo "BATCH2_START total=$TOTAL $(date '+%H:%M:%S')" >> "$MASTER"
n=0
while IFS= read -r rom; do
  [ -z "$rom" ] && continue
  n=$((n+1))
  out="g$(printf '%03d' "$n")_$(echo "$rom" | sed 's/\.nds$//; s/[^A-Za-z0-9]/_/g' | cut -c1-40)"
  echo "[$n/$TOTAL] $(date '+%H:%M:%S') START $rom -> $out" >> "$MASTER"
  res=$(bash ~/prof_one2.sh "$HOME/nds/$rom" "$out" 16 25 2>&1)
  echo "[$n/$TOTAL] $res" >> "$MASTER"
done < ~/prof_keep.txt
echo "BATCH2_DONE n=$n $(date '+%H:%M:%S')" >> "$MASTER"
