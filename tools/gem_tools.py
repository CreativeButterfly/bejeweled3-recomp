#!/usr/bin/env python3
"""Analysis helpers for the game's PAK (the Python prototypes the Rust crates were checked against).

Reads the player's OWN main.pak. This file contains no game data. Never commit a PAK or
anything extracted from one.

usage: python3 gem_tools.py <main.pak> <command> [args]
  info                 entry count, data offset, sizes
  types                entry counts by file extension
  list [substring]     list entries (optionally filtered, case-insensitive)
  cat <path>           write one entry to stdout (redirect it; do not commit it)
  manifest             parse properties/resources.xml leniently and check every path resolves
  fonts                parse every font descriptor and check list lengths and glyph sheets
  models               parse every .p3d model (v1 and v2) and check indices and textures
"""
import collections, os, re, struct, sys

KEY = 0xF7
XOR = bytes(i ^ KEY for i in range(256))


class Pak:
    """PopCap-style PAK: every byte XORed with 0xF7; magic 0xBAC04AC0, version 0, index, data."""

    def __init__(self, path):
        with open(path, "rb") as f:
            self.raw = f.read()
        head = self.raw[:4_000_000].translate(XOR)
        magic, ver = struct.unpack_from("<II", head, 0)
        if magic != 0xBAC04AC0 or ver != 0:
            raise SystemExit(f"not a PAK: magic {magic:#x}, version {ver}")
        pos, self.entries = 8, []
        while True:
            flag = head[pos]; pos += 1
            if flag & 0x80:
                break
            nl = head[pos]; pos += 1
            name = head[pos:pos + nl].decode("latin1"); pos += nl
            size, ft = struct.unpack_from("<IQ", head, pos); pos += 12
            self.entries.append((name, size, ft))
        self.data_start = pos
        self.meta, off = {}, pos
        for name, size, _ in self.entries:
            self.meta.setdefault(self.norm(name), (off, size))
            off += size
        if off > len(self.raw):
            raise SystemExit("PAK is truncated")

    @staticmethod
    def norm(name):
        return name.replace("\\", "/").lower()

    def has(self, name):
        return self.norm(name) in self.meta

    def read(self, name):
        o, s = self.meta[self.norm(name)]
        return self.raw[o:o + s].translate(XOR)


# ---------- manifest (properties/resources.xml): not valid XML, parse leniently ----------
TAG = re.compile(r"<(/?)([A-Za-z_][A-Za-z0-9_]*)((?:\s+[^<>]*?)?)\s*(/?)>", re.S)
ATTR = re.compile(r"""([A-Za-z_][A-Za-z0-9_]*)(?:=("[^"]*"|'[^']*'|[^\s"'/>]+))?""")


def parse_manifest(text):
    groups, cur = [], None
    for m in TAG.finditer(text.lstrip("\ufeff")):
        close, name, body, selfc = m.groups()
        if close:
            if name in ("Resources", "CompositeResources"):
                cur = None
            continue
        a = {x.group(1): (x.group(2).strip("\"'") if x.group(2) else True) for x in ATTR.finditer(body)}
        if name in ("Resources", "CompositeResources"):
            groups.append({"tag": name, "attrs": a, "items": []})
            if not selfc:
                cur = groups[-1]
        elif cur is not None and name != "ResourceManifest":
            cur["items"].append((name, a))
    return groups


def cmd_manifest(p):
    groups = parse_manifest(p.read("properties/resources.xml").decode("utf-8", "replace"))
    items = [(g, k, a) for g in groups for k, a in g["items"]]
    print("groups:", len(groups), "| resources:", collections.Counter(k for _, k, _ in items).most_common())
    by_id = collections.defaultdict(list)
    for g, k, a in items:
        by_id[(g["attrs"].get("res"), a.get("id"))].append(a)
    missing = []
    for g, k, a in items:
        path = a.get("path")
        if not isinstance(path, str) or path.startswith("!ref:") and True:
            if isinstance(path, str):  # alias: target must exist somewhere
                tid = path[5:]
                if not any(i for (r, i) in by_id if i == tid):
                    missing.append(path)
            continue
        base = path.replace("\\\\", "\\")
        if k == "Image":
            ok = any(p.has(base + e) for e in (".jp2", ".gif", ".png", ".jpg")) or p.has(base + "_.gif")
        elif k == "Sound":
            ok = p.has(base + ".ogg")
        elif k in ("PopAnim", "PIEffect", "Font", "RenderEffect"):
            ok = p.has(base)
        else:
            ok = True  # File entries include loose files outside the PAK
        if not ok:
            missing.append(path)
    print("unresolved paths:", len(missing), missing[:5])


# ---------- font descriptors ----------
FTOK = re.compile(r"""\s*(?:(?P<str>'(?:\\.|[^'\\])*'|"(?:\\.|[^"\\])*")|(?P<num>-?\d+(?:\.\d+)?)|(?P<word>[A-Za-z_][A-Za-z0-9_]*)|(?P<p>[(),;]))""")


def font_value(toks, i):
    k, v = toks[i]
    if k == "num":
        return (float(v) if "." in v else int(v)), i + 1
    if k == "str":
        q, body = v[0], v[1:-1]
        body = re.sub(r"\\x([0-9a-fA-F]{2})", lambda m: chr(int(m.group(1), 16)), body)
        return body.replace("\\" + q, q).replace("\\\\", "\\"), i + 1
    if k == "word":
        return ("ref", v), i + 1
    items, i = [], i + 1
    while toks[i] != ("p", ")"):
        x, i = font_value(toks, i)
        items.append(x)
        if toks[i] == ("p", ","):
            i += 1
    return items, i + 1


