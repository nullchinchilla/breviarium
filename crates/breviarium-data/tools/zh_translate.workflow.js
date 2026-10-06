export const meta = {
  name: 'zh-breviary-translate',
  description: 'Translate the breviary lexicon Latin strings to Chinese in chunks',
  phases: [{ title: 'Translate', detail: 'one agent per chunk of latin.json' }],
}

// args: { total, chunkSize, startChunk, endChunk, chunks }
//   total      - number of strings in zh/latin.json (default 19080)
//   chunkSize  - strings per agent (default 130)
//   chunks     - explicit list of chunk indices to (re)run (overrides start/end)
//   startChunk - first chunk index to run, inclusive (default 0)
//   endChunk   - last chunk index to run, exclusive (default = all)
let A = args
if (typeof A === 'string') { try { A = JSON.parse(A) } catch (e) { A = {} } }
A = A || {}
const TOTAL = A.total || 19080
const CHUNK = A.chunkSize || 130
const nChunks = Math.ceil(TOTAL / CHUNK)
const startChunk = A.startChunk || 0
const endChunk = A.endChunk || nChunks
// Build the list of chunk indices to run.
const chunkList = Array.isArray(A.chunks) && A.chunks.length
  ? A.chunks
  : Array.from({ length: endChunk - startChunk }, (_, k) => startChunk + k)

const ROOT = '/home/miyuruasuka/develop/breviarium/crates/breviarium-data'
// Per-language config (defaults = original ecumenical zh; override via args for zh-alt).
const LANG = A.lang || 'zhs'
const ROBO = A.robo || '/home/miyuruasuka/Documents/dorthisvault/sophronius etc/robo-hieronymus prompt.md'
const ADDENDUM = A.addendum || `${ROOT}/zhs/pilot/ADDENDUM.md`
const GLOSSARY = A.glossary || `${ROOT}/zhs/glossary.md`
const LATIN = `${ROOT}/${LANG}/latin.json`
const MODEL = A.model || null
// Scripture-source line varies by paradigm; the addendum carries the detail.
const SCRIPTURE = A.scripture ||
  'Protocanon → CUV Revised (BibleGateway version RCU17SS); deuterocanon → 思高/Studium (ccreadbible.org/chinesebible/znsigao)'

function pad(n) { return String(n).padStart(4, '0') }

function chunkPrompt(tag, start, end, out) {
  return `You translate ONE chunk of Roman Breviary (Divine Office, 1960) Latin strings into Chinese. Be precise and follow the locked rules exactly.

STEP 1 — read these THREE files IN FULL before translating:
  1. Master terminology rules: "${ROBO}"
  2. Breviary mechanics addendum: "${ADDENDUM}"
  3. LOCKED glossary (overrides your judgment; follow VERBATIM): "${GLOSSARY}"

STEP 2 — read the JSON array of all Latin strings at: "${LATIN}"
  Your chunk is the slice [${start}:${end}] (0-indexed, end EXCLUSIVE) — that is
  strings index ${start} through ${end - 1}. Translate ONLY those ${end - start} strings.

STEP 3 — translate each string to Chinese, following the addendum and glossary:
  - Scripture: fetch the OFFICIAL Chinese text per the addendum (${SCRIPTURE}).
    Use the WebFetch tool (if it is not already available, call ToolSearch with
    query "select:WebFetch" first, and "select:WebSearch" if you need search).
    Mind the Vulgate psalm-number offset described in the addendum.
  - Preserve ALL structural markers verbatim and in place, line-for-line: leading
    verse numbers (1:46, 116:1), the mediant * (REAL Chinese text on BOTH sides),
    the flex +, ~ markers, citation lines (translate book abbrev per glossary, keep
    numbers). Apply the absolute divine-word + Alleluia/Amen rules from the
    addendum and the locked glossary for every name/term/formula they cover.

STEP 4 — write a JSON OBJECT mapping each of your chunk's input Latin strings
  (VERBATIM as the key — copy exactly, do not alter whitespace/accents) to its
  Chinese translation. Every one of your ${end - start} strings MUST be a key.
  You may add ONE extra "_notes" string key for uncertainties. Write ONLY this
  JSON object (UTF-8, ensure_ascii false) to: "${out}"

Return ONLY a one-line status: "${tag}: <n> translated, <f> scripture fetched, notes: <short>".`
}

phase('Translate')
const thunks = []
// Two modes: explicit string RANGES (args.ranges = [[start,end],...], written to
// seg-START-END.json so they never collide with chunk files) — used for fine-
// grained re-runs to dodge the output-token cap — or whole 130-chunks by index.
const useSeg = Array.isArray(A.ranges) && A.ranges.length
const ranges = useSeg
  ? A.ranges
  : chunkList.map((i) => [i * CHUNK, Math.min(i * CHUNK + CHUNK, TOTAL)])
for (const [start, end] of ranges) {
  if (start >= TOTAL) continue
  const out = useSeg
    ? `${ROOT}/${LANG}/full/seg-${start}-${end}.json`
    : `${ROOT}/${LANG}/full/chunk-${pad(Math.floor(start / CHUNK))}.json`
  const tag = useSeg ? `seg ${start}-${end}` : `chunk ${pad(Math.floor(start / CHUNK))}`
  thunks.push(() => agent(chunkPrompt(tag, start, end, out), {
    label: `${LANG} ${tag} [${start}:${end}]`,
    phase: 'Translate',
    agentType: 'general-purpose',
    ...(MODEL ? { model: MODEL } : {}),
  }))
}
log(`launching ${thunks.length} ${LANG} agents (${useSeg ? 'ranges' : 'size ' + CHUNK}${MODEL ? ', model ' + MODEL : ''})`)
const results = await parallel(thunks)
const ok = results.filter(Boolean).length
log(`done: ${ok}/${thunks.length} agents returned`)
return { launched: thunks.length, returned: ok, results }
