export const meta = {
  name: 'zh-breviary-translate',
  description: 'Translate the breviary lexicon Latin strings to Chinese in chunks',
  phases: [{ title: 'Translate', detail: 'one agent per chunk of latin.json' }],
}

// args: { total, chunkSize, startChunk, endChunk }
//   total      - number of strings in zh/latin.json (default 19080)
//   chunkSize  - strings per agent (default 130)
//   startChunk - first chunk index to run, inclusive (default 0)
//   endChunk   - last chunk index to run, exclusive (default = all)
const A = args || {}
const TOTAL = A.total || 19080
const CHUNK = A.chunkSize || 130
const nChunks = Math.ceil(TOTAL / CHUNK)
const startChunk = A.startChunk || 0
const endChunk = A.endChunk || nChunks

const ROOT = '/home/miyuruasuka/develop/breviarium/crates/breviarium-data'
const ROBO = '/home/miyuruasuka/Documents/dorthisvault/sophronius etc/robo-hieronymus prompt.md'
const ADDENDUM = `${ROOT}/zh/pilot/ADDENDUM.md`
const GLOSSARY = `${ROOT}/zh/glossary.md`
const LATIN = `${ROOT}/zh/latin.json`

function pad(n) { return String(n).padStart(4, '0') }

function chunkPrompt(i, start, end) {
  const out = `${ROOT}/zh/full/chunk-${pad(i)}.json`
  return `You translate ONE chunk of Roman Breviary (Divine Office, 1960) Latin strings into Chinese. Be precise and follow the locked rules exactly.

STEP 1 — read these THREE files IN FULL before translating:
  1. Master terminology rules: "${ROBO}"
  2. Breviary mechanics addendum: "${ADDENDUM}"
  3. LOCKED glossary (overrides your judgment; follow VERBATIM): "${GLOSSARY}"

STEP 2 — read the JSON array of all Latin strings at: "${LATIN}"
  Your chunk is the slice [${start}:${end}] (0-indexed, end EXCLUSIVE) — that is
  strings index ${start} through ${end - 1}. Translate ONLY those ${end - start} strings.

STEP 3 — translate each string to Chinese:
  - Scripture: fetch the OFFICIAL Chinese text. Protocanon → CUV Revised
    (BibleGateway version RCU17SS); deuterocanon → 思高/Studium (ccreadbible.org
    /chinesebible/znsigao). Use the WebFetch tool (if it is not already available,
    call ToolSearch with query "select:WebFetch" first, and "select:WebSearch" if
    you need search). Mind the Vulgate→Hebrew psalm-number offset (Vulgate Ps
    10-112 = CUV +1, e.g. Vulgate 116 = CUV 诗篇 117), per the addendum.
  - Preserve ALL structural markers verbatim and in place, line-for-line: leading
    verse numbers (1:46, 116:1), the mediant * (REAL Chinese text on BOTH sides),
    the flex +, ~ markers, citation lines (translate book abbrev per glossary, keep
    numbers). Amen=阿们, Allelúia=阿肋路亚 (NOT 哈利路亚), God=神.
  - Apply the locked glossary for every name/term/formula it covers.

STEP 4 — write a JSON OBJECT mapping each of your chunk's input Latin strings
  (VERBATIM as the key — copy exactly, do not alter whitespace/accents) to its
  Chinese translation. Every one of your ${end - start} strings MUST be a key.
  You may add ONE extra "_notes" string key for uncertainties. Write ONLY this
  JSON object (UTF-8, ensure_ascii false) to: "${out}"

Return ONLY a one-line status: "chunk ${i}: <n> translated, <f> scripture fetched, notes: <short>".`
}

phase('Translate')
const thunks = []
for (let i = startChunk; i < endChunk; i++) {
  const start = i * CHUNK
  const end = Math.min(start + CHUNK, TOTAL)
  if (start >= TOTAL) break
  thunks.push(() => agent(chunkPrompt(i, start, end), {
    label: `chunk-${pad(i)} [${start}:${end}]`,
    phase: 'Translate',
    agentType: 'general-purpose',
  }))
}
log(`launching ${thunks.length} chunk agents (chunks ${startChunk}..${endChunk - 1}, size ${CHUNK})`)
const results = await parallel(thunks)
const ok = results.filter(Boolean).length
log(`done: ${ok}/${thunks.length} agents returned`)
return { launched: thunks.length, returned: ok, results }
