#!/usr/bin/env python3
"""Audit download modules for HTTP URLs outside the allowed GitHub domains.

Only scans files that perform binary downloads (update modules and scheduler).
Other files (agent command endpoints, nginx configs) are intentionally excluded.

Exits 1 if any non-allowed URL literal is found in non-comment lines.

To suppress a specific line that is intentionally non-GitHub (e.g. a self health check),
add an inline comment:
    .get("http://127.0.0.1:8080/health") // audit-urls: ok — reason

Run from the repository root.
"""
import re
import sys
import pathlib

# Only GitHub release domains are allowed for binary downloads.
ALLOWED = re.compile(
    r"https?://"
    r"(github\.com|objects\.githubusercontent\.com|api\.github\.com)"
)

URL_RE = re.compile(r'https?://[^\s\'">,)]+')

# Format strings / templates — skip lines containing these (not real URLs)
FORMAT_MARKERS = re.compile(r"\{[^}]*\}|\$\w+|%[sdfi]")

COMMENT_RE = re.compile(r"^\s*//")
SUPPRESS_RE = re.compile(r"//\s*audit-urls:\s*ok")

# Only files that perform outbound binary downloads.
# Adding a new download path outside these files requires a conscious update here.
SCAN_FILES = [
    "lynx/dashboard/server/src/update.rs",
    "lynx/dashboard/server/src/scheduler.rs",
]

failures: list[str] = []

for path_str in SCAN_FILES:
    f = pathlib.Path(path_str)
    if not f.exists():
        print(f"⚠️  Scan target not found: {f} (skipped)")
        continue
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
        if COMMENT_RE.match(line):
            continue
        if SUPPRESS_RE.search(line):
            continue
        if FORMAT_MARKERS.search(line):
            continue
        for url in URL_RE.findall(line):
            if not ALLOWED.match(url):
                failures.append(f"  {f}:{i}  {url}")

if failures:
    print("❌ Non-allowed URL found in download modules:")
    for entry in failures:
        print(entry)
    print()
    print("Allowed domains: github.com, objects.githubusercontent.com, api.github.com")
    print()
    print("If this URL is intentional (e.g. a self health check, not a download):")
    print("  Add an inline suppression comment on that line:")
    print('    .get("http://...") // audit-urls: ok — reason')
    print()
    print("If this is a new external download domain:")
    print("  Add it to ALLOWED in .github/scripts/audit-urls.py with justification.")
    sys.exit(1)

scanned = ", ".join(SCAN_FILES)
print(f"✅ All HTTP URLs in download modules are from allowed domains.")
print(f"   Scanned: {len(SCAN_FILES)} files")
