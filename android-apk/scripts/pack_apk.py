#!/usr/bin/env python3
"""Add the native library to aapt2's base APK, stored uncompressed and
16 KB page-aligned — what extractNativeLibs=false and 16 KB-page devices
(Android 15+) require. Written by hand because this build-tools' zipalign
prints its usage and exits 2 whenever -P is passed.

Usage: pack_apk.py <base.apk> <libfablegame.so> <out.apk>

The base APK's entries are copied verbatim (still-compressed bytes and
all); stored entries are kept 4-byte aligned, as a resources.arsc at
targetSdk 30+ must be.
"""

import struct
import sys
import zlib


def read_entries(path: str) -> list[dict]:
    """Every entry of a zip, with its raw (still-compressed) bytes."""
    data = open(path, "rb").read()
    eocd = data.rfind(b"PK\x05\x06")
    if eocd < 0:
        sys.exit(f"{path}: not a zip")
    n = struct.unpack("<H", data[eocd + 10 : eocd + 12])[0]
    cd_off = struct.unpack("<I", data[eocd + 16 : eocd + 20])[0]
    entries = []
    p = cd_off
    for _ in range(n):
        if data[p : p + 4] != b"PK\x01\x02":
            sys.exit(f"{path}: bad central directory at {p}")
        method = struct.unpack("<H", data[p + 10 : p + 12])[0]
        mtime, mdate = struct.unpack("<HH", data[p + 12 : p + 16])
        crc, csize, usize = struct.unpack("<III", data[p + 16 : p + 28])
        nlen, elen, clen = struct.unpack("<HHH", data[p + 28 : p + 34])
        lfh_off = struct.unpack("<I", data[p + 42 : p + 46])[0]
        name = data[p + 46 : p + 46 + nlen]
        flags = struct.unpack("<H", data[lfh_off + 6 : lfh_off + 8])[0]
        if flags & 0x8:
            sys.exit(f"{name}: data-descriptor zip not handled")
        ln, le = struct.unpack("<HH", data[lfh_off + 26 : lfh_off + 30])
        dstart = lfh_off + 30 + ln + le
        entries.append(
            dict(name=name, method=method, flags=flags, mtime=mtime, mdate=mdate,
                 crc=crc, csize=csize, usize=usize, raw=data[dstart : dstart + csize])
        )
        p += 46 + nlen + elen + clen
    return entries


def write_zip(path: str, entries: list[dict]) -> None:
    """A minimal zip writer: stored entries land `align`-aligned via extra-field
    padding (the same trick zipalign uses)."""
    out = bytearray()
    cds = []
    for e in entries:
        name, align = e["name"], e.get("align", 1)
        pad = -(len(out) + 30 + len(name)) % align
        # A well-formed extra record when it fits, plain padding otherwise
        # (libziparchive only skips the extra field by its length).
        if pad >= 4:
            extra = struct.pack("<HH", 0, pad - 4) + b"\x00" * (pad - 4)
        else:
            extra = b"\x00" * pad
        lfh_at = len(out)
        out += struct.pack(
            "<IHHHHHIIIHH", 0x04034B50, 20, e["flags"], e["method"], e["mtime"], e["mdate"],
            e["crc"], e["csize"], e["usize"], len(name), len(extra),
        )
        out += name + extra + e["raw"]
        cds.append(
            struct.pack(
                "<IHHHHHHIIIHHHHHII", 0x02014B50, 20, 20, e["flags"], e["method"], e["mtime"], e["mdate"],
                e["crc"], e["csize"], e["usize"], len(name), 0, 0, 0, 0, 0, lfh_at,
            )
            + name
        )
    cd_at = len(out)
    for cd in cds:
        out += cd
    out += struct.pack(
        "<IHHHHIIH", 0x06054B50, 0, 0, len(cds), len(cds), len(out) - cd_at, cd_at, 0
    )
    open(path, "wb").write(out)


def main() -> None:
    base, so, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    entries = read_entries(base)
    raw = open(so, "rb").read()
    # Replace any lib/arm64-v8a entry already in the base (a stale one from
    # an earlier run), then add ours stored and 16 KB page-aligned.
    entries = [e for e in entries if not e["name"].startswith(b"lib/")]
    entries.append(
        dict(name=b"lib/arm64-v8a/libfablegame.so", method=0, flags=0,
             mtime=0, mdate=0x21, crc=zlib.crc32(raw) & 0xFFFFFFFF,
             csize=len(raw), usize=len(raw), raw=raw, align=16384)
    )
    # Stored entries need 4-byte alignment too (resources.arsc at 30+).
    for e in entries:
        if e["method"] == 0:
            e["align"] = max(e.get("align", 1), 4)
    write_zip(out_path, entries)

    # Read it back and prove the alignments held: find each entry's real
    # local header through the central directory (a name can appear
    # anywhere, so searching for it directly is not enough).
    for e in read_entries(out_path):
        name = e["name"]
        data2 = open(out_path, "rb").read()
        pos = 0
        off = None
        while True:
            pos = data2.find(b"PK\x03\x04", pos)
            if pos < 0:
                break
            nlen, elen = struct.unpack("<HH", data2[pos + 26 : pos + 30])
            if data2[pos + 30 : pos + 30 + nlen] == name:
                off = pos + 30 + nlen + elen
                break
            pos += 4
        if off is None:
            sys.exit(f"lost {name!r} re-reading")
        want = 16384 if name.endswith(b".so") else 4 if e["method"] == 0 else 1
        state = "OK " if off % want == 0 else "BAD"
        print(f"{state} {name.decode():44s} offset {off:>9} mod {want:>5} = {off % want}")
        if off % want != 0:
            sys.exit(1)


if __name__ == "__main__":
    main()
