# Breviary zh-translation addendum (read AFTER the robo-hieronymus prompt)

You are translating the **Roman Breviary (Divine Office), 1960 rubrics** from
**Latin** (not English) into Chinese. The robo-hieronymus terminology rules
(`robo-hieronymus prompt.md`) still fully apply: God = 神 always; Protestant/CUV
default with the named Catholic exceptions; Bible names per CUV; 「」 quotation
marks; etc. This file adds breviary-specific mechanics.

## Locked glossary — follow VERBATIM
A shared term-lock glossary is at
`/home/miyuruasuka/develop/breviarium/crates/breviarium-data/zh/glossary.md`.
For every Latin term/name/formula listed there, use the given Chinese exactly,
so the same word is rendered identically across all chunks. The glossary
overrides your own judgment (but never the absolute rules: God = 神, 阿肋路亚,
CUV Bible names). If you must coin a rendering not in the glossary for a
recurring proper name, note it in `_notes` so it can be added.

## Output contract
- Your input is a JSON array of Latin strings.
- Return a JSON **object** mapping each input string (verbatim, as the key) to
  its Chinese translation. Every input string must appear as a key, exactly.
- Write it to the output path you are given. Nothing else in that file.

## Bible quotations — fetch the official Chinese text
Most psalms, canticles, antiphons, chapters, and readings are scripture. For any
scriptural string:
1. Identify the verse(s) from the Latin and any leading reference.
2. **Protocanon** → Chinese Union Version, Revised (BibleGateway `RCU17SS`).
   Fetch e.g. `https://www.biblegateway.com/passage/?search=<book>+<chap>&version=RCU17SS`.
3. **Deuterocanon** (Tobit, Judith, Wisdom, Sirach/Ecclesiasticus, Baruch,
   1–2 Maccabees, additions to Daniel/Esther) → Studium (思高) text from
   ccreadbible.org/chinesebible/znsigao (follow the links to the book/chapter).
4. Use the official wording **verbatim** for the verse body, then re-apply the
   breviary's structural markers (below). Do not paraphrase fetched scripture.

### CRITICAL: psalm numbering offset
The breviary uses **Vulgate/Septuagint** psalm numbers. CUV uses Hebrew numbers.
For most of the psalter they differ by one:
- Vulgate Ps 10–112  →  Hebrew/CUV Ps **+1**  (e.g. Vulgate 116 = CUV 诗篇 **117**)
- Vulgate Ps 9 = CUV 9–10; Ps 113 = CUV 114–115; Ps 114–115 = CUV 116;
  Ps 146–147 = CUV 147. Ps 1–8 and 148–150 match.
So when a string starts `116:1`, fetch CUV **诗篇 117:1**. NT canticles
(Magnificat = Luke 1, Benedictus = Luke 1, Nunc Dimittis = Luke 2) and OT
canticles cited by their own book (Isaiah, etc.) use that book's numbering —
no psalm offset. If unsure of the mapping, say so in a `_notes` key.

## Structural markers — preserve verbatim, in place
The renderer aligns the zh column to the Latin **line-for-line and node-for-node**.
- **Leading verse reference** like `1:46`, `116:1`, `127:3a`: keep it at the very
  start of the line, unchanged, followed by a space, then the Chinese.
- **`*`** (mediant pause) and **`+`** (flex): keep them at the same point in the
  sense of the verse, surrounded by spaces as in the Latin. For antiphons, the
  `*` marks the intonation split — keep it. **CRITICAL: a `*` must have real
  Chinese text on BOTH sides** — it divides the verse into two sung halves. Split
  the (possibly fetched CUV) wording so the first half is before the `*` and the
  second half after it, mirroring where the Latin places the `*`. Never leave the
  text after `*` empty (the pilot's Magnificat 1:47 `…为乐； *` with nothing after
  was wrong — it must read e.g. `我灵欢腾 * 因神我的救主`).
- **`~`** line markers: keep if present.
- Citation/reference lines that are NOT prose (`Luc. 1:46-55`, `Luc 15:`,
  `Zach 8:19`, `Hom. 34 in Evang.`): translate the book abbreviation to the CUV
  abbreviation (路 for Luke, 亚 for Zechariah, etc.) and keep the numbers; e.g.
  `Luc. 1:46-55` → `路 1:46-55`.
- `Amen` → 阿们.  `Allelúia`/`allelúia`/`Alleluia` → **阿肋路亚** (Catholic
  liturgical transliteration — this is a fixed exception; do NOT use 哈利路亚).
- A line that is purely a heading/title (`Canticum B. Mariæ Virginis`,
  `Léctio sancti Evangélii secúndum Lucam`, `Homilía S. Gregórii Papæ`) is
  translated as a title (no quotation marks): e.g. 圣母马利亚的赞主曲 / 路加福音
  / 教宗圣额我略讲道. Use established Chinese for these where one exists; flag
  uncertain proper names in `_notes`.
- Rubric fragments in parentheses (`(sed post partum omittitur)`) are rubrics →
  translate as rubric text, keep the parentheses.

## Tone
Liturgical, reverent, classical register (this is higher-register content, like
the Catechism / encyclicals). Match the CUV's dignified tone for scripture.

## When unsure
Add a `"_notes"` key to your output object (a string) listing any proper names,
mappings, or terminology choices you were unsure about — do NOT block on them;
make your best choice and note it. `_notes` is the only non-Latin key allowed.
