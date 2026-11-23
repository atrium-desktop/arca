#!/bin/sh
# Report or assert the Optics link mode of a built Arca binary.
#
# Upstream `optics-build` supports both modes and picks per build:
#
#   OPTICS_STATIC=1   require the liboptics_*.a archives (fail if absent)
#   OPTICS_DYNAMIC=1  force the liboptics_*.so shared objects
#   static feature    request the archives (environment overrides it)
#   (none)            shared-preferred: shared objects if present, else
#                     archives on a static-only tree
#
# Arca's release pipeline pins the static mode; development builds follow the
# host. This script inspects the actual ELF output instead of trusting the
# intent, and asserts whichever mode the caller requires.
#
# Usage:
#   scripts/check-optics-link.sh [--expect static|dynamic]
#                                [--allow-unverified-stripped]
#                                [path-to-binary]
#
# Without --expect it prints the detected mode and exits 0. With --expect it
# exits 1 when the binary does not match. With no path it inspects
# target/release/arca.
#
# A stripped binary loses its symbol table, so the embedded-symbol evidence is
# unavailable and the gate exits 1 for --expect static. Run it before `strip`
# (`strip --strip-debug` keeps the symbols) or pass
# --allow-unverified-stripped to accept the weaker evidence deliberately.
#
# See docs/adr/0014-optics-link-mode-and-static-release.md.
set -eu

expected=
binary=
want_expect=no
allow_unverified=no

for arg in "$@"; do
    if [ "$want_expect" = yes ]; then
        expected=$arg
        want_expect=no
        continue
    fi
    case "$arg" in
        --expect)
            want_expect=yes
            ;;
        --expect=*)
            expected=${arg#--expect=}
            ;;
        --allow-unverified-stripped)
            allow_unverified=yes
            ;;
        -h|--help)
            sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        -*)
            printf 'error: unknown option: %s\n' "$arg" >&2
            exit 2
            ;;
        *)
            binary=$arg
            ;;
    esac
done

if [ "$want_expect" = yes ]; then
    printf 'error: --expect needs an argument (static or dynamic)\n' >&2
    exit 2
fi

case "${expected:-auto}" in
    auto|static|dynamic) ;;
    *)
        printf 'error: --expect must be static or dynamic, got %s\n' "$expected" >&2
        exit 2
        ;;
esac

binary=${binary:-target/release/arca}

if [ ! -f "$binary" ]; then
    printf 'error: binary not found: %s\n' "$binary" >&2
    printf 'build it first: cargo build --release -p arca\n' >&2
    exit 1
fi

for tool in readelf nm; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'error: %s (binutils) is required\n' "$tool" >&2
        exit 1
    fi
done

dynamic_needed=$(readelf -d --wide "$binary" | grep -o 'liboptics_[a-z_]*\.so[^]]*' || true)

# A static link has nothing to find at run time. Any runpath is suspect, and the
# ones that matter are those that would let a shared Optics module satisfy the
# load behind the gate's back: a directory that actually holds liboptics_*.so, or
# a path naming an Optics tree/prefix. Directory membership is checked instead of
# a path substring so an install into /usr/local/lib is not missed.
optics_rpath=
while IFS= read -r dir; do
    [ -n "$dir" ] || continue
    case "$dir" in
        *optics*) optics_rpath="$optics_rpath$dir
" ;;
        *)
            if [ -d "$dir" ] && [ -n "$(find "$dir" -maxdepth 1 -name 'liboptics_*.so*' -print -quit 2>/dev/null)" ]; then
                optics_rpath="$optics_rpath$dir
"
            fi
            ;;
    esac
done <<EOF
$(readelf -d --wide "$binary" \
    | grep -E '\((RPATH|RUNPATH)\)' \
    | sed 's/.*\[//; s/\].*//' \
    | tr ':' '\n')
EOF
optics_rpath=$(printf '%s' "$optics_rpath" | sed '/^$/d')

# Static archives are linked, not registered as a dependency, so the symbols
# have to be in the executable. A packager may strip the symbol table, which
# removes that evidence; the gate then degrades to what survives strip: no
# Optics DT_NEEDED entry, no Optics runpath, and at least one optics_* code
# reference in the binary (the C entry points are referenced by name in the
# generated bindings and panic messages).
embedded=
if readelf -S "$binary" | grep -q '\.symtab'; then
    if nm "$binary" 2>/dev/null | grep -q 'optics_app_run$'; then
        embedded=yes
    fi
else
    if strings -a "$binary" 2>/dev/null | grep -q 'optics_'; then
        embedded=stripped
    fi
fi

if [ -n "$dynamic_needed" ]; then
    mode=dynamic
elif [ "$embedded" = yes ]; then
    mode=static
elif [ "$embedded" = stripped ]; then
    # No DT_NEEDED entry for Optics and no symbol table: the modules are not
    # tracked by the loader, which is all this gate can observe after strip.
    mode=static-unverified
else
    mode=unknown
fi

report() {
    printf '%s\n' "$@" >&2
}

case "$mode" in
    dynamic)
        if [ "$expected" = static ]; then
            report "FAIL: $binary dynamically links Optics:" \
                "$(printf '%s\n' "$dynamic_needed" | sed 's/^/  /')" \
                "Rebuild Optics with static archives (meson -Ddefault_library=both)," \
                "then build with OPTICS_STATIC=1 (or the static Cargo feature)."
            exit 1
        fi
        printf 'ok: %s links Optics dynamically: %s\n' \
            "$binary" "$(printf '%s' "$dynamic_needed" | tr '\n' ' ')"
        ;;
    static)
        if [ "$expected" = dynamic ]; then
            report "FAIL: $binary embeds Optics but --expect dynamic was requested"
            report "Rebuild with OPTICS_DYNAMIC=1, or drop the --expect flag."
            exit 1
        fi
        if [ -n "$optics_rpath" ]; then
            report "FAIL: $binary carries an Optics build-tree runpath:" \
                "$(printf '%s\n' "$optics_rpath" | sed 's/^/  /')"
            exit 1
        fi
        printf 'ok: %s embeds the Optics native stack (no liboptics_*.so, no Optics runpath)\n' \
            "$binary"
        ;;
    static-unverified)
        if [ "$expected" = dynamic ]; then
            report "FAIL: $binary has no liboptics_*.so dependency but --expect dynamic was requested"
            exit 1
        fi
        if [ -n "$optics_rpath" ]; then
            report "FAIL: $binary carries an Optics build-tree runpath:" \
                "$(printf '%s\n' "$optics_rpath" | sed 's/^/  /')"
            exit 1
        fi
        if [ "$allow_unverified" = no ]; then
            report "FAIL: $binary is stripped, so the embedded Optics symbols cannot be" \
                "confirmed. Run this gate before \`strip\`, or pass" \
                "--allow-unverified-stripped to accept the weaker evidence" \
                "(no liboptics_*.so, no Optics runpath, optics_* references present)."
            exit 1
        fi
        printf 'ok: %s has no liboptics_*.so dependency and no Optics runpath; the\n' "$binary"
        printf '    embedded-symbol probe is unavailable because the binary is stripped\n'
        ;;
    unknown)
        report "FAIL: $binary neither depends on liboptics_*.so nor contains optics_* code;"
        report "the Optics stack was not linked at all."
        exit 1
        ;;
esac
