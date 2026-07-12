#!/bin/bash
# ab_batch.sh [base_bin] [opt_bin] [rom_list] — interleaved A/B fps of two builds (both dropped in
# $HOME on the test box, referenced by name) over a newline list of rom filenames (relative to
# $DSVITA_ROMS_DIR; default $HOME/ab_roms.txt). 3 interleaved runs each = thermal-drift-resistant.
set -u
. "$(dirname "$0")/env.sh"
require_env DSVITA_ROMS_DIR
BASE="${1:-dsvita_base}"; OPT="${2:-dsvita_opt}"; LIST="${3:-$HOME/ab_roms.txt}"
R="$HOME/ab_results.txt"; : > "$R"
MEASURE="$(dirname "$0")/ab_measure.sh"
while IFS= read -r rom; do
  [ -z "$rom" ] && continue
  label=$(echo "$rom" | sed 's/\.nds$//' | cut -c1-20)
  for i in 1 2 3; do
    b=$(bash "$MEASURE" "$BASE" "$DSVITA_ROMS_DIR/$rom")
    o=$(bash "$MEASURE" "$OPT" "$DSVITA_ROMS_DIR/$rom")
    echo "$label run$i base=$b opt=$o" >> "$R"
  done
done < "$LIST"
echo "AB_DONE" >> "$R"