def parse_font(data):
    # Most descriptors are Latin-1; three start with a UTF-8 byte order mark.
    text = data[3:].decode("utf-8", "replace") if data.startswith(b"\xef\xbb\xbf") else data.decode("latin1")
    toks, i = [], 0
    while True:
        m = FTOK.match(text, i)
        if not m:
            if text[i:].strip():
                raise ValueError("bad token near " + repr(text[i:i + 30]))
            break
        toks.append((m.lastgroup, m.group(m.lastgroup)))
        i = m.end()
    layers, defs = collections.OrderedDict(), {}
    stmt = []
    for t in toks + [("p", ";")]:
        if t != ("p", ";"):
            stmt.append(t)
            continue
        if not stmt:
            continue
        cmd, args, j = stmt[0][1], [], 1
        while j < len(stmt):
            v, j = font_value(stmt, j)
            args.append(v)
        if cmd == "Define":
            defs[args[0][1]] = args[1]
        elif cmd == "CreateLayer":
            layers[args[0][1]] = {}
        elif cmd.startswith("LayerSet"):
            layers[args[0][1]][cmd[8:]] = args[1:]
        stmt = []
    return layers, defs


def cmd_fonts(p):
    ok = bad = 0
    for name, _, _ in p.entries:
        n = p.norm(name)
        if not (n.startswith("fonts/") and n.endswith(".txt")):
            continue
        try:
            layers, defs = parse_font(p.read(name))
        except Exception as e:
            print("FAIL", name, e); bad += 1; continue
        folder = n.rsplit("/", 1)[0]
        for ln, L in layers.items():
            res = lambda x: defs[x[1]] if isinstance(x, tuple) else x
            if "CharWidths" in L:
                lens = {len(res(L["CharWidths"][0])), len(res(L["CharWidths"][1]))}
                if "CharOffsets" in L: lens.add(len(L["CharOffsets"][1]))
                if "ImageMap" in L: lens.add(len(L["ImageMap"][1]))
                if len(lens) > 1:
                    print("inconsistent lists", name, ln); bad += 1
            if "Image" in L and not (p.has(f"{folder}/{L['Image'][0]}_.gif") or p.has(f"{folder}/{L['Image'][0]}.gif")):
                print("missing glyph sheet", name, ln, L["Image"][0]); bad += 1
        ok += 1
    print(f"font descriptors parsed: {ok}, problems: {bad}")


# ---------- .p3d models ----------
def parse_p3d(b):
    if b[:4] != b"\x00\xef\xbe\x3d":
        raise ValueError("bad magic")
    ver = struct.unpack_from("<I", b, 4)[0]
    if ver not in (1, 2):
        raise ValueError(f"version {ver}")
    i = 8

    def string(i):
        n = struct.unpack_from("<H", b, i)[0]
        return b[i + 2:i + 2 + n].decode("latin1"), i + 2 + n

    n = struct.unpack_from("<H", b, i)[0]; i += 2
    for _ in range(n): _, i = string(i)
    n = struct.unpack_from("<H", b, i)[0]; i += 2
    for _ in range(n):
        if ver >= 2: i += 1
        _, i = string(i)
    n = struct.unpack_from("<H", b, i)[0]; i += 2
    props = {}
    for _ in range(n):
        k, i = string(i); v, i = string(i); props[k] = v
    meshes = []
    while i < len(b):
        flags, fmt, nv = struct.unpack_from("<HIH", b, i); i += 8
        if fmt != 338: raise ValueError(f"vertex format {fmt}")
        verts = [struct.unpack_from("<3f3f4B2f", b, i + 36 * k) for k in range(nv)]; i += 36 * nv
        nt = struct.unpack_from("<H", b, i)[0]; i += 2
        idx = struct.unpack_from(f"<{3 * nt}H", b, i); i += 6 * nt
        if idx and max(idx) >= nv: raise ValueError("index out of range")
        meshes.append((verts, idx))
    if not meshes: raise ValueError("no mesh")
    return ver, props, meshes


def cmd_models(p):
    for name, _, _ in p.entries:
        if not name.lower().endswith(".p3d"):
            continue
        try:
            ver, props, meshes = parse_p3d(p.read(name))
        except Exception as e:
            print(f"{name:26} NOT PARSED ({e})"); continue
        tex = os.path.splitext(props.get("texture0.fileName", ""))[0].lower()
        found = any(p.has(f"images/1200/tex/{tex}{e}") for e in (".jp2", ".gif", ".png", ".jpg", "_.gif"))
        nv = sum(len(m[0]) for m in meshes); nt = sum(len(m[1]) // 3 for m in meshes)
        print(f"{name:26} v{ver} meshes={len(meshes)} verts={nv} tris={nt} texture={tex!r} found={found}")


def main(argv):
    if len(argv) < 3:
        print(__doc__); return 2
    p = Pak(argv[1]); cmd = argv[2]
    if cmd == "info":
        print("entries:", len(p.entries), "| data start:", p.data_start, "| data bytes:", sum(s for _, s, _ in p.entries), "| file bytes:", len(p.raw))
    elif cmd == "types":
        print(collections.Counter(os.path.splitext(n)[1].lower() for n, _, _ in p.entries).most_common())
    elif cmd == "list":
        sub = argv[3].lower() if len(argv) > 3 else ""
        for n, s, _ in p.entries:
            if sub in n.lower(): print(f"{s:10} {n}")
    elif cmd == "cat":
        sys.stdout.buffer.write(p.read(argv[3]))
    elif cmd == "manifest": cmd_manifest(p)
    elif cmd == "fonts": cmd_fonts(p)
    elif cmd == "models": cmd_models(p)
    else:
        print(__doc__); return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
