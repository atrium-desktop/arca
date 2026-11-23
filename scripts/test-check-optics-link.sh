#!/bin/sh
# Self-test for scripts/check-optics-link.sh.
#
# The gate is the only thing standing between a misconfigured link and a
# shipped artifact, so its own failure modes are tested: it must reject a binary
# with no Optics code (a false "ok" would defeat the gate), must reject a fully
# stripped binary unless the weaker evidence is accepted explicitly, and must
# reject bad arguments instead of guessing.
#
# The passing cases need a real Arca binary, so they run only when one exists:
# `cargo build --release -p arca` first, then this script.
#
# Usage: scripts/test-check-optics-link.sh [path-to-built-arca]
set -eu

gate=$(dirname "$0")/check-optics-link.sh
binary=${1:-target/release/arca}
failures=0
tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/arca-link-gate.XXXXXX")
trap 'rm -rf "$tmpdir"' EXIT INT TERM

pass() { printf 'ok: %s\n' "$1"; }
fail() {
    printf 'FAIL: %s\n' "$1" >&2
    failures=$((failures + 1))
}

# Runs the gate and asserts its exit status. `expect_status` is 0 or 1; the
# argument-parsing cases use 2.
assert_status() {
    label=$1
    expected=$2
    shift 2
    if "$@" >"$tmpdir/out" 2>&1; then
        actual=0
    else
        actual=$?
    fi
    if [ "$actual" -eq "$expected" ]; then
        pass "$label"
    else
        fail "$label (exit $actual, expected $expected)"
        sed 's/^/    /' "$tmpdir/out" >&2
    fi
}

# --- negative cases: a binary with no Optics must never pass ------------------

assert_status "a non-Optics binary is rejected" 1 "$gate" /bin/true

cp /bin/true "$tmpdir/stripped-nonoptics"
if strip "$tmpdir/stripped-nonoptics" 2>/dev/null; then
    assert_status "a stripped non-Optics binary is rejected" 1 \
        "$gate" "$tmpdir/stripped-nonoptics"
else
    printf 'skip: strip unavailable, stripped negative case not exercised\n'
fi

assert_status "a missing binary is rejected" 1 "$gate" "$tmpdir/does-not-exist"

# --- argument handling -------------------------------------------------------

assert_status "--expect without a value is rejected" 2 "$gate" --expect
assert_status "an unknown --expect value is rejected" 2 "$gate" --expect bogus
assert_status "an unknown option is rejected" 2 "$gate" --frobnicate

# --- positive cases: need a real build --------------------------------------

if [ ! -f "$binary" ]; then
    printf 'skip: no built binary at %s; build it to run the positive cases\n' "$binary"
else
    assert_status "the built binary reports a mode" 0 "$gate" "$binary"

    mode=$("$gate" "$binary" 2>&1 || true)
    case "$mode" in
        *"links Optics dynamically"*)
            assert_status "the announced dynamic mode is accepted" 0 \
                "$gate" --expect dynamic "$binary"
            assert_status "the announced dynamic mode is not accepted as static" 1 \
                "$gate" --expect static "$binary"
            ;;
        *"embeds the Optics native stack"*)
            assert_status "the announced static mode is accepted" 0 \
                "$gate" --expect static "$binary"
            assert_status "the announced static mode is not accepted as dynamic" 1 \
                "$gate" --expect dynamic "$binary"

            cp "$binary" "$tmpdir/stripped-static"
            if strip "$tmpdir/stripped-static" 2>/dev/null; then
                assert_status "a fully stripped binary fails --expect static" 1 \
                    "$gate" --expect static "$tmpdir/stripped-static"
                assert_status "a fully stripped binary passes when explicitly allowed" 0 \
                    "$gate" --expect static --allow-unverified-stripped \
                    "$tmpdir/stripped-static"
            else
                printf 'skip: strip unavailable, stripped-artifact cases not exercised\n'
            fi
            ;;
        *)
            fail "the gate did not announce a recognised mode"
            printf '    %s\n' "$mode" >&2
            ;;
    esac
fi

if [ "$failures" -ne 0 ]; then
    printf '%s gate self-test(s) failed\n' "$failures" >&2
    exit 1
fi
printf 'gate self-test passed\n'
