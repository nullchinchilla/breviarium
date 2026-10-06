#!/usr/bin/env python3
"""Post-merge corrections to the zh translation dict (zh/chinese.json).

Currently one fixup — the divine-name normalization (user policy):

  The breviary's Latin always reads ``Dóminus`` (Lord), but CUV prints 耶和华
  (YHWH) for the divine name in OT quotations. Normalize 耶和华 → 主, EXCEPT the
  ``主耶和华`` adjacency (Adonai YHWH / "the Lord GOD"), where the second word
  becomes 神 to avoid a 主主 collision — mirroring Dóminus/Deus = "the LORD our
  God". So:  主耶和华 → 主神 ;  then  耶和华 → 主.

NOT touched (per user): 天主 and 上主 — these are either legitimate terms
(天主教, 天主经, the saint name 若望·德·天主) or verbatim 思高/Studium wording in
deuterocanon quotations, where the God=神 ban does not apply.

Run after tools/zh_merge.py and before ``en2.py --lang zh apply``.
"""
import json
from pathlib import Path

ZH = Path(__file__).resolve().parent.parent / "zh"


def fix_divine_name(text: str) -> tuple[str, int, int]:
    n_adon = text.count("主耶和华")
    t = text.replace("主耶和华", "主神")
    n_yhwh = t.count("耶和华")
    t = t.replace("耶和华", "主")
    return t, n_adon, n_yhwh


def main():
    path = ZH / "chinese.json"
    m = json.load(open(path, encoding="utf-8"))
    adon = yhwh = touched = 0
    for k, v in m.items():
        nv, a, y = fix_divine_name(v)
        if nv != v:
            m[k] = nv
            touched += 1
            adon += a
            yhwh += y
    path.write_text(json.dumps(m, ensure_ascii=False, indent=2) + "\n",
                    encoding="utf-8")
    leftover = sum(v.count("耶和华") for v in m.values())
    print(f"entries touched          : {touched}")
    print(f"主耶和华 → 主神           : {adon}")
    print(f"耶和华 → 主              : {yhwh}")
    print(f"耶和华 remaining (should 0): {leftover}")
    print(f"上主 kept (Studium)       : {sum(v.count('上主') for v in m.values())}")
    print(f"天主 kept (Studium/terms) : {sum(v.count('天主') for v in m.values())}")


if __name__ == "__main__":
    main()
