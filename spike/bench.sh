#!/usr/bin/env bash
# Phase 00 benchmark: wall-clock + peak VRAM for one spike run.
#
#   ./bench.sh <label> <audio> [extra spike args...]
#
# Peak VRAM has to be sampled while the process runs; nvidia-smi is polled in the
# background and the maximum per-process figure is taken. Reports the model's own
# footprint, not total board usage, so a browser on the same GPU does not pollute it.

set -u

LABEL="${1:?usage: bench.sh <label> <audio> [args...]}"
AUDIO="${2:?usage: bench.sh <label> <audio> [args...]}"
shift 2

MODEL="${SILLAGE_MODEL:-D:/Documents/MANTARA/AI COMPAGNON APP/models/whisper/ggml-large-v3-turbo.bin}"
OUT="results-${LABEL}.json"
LOG="run-${LABEL}.log"
VRAM="vram-${LABEL}.txt"

if [ ! -x ./target/release/spike.exe ]; then
  echo "spike.exe not built" >&2
  exit 1
fi

nvidia-smi --query-compute-apps=used_memory --format=csv,noheader,nounits -l 1 > "$VRAM" 2>/dev/null &
SMI=$!

START=$(date +%s%3N)
./target/release/spike.exe --model "$MODEL" --audio "$AUDIO" --out "$OUT" "$@" > "$LOG" 2>&1
RC=$?
END=$(date +%s%3N)

kill "$SMI" 2>/dev/null
wait "$SMI" 2>/dev/null

PEAK=$(grep -E '^[0-9]+$' "$VRAM" 2>/dev/null | sort -rn | head -1)
PEAK=${PEAK:-0}

echo "=== $LABEL ==="
echo "exit            : $RC"
echo "wall clock      : $(( END - START )) ms"
echo "peak vram       : ${PEAK} MB"
grep -E "stream calls|segments |words |coverage|monotonic|drift|speed" "$LOG" || true
[ $RC -ne 0 ] && { echo "--- failure tail ---"; tail -15 "$LOG"; }
exit $RC
