#!/usr/bin/env bash
# Run HULK integration tests and report pass/fail by section.
#
# Usage:
#   bash tests/run_tests.sh                  # uses 'cargo run -p hulkc'
#   HULKC=./target/debug/hulkc bash tests/run_tests.sh  # uses pre-built binary
#
# Exit code: 0 if all hulk_std tests pass; non-zero otherwise.
# Extension test failures are reported but do not affect the exit code.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Build the command array: prefer HULKC env var (path to binary), else cargo run.
if [ -n "${HULKC:-}" ]; then
    HULKC_CMD=("$HULKC")
else
    HULKC_CMD=(cargo run --manifest-path "$REPO_ROOT/Cargo.toml" -p hulkc --quiet --)
fi

total_pass=0
total_fail=0
std_fail=0

# Run a single .hulk file. Prints PASS/FAIL and updates counters.
# Returns 0 on pass, 1 on fail.
run_file() {
    local file="$1"
    local name
    name="$(basename "$file")"

    if "${HULKC_CMD[@]}" run "$file" >/dev/null 2>&1; then
        printf "  %-44s PASS\n" "$name"
        ((total_pass++)) || true
        return 0
    else
        printf "  %-44s FAIL\n" "$name"
        ((total_fail++)) || true
        return 1
    fi
}

# Run all .hulk files in a section directory.
# $1 = section name (subdirectory under tests/)
# $2 = "critical" (true/false) — if true, failures increment std_fail
run_section() {
    local section="$1"
    local critical="${2:-true}"
    local dir="$SCRIPT_DIR/$section"
    local sec_pass=0
    local sec_fail=0

    echo ""
    echo "=== $section ==="

    if [ ! -d "$dir" ]; then
        echo "  (directory not found: $dir)"
        return
    fi

    local found=0
    for f in "$dir"/*.hulk; do
        [ -f "$f" ] || continue
        found=1
        if run_file "$f"; then
            ((sec_pass++)) || true
        else
            ((sec_fail++)) || true
            if [ "$critical" = "true" ]; then
                ((std_fail++)) || true
            fi
        fi
    done

    if [ "$found" -eq 0 ]; then
        echo "  (no .hulk files found)"
    else
        echo "  ----------------------------------------"
        echo "  $sec_pass passed, $sec_fail failed"
    fi
}

echo "HULK Integration Test Runner"
echo "========================================"

run_section "hulk_std"  "true"
run_section "extension" "false"

echo ""
echo "========================================"
echo "Total: $total_pass passed, $total_fail failed"

if [ "$std_fail" -gt 0 ]; then
    echo "RESULT: FAILED — $std_fail hulk_std test(s) did not pass."
    exit 1
fi

echo "RESULT: OK — all standard tests passed."
exit 0
