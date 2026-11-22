"""Temporary CI-only data export; no executable artifacts are exported."""
import base64
import hashlib
import io
from pathlib import Path
import tomllib
import zipfile

root = Path("Cargo.lock").read_bytes()
lock = Path("fuzz/Cargo.lock").read_bytes()

def identities(data):
    return {(p["name"], p["version"], p.get("source"), p.get("checksum"))
            for p in tomllib.loads(data.decode())["package"]}

assert identities(root) <= identities(lock), "bootstrap changed an existing dependency"
core = [p for p in tomllib.loads(lock.decode())["package"] if p["name"] == "seihai-core"]
assert len(core) == 1 and core[0]["source"].endswith("#81ba7e24df32ff5f937d0c09e7d5f75da6aa6913")
assert Path("Cargo.lock").read_bytes() == root
archive = io.BytesIO()
total = 0
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
    for path in sorted(Path("fuzz/corpus").glob("*/*")):
        data = path.read_bytes()
        total += len(data)
        assert len(data) <= 131072
        output.writestr(path.relative_to("fuzz/corpus").as_posix(), data)
assert total <= 512 * 1024
for name, data in [("LOCK", lock), ("CORPUS", archive.getvalue())]:
    print(f"FUZZ_{name}_SHA256={hashlib.sha256(data).hexdigest()}")
    encoded = base64.b64encode(data).decode()
    for offset in range(0, len(encoded), 2000):
        print(f"FUZZ_{name}_DATA={encoded[offset:offset + 2000]}")
