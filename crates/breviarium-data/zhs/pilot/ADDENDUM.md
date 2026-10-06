# Breviary zh-alt translation addendum — Catholic / 思高 paradigm

Read AFTER "robo-hieronymus prompt ALT.md". You translate the Roman Breviary
(Divine Office, 1960) from **Latin** into Chinese in a uniform, official Catholic
style (天主 / 圣神 / 宗徒 / 思高 Bible — the Vatican.va · 思高 · 天主教教理 register).
The ALT prompt's terminology, names, book abbreviations, and glossary fully apply.
This file adds breviary-specific mechanics.

## Locked glossary — follow VERBATIM
A shared term-lock glossary is at
`/home/miyuruasuka/develop/breviarium/crates/breviarium-data/zh-alt/glossary.md`.
For every Latin term/name/formula it lists, use the given Chinese exactly, so the
same word is identical across all chunks. It overrides your own judgment (but
never the absolute Catholic rules: God = 天主, 圣神, 宗徒, 阿肋路亚, 思高 names).
If you must coin a rendering for a recurring proper name not in the glossary,
note it in `_notes`.

## Output contract
- Input: a JSON array of Latin strings.
- Output: a JSON **object** mapping each input string (verbatim, as the key) to
  its Chinese translation. Every input string must appear as a key, exactly.
- Write it to the output path you are given. Nothing else in that file. You may
  add ONE `"_notes"` string key for uncertainties.

## Bible quotations — 思高 for ALL scripture
Most psalms, canticles, antiphons, chapters, and readings are scripture. For any
scriptural string use the **思高 (Studium Biblicum)** wording — for BOTH
protocanon and deuterocanon (this is a Catholic edition; do NOT use the
Protestant Chinese Union Version):
1. Identify the verse(s) from the Latin and any leading reference.
2. Fetch 思高 from `https://www.ccreadbible.org/chinesebible/znsigao` — follow the
   links to the book, then the chapter. Use the WebFetch tool (if unavailable,
   call ToolSearch with query "select:WebFetch", and "select:WebSearch" for
   search). For official Catholic document wording use web_search.
3. Use the 思高 wording **verbatim** for the verse body, then re-apply the
   breviary's structural markers (below). Do not paraphrase fetched scripture.
4. Catholic biblical names, book names and abbreviations per the ALT prompt
   (路=Luke, 若=John, 咏=Psalms, 匝=Zechariah, 德=Sirach, 智=Wisdom, 宗=Acts, …).

### Psalm numbering offset
The breviary uses **Vulgate/Septuagint** psalm numbers. The 思高圣咏集 numbers the
Psalms by the **Hebrew** convention, so for most of the psalter they differ by
one, exactly like modern Bibles:
- Vulgate Ps 10–112 → 思高 圣咏 **+1** (e.g. Vulgate 116 = 思高 圣咏 **117**)
- Vulgate Ps 9 = 思高 9–10; 113 = 114–115; 114–115 = 116; 146–147 = 147.
  Ps 1–8 and 148–150 match.
**Verify on the site** and note uncertainties in `_notes`. NT canticles
(Magnificat = Luke 1, Benedictus = Luke 1, Nunc Dimittis = Luke 2) and OT
canticles cited by their own book (Isaiah, etc.) use that book's numbering — no
psalm offset.

## Divine words
- God = **天主**.  Holy Spirit = **圣神**.  Apostle = **宗徒**.
- The Lord (Dóminus): in general prose use **主**; but inside verbatim 思高
  scripture quotations KEEP the 思高 wording exactly — 思高 prints **上主** for the
  Lord (YHWH) and **天主** for God. Do NOT change 思高's 上主/天主 to 神/主.
- Amen = **阿们**.  Allelúia / allelúia / Alleluia = **阿肋路亚**.

## Structural markers — preserve verbatim, in place, line-for-line
The renderer aligns the zh-alt column to the Latin line-for-line and
node-for-node.
- **Leading verse reference** (`1:46`, `116:1`, `127:3a`): keep at the very start
  of the line, unchanged, then a space, then the Chinese.
- **`*`** (mediant) and **`+`** (flex): keep at the same point in the sense of the
  verse, spaces as in the Latin. A `*` must have REAL Chinese text on BOTH sides
  (split the 思高 wording so the first half precedes `*` and the second follows).
- **`~`** markers: keep if present.
- Citation/reference lines (`Luc. 1:46-55`, `Zach 8:19`, `Hom. 34 in Evang.`):
  translate the book abbreviation to the Catholic form (路, 匝, …), keep numbers;
  e.g. `Luc. 1:46-55` → `路 1:46-55`.
- Pure heading/title lines (`Canticum B. Mariæ Virginis`, `Léctio sancti
  Evangélii secúndum Lucam`, `Homilía S. Gregórii Papæ`) → translate as a title
  (no quotation marks), using established Catholic wording (e.g. 圣母玛利亚的赞主曲
  / 路加福音 / 教宗大圣额我略讲道). Flag uncertain names in `_notes`.
- Rubric fragments in parentheses (`(sed post partum omittitur)`) → translate as
  rubric text, keep the parentheses.

## Tone
Formal, precise, ecclesial Catholic liturgical register (Vatican.va · 思高 ·
天主教教理 style). Match the 思高 dignified tone for scripture.

## When unsure
Add a `"_notes"` key listing proper names / mappings / terminology you were
unsure about — make your best Catholic choice and note it; do not block.
