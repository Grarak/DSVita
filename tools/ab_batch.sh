#!/bin/bash
R=~/ab_results.txt; : > "$R"
POKE="$HOME/nds/Pokemon - Black Version (USA, Europe) (NDSi Enhanced).nds"
COD="$HOME/nds/1617 - Call of Duty 4 - Modern Warfare (USA).nds"
CAS="$HOME/nds/0121 - Castlevania - Dawn of Sorrow (USA).nds"
for entry in "Pokemon:$POKE" "Castlevania:$CAS" "CoD4:$COD"; do
  label=${entry%%:*}; rom=${entry#*:}
  for i in 1 2 3; do
    b=$(bash ~/ab_measure.sh dsvita_a32touch "$rom")
    o=$(bash ~/ab_measure.sh dsvita_opt "$rom")
    echo "$label run$i base=$b opt=$o" >> "$R"
  done
done
echo "AB_DONE" >> "$R"
