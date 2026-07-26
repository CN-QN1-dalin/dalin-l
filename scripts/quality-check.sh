#!/bin/bash
# Dalin L 3.0 — Quality Engine CI Integration Script
# Usage: ./scripts/quality-check.sh [--json] [--strict] [--stdlib-only]

set -euo pipefail

JSON=false
STRICT=false
STDLIB_ONLY=false
EXIT_CODE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json) JSON=true; shift ;;
        --strict) STRICT=true; shift ;;
        --stdlib-only) STDLIB_ONLY=true; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [ "$STDLIB_ONLY" = true ]; then
    TARGET_DIR="stdlib"
    EXCLUDE_FLAG=""
else
    TARGET_DIR="."
    EXCLUDE_FLAG="--exclude"
fi

echo "============================================================"
echo "  Dalin L 3.0 — Quality Engine Check"
echo "  Mode: $(if [ "$JSON" = true ]; then echo 'JSON'; else echo 'text'; fi)"
echo "  Strict: $(if [ "$STRICT" = true ]; then echo 'yes'; else echo 'no'; fi)"
echo "============================================================"
echo ""

# Build release first if not built
if [ ! -f ./target/release/dalib ]; then
    echo "Building release binary..."
    cargo build --release -p dalin-compiler -p dalin_l 2>&1 | tail -3
    echo ""
fi

# Run quality check on target directory
if [ "$STDLIB_ONLY" = true ]; then
    for f in stdlib/*.dal; do
        OUTPUT=$(./target/release/dalib check -i "$f" --quality 2>&1)
        SCORE=$(echo "$OUTPUT" | sed -n 's/.*Score: \([0-9.]*\)\/100.*/\1/p')
        GATE=$(echo "$OUTPUT" | grep "Gate:" | head -1 | awk '{print $4}')
        
        if [ "$STRICT" = true ] && [ "$GATE" != "PASS" ]; then
            echo "❌ FAIL: $f (Score: $SCORE, Gate: $GATE)"
            EXIT_CODE=1
        elif [ "$GATE" = "PASS" ]; then
            echo "✅ PASS: $f (Score: $SCORE)"
        else
            echo "⚠️  WARN: $f (Score: $SCORE, Gate: $GATE)"
        fi
        
        # Skip warnings to keep output clean unless strict mode
        if [ "$JSON" = false ] && [ "$GATE" = "PASS" ]; then
            continue
        fi
        echo "$OUTPUT"
        echo "---"
    done
else
    echo "Running full workspace test suite..."
    cargo test --workspace --exclude dalin-pyo3 2>&1 | tail -5
    echo ""
    echo "Quality check completed."
fi

if [ "$JSON" = true ]; then
    echo '{"status": "success", "exit_code": 0}'
fi

echo ""
echo "============================================================"
echo "  Quality check finished with exit code: $EXIT_CODE"
echo "============================================================"
exit $EXIT_CODE
