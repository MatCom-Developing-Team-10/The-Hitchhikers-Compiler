#!/usr/bin/env bash
# Run HULK integration tests and report pass/fail by section.
#
# Each test exercises the FULL matcom/compilers pipeline, exactly as the course
# harness does:
#   1. `./hulk program.hulk`  — compile/validate, emitting `./output`.
#   2. `./output`             — execute, producing the program's stdout.
# The captured stdout is then compared against a committed `<name>.expected`
# file. A test passes only if compilation succeeds AND the output matches.
# (The old runner only checked the exit code of `hulk run`, which is
# compile-only — it never executed the program nor verified its output.)
#
# Usage:
#   bash tests/run_tests.sh                  # uses 'cargo run -p hulkc'
#   HULKC=./target/release/hulkc bash tests/run_tests.sh  # uses pre-built binary
#   UPDATE_EXPECTED=1 bash tests/run_tests.sh             # regenerate .expected
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

# Compilation emits `./output` in the current directory, and `./output`
# re-invokes the compiler in exec mode, so run everything from the repo root.
cd "$REPO_ROOT"

total_pass=0
total_fail=0
std_fail=0

# Run a single .hulk file through compile + execute and compare stdout against
# its committed `.expected` file. Prints PASS/FAIL and updates counters.
# Returns 0 on pass, 1 on fail.
run_file() {
    local file="$1"
    local name expected got rc cerr rerr
    name="$(basename "$file")"
    expected="${file%.hulk}.expected"

    cerr="$(mktemp)"
    rerr="$(mktemp)"

    # Step 1: compile/validate. Emits ./output on success.
    if ! "${HULKC_CMD[@]}" "$file" >/dev/null 2>"$cerr"; then
        printf "  %-44s FAIL (compile)\n" "$name"
        sed 's/^/      /' "$cerr"
        rm -f "$cerr" "$rerr"
        ((total_fail++)) || true
        return 1
    fi

    # Step 2: execute the emitted program, capturing stdout.
    got="$("$REPO_ROOT/output" 2>"$rerr")"
    rc=$?
    if [ "$rc" -ne 0 ]; then
        printf "  %-44s FAIL (runtime rc=%d)\n" "$name" "$rc"
        sed 's/^/      /' "$rerr"
        rm -f "$cerr" "$rerr"
        ((total_fail++)) || true
        return 1
    fi
    rm -f "$cerr" "$rerr"

    # Optionally (re)generate the golden file instead of comparing.
    if [ "${UPDATE_EXPECTED:-0}" = "1" ]; then
        printf '%s\n' "$got" >"$expected"
        printf "  %-44s UPDATED\n" "$name"
        ((total_pass++)) || true
        return 0
    fi

    if [ ! -f "$expected" ]; then
        printf "  %-44s FAIL (no .expected — run with UPDATE_EXPECTED=1)\n" "$name"
        ((total_fail++)) || true
        return 1
    fi

    # Step 3: compare actual output against the golden file.
    if diff -u "$expected" <(printf '%s\n' "$got") >/dev/null 2>&1; then
        printf "  %-44s PASS\n" "$name"
        ((total_pass++)) || true
        return 0
    else
        printf "  %-44s FAIL (output mismatch)\n" "$name"
        diff -u "$expected" <(printf '%s\n' "$got") | sed 's/^/      /'
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

# Clean up the generated executable so it doesn't linger in the working tree.
rm -f "$REPO_ROOT/output"

echo ""
echo "========================================"
echo "Total: $total_pass passed, $total_fail failed"

if [ "$std_fail" -gt 0 ]; then
    echo "RESULT: FAILED — $std_fail hulk_std test(s) did not pass."
    exit 1
fi

echo "RESULT: OK — all standard tests passed."
exit 0
