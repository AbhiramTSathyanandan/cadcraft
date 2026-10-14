#!/usr/bin/env python3
"""Generate the CADCraft command reference the Craneye drawing assistant reads.

Every `CommandSpec::new("id", "Label", …)` under crates/engine/src/cmd is listed with its
aliases, menu path and parameter doc, exactly as the engine declares them — the same text
`engine.commands` / the MCP `list_commands` tool returns, but flattened into one document so
an LLM can have all of it in context (prompt-cached) instead of spending tool calls looking
commands up one filter at a time.

Usage: python3 tools/command-reference/generate.py > docs/command-reference.md
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CMD_DIR = ROOT / "crates" / "engine" / "src" / "cmd"

SPEC_RE = re.compile(r'CommandSpec::new\(\s*"(?P<id>[^"]+)"\s*,\s*"(?P<label>[^"]*)"', re.S)
STR_RE = re.compile(r'"((?:[^"\\]|\\.)*)"')


def unescape(s: str) -> str:
    return s.encode("utf-8").decode("unicode_escape") if "\\" in s else s


def parse_file(path: Path):
    text = path.read_text(encoding="utf-8")
    out = []
    for m in SPEC_RE.finditer(text):
        start = m.end()
        # The builder chain ends at the next CommandSpec::new, the end of the vec, or a bare
        # `;` at statement level.
        nxt = SPEC_RE.search(text, start)
        end = nxt.start() if nxt else len(text)
        chain = text[start:end]
        params = alias = menu = None
        pm = re.search(r'\.params\(\s*("(?:[^"\\]|\\.)*")', chain, re.S)
        if pm:
            params = unescape(pm.group(1)[1:-1])
        am = re.search(r"\.alias\(&\[(.*?)\]\)", chain, re.S)
        if am:
            alias = [unescape(s) for s in STR_RE.findall(am.group(1))]
        mm = re.search(r"\.menu\(&\[(.*?)\]\)", chain, re.S)
        if mm:
            menu = [unescape(s) for s in STR_RE.findall(mm.group(1))]
        out.append({"id": m.group("id"), "label": m.group("label"), "params": params, "alias": alias, "menu": menu})
    return out


def main() -> None:
    sections = []
    for path in sorted(CMD_DIR.rglob("*.rs")):
        specs = parse_file(path)
        if not specs:
            continue
        sections.append((path.relative_to(CMD_DIR).as_posix().removesuffix(".rs"), specs))
    total = sum(len(s) for _, s in sections)
    w = sys.stdout.write
    w(f"## Command catalogue ({total} commands, generated from crates/engine/src/cmd)\n\n")
    w("Format: `id` — Label · aliases · menu path\n  params: `{…}` (the JSON object `execute {command: id, params}` takes; `?` = optional; `a | b` = one of)\n\n")
    for module, specs in sections:
        w(f"### {module}\n\n")
        for s in specs:
            bits = [f"`{s['id']}` — {s['label']}"]
            if s["alias"]:
                bits.append("aliases " + ", ".join(f"`{a}`" for a in s["alias"]))
            if s["menu"]:
                bits.append("menu " + " › ".join(s["menu"]))
            w("- " + " · ".join(bits) + "\n")
            if s["params"]:
                w(f"  params: `{s['params']}`\n")
        w("\n")


if __name__ == "__main__":
    main()
