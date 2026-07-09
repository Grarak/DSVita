#!/bin/bash
# batch_audio.sh — re-profile worst performers WITH audio (-a). Reads ~/prof_worst.txt (rom names).
set -u
D=~/profA; mkdir -p "$D"; MASTER="$D/master.log"; : > "$MASTER"
TOTAL=$(grep -c . ~/prof_worst.txt)
echo "AUDIO_START total=$TOTAL $(date '+%H:%M:%S')" >> "$MASTER"
n=0
while IFS= read -r rom; do
  [ -z "$rom" ] && continue
  n=$((n+1))
  out="a$(printf '%03d' "$n")_$(echo "$rom" | sed 's/\.nds$//; s/[^A-Za-z0-9]/_/g' | cut -c1-40)"
  echo "[$n/$TOTAL] $(date '+%H:%M:%S') START $rom -> $out" >> "$MASTER"
  res=$(bash ~/prof_one2_audio.sh "$HOME/nds/$rom" "$out" 16 25 2>&1)
  echo "[$n/$TOTAL] $res" >> "$MASTER"
done < ~/prof_worst.txt
echo "AUDIO_DONE n=$n $(date '+%H:%M:%S')" >> "$MASTER"
