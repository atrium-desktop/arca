#!/usr/bin/env python3
"""Code-to-documentation synchronisation gates.

`docs/dev/standards/workflow.md` requires user-visible surfaces to be
documented in the same change that introduces them. Review discipline alone
did not hold: the CLI reference was missing `--new-window`, the configuration
reference documented a `sort = modified` value the parser rejected, and the
shortcut reference omitted seven bound keys.

Each check below derives the surface from the *source* and asserts the
reference documents it, so the two can no longer drift silently.

Usage: check-docs.py {cli|config|keys|links}
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)


def ok(message: str) -> None:
    print(f"ok: {message}")


# ---------------------------------------------------------------- CLI flags


def documented_cli_flags() -> set[str]:
    """Flags the CLI reference claims the binary accepts."""
    text = read(DOCS / "reference" / "cli.md")
    return set(re.findall(r"`(--[a-zA-Z-]+)`", text))


def source_cli_flags() -> set[str]:
    """Flags the entrypoint actually parses.

    Derived from the `match argument.as_str()` arms plus the pre-dispatch
    checks, which is where every accepted flag must appear.
    """
    # Flags live in the pure parser; `unknown_options` enumerates every one it
    # recognises, which is the authoritative list.
    text = read(ROOT / "crates" / "arca" / "src" / "cli.rs")
    # Test code uses deliberate non-flags ("-x") to exercise rejection.
    text = text.split("#[cfg(test)]", 1)[0]
    flags: set[str] = set()
    parts = text.split("fn unknown_options", 1)
    if len(parts) == 2:
        body = parts[1].split("}", 1)[0]
        flags.update(re.findall(r'"(-{1,2}[a-zA-Z-]+)"', body))
    # Single-letter forms are matched through `matches!` elsewhere in the file.
    flags.update(re.findall(r'"(-[A-Za-z])"', text))
    return flags


def check_cli() -> None:
    documented = documented_cli_flags()
    source = source_cli_flags()
    # Help/version are exercised by the shell rather than documented as rows.
    # `--help`/`--version` are the long forms of `-h`/`-V`, documented as such.
    ignorable = {"-h", "-V", "--help", "--version"}
    missing = source - documented - ignorable
    if missing:
        fail(f"CLI flags parsed but undocumented in docs/reference/cli.md: {sorted(missing)}")

    # `--help` is a third copy of the same list. It drifted before (missing
    # -h/-V), so hold it to the same standard as the reference.
    cli_source = read(ROOT / "crates" / "arca" / "src" / "cli.rs")
    usage = cli_source.split("pub const USAGE", 1)
    if len(usage) == 2:
        body = usage[1].split('";', 1)[0]
        undocumented_in_usage = {flag for flag in source if flag not in body}
        if undocumented_in_usage:
            fail(f"--help text omits accepted flags: {sorted(undocumented_in_usage)}")
    stale = documented - source
    if stale:
        fail(f"CLI flags documented but not parsed by crates/arca: {sorted(stale)}")
    ok(f"CLI reference matches {len(source - ignorable)} parsed flags")


# ------------------------------------------------------------- config keys


def check_config() -> None:
    source = read(ROOT / "crates" / "arca-engine" / "src" / "config.rs")
    knob_block = source.split("fn apply_str", 1)[1].split("fn to_str", 1)[0]
    keys = set(re.findall(r'"([a-z_]+)"\s*(?:if|\||=>)', knob_block))
    if not keys:
        fail("could not derive configuration keys from Config::apply_str")

    reference = read(DOCS / "reference" / "configuration.md")
    undocumented = {key for key in keys if f"`{key}`" not in reference}
    if undocumented:
        fail(f"config keys parsed but undocumented: {sorted(undocumented)}")

    # Enum-valued keys: every accepted spelling must be listed. `modified` was
    # documented while the parser rejected it, and vice versa for `mtime`.
    entry = read(ROOT / "crates" / "arca-engine" / "src" / "entry.rs")
    sort_arms = entry.split("pub fn parse", 1)[1].split("}", 1)[0]
    sort_values = set(re.findall(r'"([a-z]+)"', sort_arms))
    missing = {value for value in sort_values if f"`{value}`" not in reference}
    if missing:
        fail(f"sort values accepted but undocumented: {sorted(missing)}")

    ok(f"configuration reference covers {len(keys)} keys and {len(sort_values)} sort values")


# -------------------------------------------------------- keyboard shortcuts


def check_keys() -> None:
    source = read(ROOT / "crates" / "arca-ui" / "src" / "app.rs")
    handler = source.split("fn handle_keys", 1)[1]
    reference = read(DOCS / "reference" / "keyboard-shortcuts.md")

    missing: list[str] = []

    # Ctrl+<letter> chords in the `k if ctrl => match` block.
    ctrl_block = handler.split("k if ctrl => match", 1)
    if len(ctrl_block) == 2:
        for letter in re.findall(r"'([a-z0-9])'\s*(?:if|\|)?", ctrl_block[1].split("_ => {}", 1)[0]):
            chord = f"Ctrl+{letter.upper()}"
            if chord not in reference:
                missing.append(chord)

    # Named keys handled through `lens::key::*` guards.
    named = {
        "F2": r"key::F2",
        "F3": r"key::F3",
        "F5": r"key::F5",
        "F11": r"key::F11",
        "Delete": r"key::DELETE",
        "Enter": r"key::RETURN",
        "Backspace": r"key::BACKSPACE",
        "Space": r"KEY_SPACE if",
    }
    for label, pattern in named.items():
        if re.search(pattern, handler) and label not in reference:
            missing.append(label)

    if missing:
        fail(f"bound shortcuts undocumented in docs/reference/keyboard-shortcuts.md: {missing}")
    ok("keyboard reference covers every bound shortcut group")


# ------------------------------------------------------------- doc links


def check_links() -> None:
    broken: list[tuple[str, str]] = []
    for markdown in list(DOCS.rglob("*.md")) + list(ROOT.glob("*.md")):
        text = read(markdown)
        for match in re.finditer(r"\[[^\]]+\]\(([^)]+)\)", text):
            target = match.group(1).split("#", 1)[0]
            if not target or target.startswith(("http://", "https://", "mailto:")):
                continue
            if not (markdown.parent / target).resolve().exists():
                broken.append((str(markdown.relative_to(ROOT)), target))
    if broken:
        for source, target in broken:
            print(f"  {source} -> {target}", file=sys.stderr)
        fail(f"{len(broken)} unresolved internal documentation link(s)")
    ok("all internal documentation links resolve")


# ----------------------------------------------------------- test locality

# A monolithic `lib.rs::tests` block grows into a thousand-line dumping ground
# where no test sits next to the code it constrains. Cap it and require every
# module with logic to own a `mod tests`.
MAX_ROOT_TESTS = 12


def check_test_locality() -> None:
    crate_src = ROOT / "crates" / "arca-ui" / "src"
    lib = read(crate_src / "lib.rs")
    root_tests = lib.count("#[test]")
    if root_tests > MAX_ROOT_TESTS:
        fail(
            f"crates/arca-ui/src/lib.rs holds {root_tests} tests "
            f"(cap {MAX_ROOT_TESTS}); put each beside the module it constrains"
        )

    # Modules above a size threshold must carry tests of their own.
    exempt = {"lib.rs", "test_support.rs", "device.rs"}
    orphaned = []
    for path in sorted(crate_src.glob("*.rs")):
        if path.name in exempt:
            continue
        source = read(path)
        if source.count("\n") < 80:
            continue  # too small to demand a suite
        if "#[cfg(test)]" not in source:
            orphaned.append(path.name)
    if orphaned:
        fail(f"modules with logic but no test module: {orphaned}")
    ok(f"test locality: {root_tests} root tests, every substantial module owns its suite")


CHECKS = {
    "cli": check_cli,
    "config": check_config,
    "keys": check_keys,
    "links": check_links,
    "tests": check_test_locality,
}


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] not in CHECKS:
        print(f"usage: {sys.argv[0]} {{{'|'.join(CHECKS)}}}", file=sys.stderr)
        sys.exit(2)
    CHECKS[sys.argv[1]]()


if __name__ == "__main__":
    main()
