#!/bin/bash
# Re-run every equivalence script: graft vs body, in-process and with a snapshot after each cell.
IMG=$1; out=/tmp/eq2; fail=0
for f in /tmp/eq/*.py; do
  b=$(basename $f .py)
  mods=$(grep -oE '\b(json|re|collections|asyncio)\b' $f | sort -u | tr '\n' ' ')
  for mode in dump snap; do
    $IMG $mode $f $mods >$out/$b.$mode.img 2>$out/$b.$mode.img.err
    MM_NO_IMAGES=1 $IMG $mode $f $mods >$out/$b.$mode.body 2>$out/$b.$mode.body.err
    if cmp -s $out/$b.$mode.img $out/$b.$mode.body; then r=same; else r=DIFF; fail=1; fi
    echo "$b $mode $r lines=$(wc -l <$out/$b.$mode.img) $(cat $out/$b.$mode.img.err | tr '\n' ' ') | body: $(cat $out/$b.$mode.body.err | tr '\n' ' ')"
  done
done
exit $fail
