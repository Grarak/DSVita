#!/bin/bash
# batch_audio.sh [rom_list] — re-profile WITH audio (-a) over a newline list of rom filenames
# (relative to $DSVITA_ROMS_DIR; default $HOME/prof_worst.txt).
set -u
. "$(dirname "$0")/env.sh"
require_env DSVITA_ROMS_DIR
LIST="${1:-$HOME/prof_worst.txt}"
D="$HOME/profA"; mkdir -p "$D"; MASTER="$D/master.log"; : > "$MASTER"
TOTAL=$(grep -c . "$LIST")
echo "AUDIO_START total=$TOTAL $(date '+%H:%M:%S')" >> "$MASTER"
n=0
while IFS= read -r rom; do
  [ -z "$rom" ] && continue
  n=$((n+1))
  out="a$(printf '%03d' "$n")_$(echo "$rom" | sed 's/\.nds$//; s/[^A-Za-z0-9]/_/g' | cut -c1-40)"
  echo "[$n/$TOTAL] $(date '+%H:%M:%S') START $rom -> $out" >> "$MASTER"
  res=$(bash "$(dirname "$0")/prof_one2_audio.sh" "$DSVITA_ROMS_DIR/$rom" "$out" 16 25 2>&1)
  echo "[$n/$TOTAL] $res" >> "$MASTER"
done < "$LIST"
echo "AUDIO_DONE n=$n $(date '+%H:%M:%S')" >> "$MASTER"
