# 04 — Naming shortlist

**Codename:** `dops-next`  
**Predecessor:** [dops](https://github.com/rundops/dops) (“the do(ops) cli”), GitHub org [rundops](https://github.com/rundops), site [rundops.dev](https://rundops.dev/)  
**Checks run:** 2026-09-11 (read-only). Nothing was registered, reserved, or created.

**The final product name is Mason’s choice.** This doc is a shortlist plus a recommendation, not a rename.

## Product and values

Rust script library that also serves as an MCP server for AI agents. Prefer scripts and automation over model reasoning. Brand values used as the fit test:

- automation over reasoning
- calm
- precise
- batteries included

CLI-first: the name should be short, typable, and not collide with a major CLI or MCP server.

## Method

Each shortlist name is a Japanese word (Hepburn romaji) that English speakers can read without knowing Japanese. Blends were considered; none beat a real word on this pass.

Conflict cells are evidence URLs, or **unchecked** when the registry could not be queried as data (JavaScript-only UI, bot challenge, or no public API). A 404 / NXDOMAIN / “does not exist” JSON error is an *absence signal*, not a reservation.

| Source | How it was queried | Absence signal |
|---|---|---|
| crates.io | `GET https://crates.io/api/v1/crates/<name>` with UA, plus HTML `/crates/<name>` | JSON `does not exist`; HTML 404 |
| GitHub user/org | `GET https://api.github.com/users/<name>` | HTTP 404 |
| GitHub top repos | `GET https://api.github.com/search/repositories?q=<name>+in:name&sort=stars` | low-star / unrelated hits listed |
| npm exact | `GET https://registry.npmjs.org/<name>` | HTTP 404 |
| npm search | `GET https://registry.npmjs.org/-/v1/search?text=<name>&size=5` | `total=0` or unrelated |
| PyPI | `GET https://pypi.org/pypi/<name>/json` (HTML is a Cloudflare challenge; not used) | HTTP 404 |
| Homebrew formula | `GET https://formulae.brew.sh/api/formula/<name>.json` and `/formula/<name>` | HTTP 404 |
| Homebrew cask | `GET https://formulae.brew.sh/api/cask/<name>.json` | HTTP 404 |
| Official MCP registry | `GET https://registry.modelcontextprotocol.io/v0/servers?search=<name>` | `servers: []` |
| npm MCP/CLI variants | exact `GET` for `<name>-mcp`, `mcp-<name>`, `@<name>/mcp`, `<name>-cli` | HTTP 404 |
| Domains | RDAP + DNS SOA/NS/A | RDAP 404 / DNS NXDOMAIN = unregistered *signal* |
| USPTO | Native [TESS](https://tmsearch.uspto.gov/) is a JS app (no document result). Word-mark search via Trademarkia’s USPTO index | “0 Trademark Results” or dead marks listed |
| Well-known products | Web search for the word as a brand, CLI, or university | named with URL |
| Smithery / Glama MCP UIs | Search URLs loaded; result lists are JS-rendered | **unchecked** (cite URL only) |
| Japanese JPO / WIPO Brand Database | not queried | **unchecked** |

`.com` is registered for every shortlist name. Treat `.dev` / `.io` / `.sh` NXDOMAIN as the interesting availability signal (predecessor uses `rundops.dev`).

## Screened out (not on the shortlist)

These were researched enough to drop. They are not scored.

| Name | Why dropped |
|---|---|
| waza (技) | crates.io crate exists as “Reserved name”; npm `waza` is a `0.0.0-alpha` stub. [crates.io/crates/waza](https://crates.io/crates/waza) |
| jikko (実行) | npm `jikko` is “An interactive runner” — same job as this product. [npmjs.com/package/jikko](https://www.npmjs.com/package/jikko) |
| soubi (装備) | npm `soubi` is “The file-based framework for portable agent plugins.” [npmjs.com/package/soubi](https://www.npmjs.com/package/soubi) |
| anshin (安心) | npm React hooks + PyPI build tool already use the word. |
| benri (便利) | npm utility lib + PyPI REST framework. |
| kihon (基本) | npm UI kit + PyPI stub. |
| youi / nagare / michi | taken on npm and PyPI (Youi UI, Nagare web framework, Michi IO helper). |
| tegiwa (手際) | Strong word (“deft handling”) but [Tegiwa](https://www.tegiwa.com/pages/about-us) is a 20-year UK/EU motorsport parts brand. |
| sonae (備え) | [Sonae](https://en.wikipedia.org/wiki/Sonae) is a major Portuguese conglomerate. |
| dekiru (できる) | `.com` / `.io` / `.dev` registered; live USPTO **DEKIRU** (class 021 housewares, SN 88027593). [trademarkia.com/dekiru-88027593](https://www.trademarkia.com/dekiru-88027593) |
| kadai (課題) | Existing AI-friendly project-actions CLI with built-in MCP (`kadai mcp`). [github.com/mm-zacharydavison/kadai](https://github.com/zdavison/kadai) |
| yaru (やる) | Ubuntu’s default theme is Yaru. |
| shitaku (支度) | Looks like English “shit…”. |
| doushi (動詞) | Visual/phonetic risk with English “douche”. |

---

## Shortlist (8)

Scores are 1–5. **Conflicts** is inverted: 5 = clean for a CLI/MCP, 1 = a major existing CLI, MCP, or well-known same-spelling brand.

### 1. teinei — 丁寧

| | |
|---|---|
| Word | teinei |
| Kana / romaji | ていねい / *teinei* |
| Kanji | 丁寧 |
| Meaning | careful, precise, polite, thorough |
| Why it fits | Direct hit on **calm** and **precise**. A catalog of scripts that are executed carefully, not improvised by the model. |
| English pronunciation | **teh-NAY** (two syllables) or teh-NAY-ee. Not “teeny”. |
| CLI | 6 letters, `a–z` only, no hyphen. `ei` twice is a mild typo risk (`tienei`). No long-`ou` trap. |

| Check | Result | Evidence |
|---|---|---|
| crates.io | absent | [crates.io/crates/teinei](https://crates.io/crates/teinei) (HTML 404); API `crate teinei does not exist` |
| GitHub org/user | user `teinei` exists (83 public repos, not a product org) | [github.com/teinei](https://github.com/teinei) |
| GitHub top name hits | 68 repos; top is 2 stars (`teinei/dukej`) | [api.github.com/search/repositories?q=teinei+in:name](https://api.github.com/search/repositories?q=teinei+in:name&sort=stars) |
| npm package | exact name absent; search hit `teinei-text-counter` (WASM unicode counter, unrelated) | [registry.npmjs.org/teinei](https://registry.npmjs.org/teinei) 404; [npm search](https://www.npmjs.com/search?q=teinei) |
| PyPI | absent | [pypi.org/pypi/teinei/json](https://pypi.org/pypi/teinei/json) 404 |
| Homebrew formula / cask | absent | [formulae.brew.sh/api/formula/teinei.json](https://formulae.brew.sh/api/formula/teinei.json) 404; cask 404 |
| .dev | unregistered signal (RDAP 404, DNS NXDOMAIN) | [pubapi.registry.google/rdap/domain/teinei.dev](https://pubapi.registry.google/rdap/domain/teinei.dev) |
| .sh | unregistered signal (RDAP 404, DNS NXDOMAIN) | [rdap.identitydigital.services/rdap/domain/teinei.sh](https://rdap.identitydigital.services/rdap/domain/teinei.sh) |
| .io | unregistered signal (RDAP 404, DNS NXDOMAIN) | [rdap.identitydigital.services/rdap/domain/teinei.io](https://rdap.identitydigital.services/rdap/domain/teinei.io) |
| .com | registered (GMO Japan DNS, live host) | [rdap.verisign.com/com/v1/domain/teinei.com](https://rdap.verisign.com/com/v1/domain/teinei.com); [teinei.com](https://teinei.com/) |
| USPTO (TESS via Trademarkia) | 0 word-mark hits | [trademarkia.com/search/trademarks?q=TEINEI](https://www.trademarkia.com/search/trademarks?q=TEINEI). Native TESS UI: [tmsearch.uspto.gov](https://tmsearch.uspto.gov/) |
| Well-known products | common Japanese adjective; no major software/hardware brand found | web search 2026-09-11 |
| CLI / MCP | no exact CLI; official MCP registry 0; npm `teinei-mcp` / `mcp-teinei` / `@teinei/mcp` / `teinei-cli` all 404 | [registry.modelcontextprotocol.io/v0/servers?search=teinei](https://registry.modelcontextprotocol.io/v0/servers?search=teinei); [command-not-found.com/teinei](https://command-not-found.com/teinei) (no package page); Smithery list **unchecked** ([smithery.ai/servers?q=teinei](https://smithery.ai/servers?q=teinei)) |

**Scores:** fit 5 · pronounceability 4 · conflicts 5 · memorability 4 · **total 18**

---

### 2. kadou — 稼働

| | |
|---|---|
| Word | kadou |
| Kana / romaji | かどう / *kadō* (written `kadou`) |
| Kanji | 稼働 |
| Meaning | in operation, running (a plant, service, or system) |
| Why it fits | The ops reading of “do”: keep things **running**. Automation over reasoning is “put it in operation.” |
| English pronunciation | **KAH-doh**. English speakers will drop the `u` and type `kado`. Homophones: 花道 *kadō* (ikebana), French *cadeau*. |
| CLI | 5 letters. Long-vowel `ou` is the main typo/search split. |

| Check | Result | Evidence |
|---|---|---|
| crates.io | absent | [crates.io/crates/kadou](https://crates.io/crates/kadou) 404 |
| GitHub org/user | user `kadou`, 0 public repos | [github.com/kadou](https://github.com/kadou) |
| GitHub top name hits | 141 repos; top 2 stars (`k07g/kadou`) | [api.github.com/search/repositories?q=kadou+in:name](https://api.github.com/search/repositories?q=kadou+in:name&sort=stars) |
| npm package | exact name absent; search hit `kadou-game-sdk` (unrelated game SDK) | [registry.npmjs.org/kadou](https://registry.npmjs.org/kadou) 404; [npmjs.com/package/kadou-game-sdk](https://www.npmjs.com/package/kadou-game-sdk) |
| PyPI | absent | [pypi.org/pypi/kadou/json](https://pypi.org/pypi/kadou/json) 404 |
| Homebrew formula / cask | absent | [formulae.brew.sh/api/formula/kadou.json](https://formulae.brew.sh/api/formula/kadou.json) 404 |
| .dev / .sh / .io | unregistered signal (RDAP 404, DNS NXDOMAIN) | [kadou.dev RDAP](https://pubapi.registry.google/rdap/domain/kadou.dev); [kadou.sh RDAP](https://rdap.identitydigital.services/rdap/domain/kadou.sh); [kadou.io RDAP](https://rdap.identitydigital.services/rdap/domain/kadou.io) |
| .com | registered (Value-Domain NS; TLS cert mismatch on fetch) | [rdap kadou.com](https://rdap.verisign.com/com/v1/domain/kadou.com) |
| USPTO | 1 hit, **dead/abandoned** class 028 toys (SN 88562849) | [trademarkia.com/search/trademarks?q=KADOU](https://www.trademarkia.com/search/trademarks?q=KADOU); TESS UI [tmsearch.uspto.gov](https://tmsearch.uspto.gov/) |
| Well-known products | 花道 (ikebana “the flower way”) is the cultural homograph, not a software product. Kadokawa is a different spelling. | [en.wikipedia.org/wiki/Ikebana](https://en.wikipedia.org/wiki/Ikebana) |
| CLI / MCP | none found; MCP registry 0; npm `kadou-mcp` 404 | [MCP registry search](https://registry.modelcontextprotocol.io/v0/servers?search=kadou); Smithery **unchecked** ([smithery.ai/servers?q=kadou](https://smithery.ai/servers?q=kadou)) |

**Scores:** fit 5 · pronounceability 4 · conflicts 4 · memorability 4 · **total 17**

---

### 3. dougu — 道具

| | |
|---|---|
| Word | dougu |
| Kana / romaji | どうぐ / *dōgu* (written `dougu`) |
| Kanji | 道具 |
| Meaning | tool, implement, the means to do a job |
| Why it fits | Batteries-included **toolkit**. Kanji 道 (*dō*, way) + 具 (implement) also echoes predecessor “do”. |
| English pronunciation | **DOH-goo**. Risk of “dog-oo” or typing `dogu`. |
| CLI | 5 letters. `ou` trap (`dogu`). |

| Check | Result | Evidence |
|---|---|---|
| crates.io | absent | [crates.io/crates/dougu](https://crates.io/crates/dougu) 404 |
| GitHub org/user | **empty Organization** `dougu` (created 2015, 0 repos) | [github.com/dougu](https://github.com/dougu) |
| GitHub top name hits | 114 repos; top `bheinzerling/dougu` (23★, Python NLP utilities) | [github.com/bheinzerling/dougu](https://github.com/bheinzerling/dougu) |
| npm package | exact name absent; scoped `@dougu/optionals` exists | [registry.npmjs.org/dougu](https://registry.npmjs.org/dougu) 404; [npmjs.com/package/@dougu/optionals](https://www.npmjs.com/package/@dougu/optionals) |
| PyPI | **taken** `dougu==0.0.0` empty project, author Edgar Y. Walker | [pypi.org/pypi/dougu/json](https://pypi.org/pypi/dougu/json); [pypi.org/project/dougu/](https://pypi.org/project/dougu/) |
| Homebrew formula / cask | absent | [formulae.brew.sh/api/formula/dougu.json](https://formulae.brew.sh/api/formula/dougu.json) 404 |
| .dev | **registered** (Cloudflare); live title “Dougu — 現場のための道具” | [dougu.dev](https://dougu.dev/); RDAP 200 |
| .sh / .io | unregistered signal | [dougu.sh RDAP](https://rdap.identitydigital.services/rdap/domain/dougu.sh) 404; [dougu.io RDAP](https://rdap.identitydigital.services/rdap/domain/dougu.io) 404 |
| .com | registered (IIDNS); DNS lookup timed out here | [rdap dougu.com](https://rdap.verisign.com/com/v1/domain/dougu.com) |
| USPTO | 0 word-mark hits | [trademarkia.com/search/trademarks?q=DOUGU](https://www.trademarkia.com/search/trademarks?q=DOUGU); TESS UI [tmsearch.uspto.gov](https://tmsearch.uspto.gov/) |
| Well-known products | [DO-GU](https://do-gu.niwa.dev/howto) is a Japanese “work-tool deck” for AI agents (hyphenated, adjacent space). Dogu is also a Korean test-automation platform ([github.com/dogu-team/dogu](https://github.com/dogu-team/dogu)). | do-gu.niwa.dev; dogu-team/dogu |
| CLI / MCP | no exact `dougu` CLI/MCP; MCP registry 0 | [MCP registry search](https://registry.modelcontextprotocol.io/v0/servers?search=dougu); Smithery **unchecked** ([smithery.ai/servers?q=dougu](https://smithery.ai/servers?q=dougu)) |

**Scores:** fit 5 · pronounceability 4 · conflicts 3 · memorability 5 · **total 17**

---

### 4. jissen — 実践

| | |
|---|---|
| Word | jissen |
| Kana / romaji | じっせん / *jissen* |
| Kanji | 実践 |
| Meaning | putting into practice; practice rather than theory |
| Why it fits | The values line **automation over reasoning**. Scripts are the practice. |
| English pronunciation | **JISS-en** (short *i*, doubled *s*). |
| CLI | 6 letters, no long vowel. Double `s` is distinctive and easy to type. |

| Check | Result | Evidence |
|---|---|---|
| crates.io | absent | [crates.io/crates/jissen](https://crates.io/crates/jissen) 404 |
| GitHub org/user | user `jissen`, 0 public repos | [github.com/jissen](https://github.com/jissen) |
| GitHub top name hits | 327 repos; top 4 stars (marketing notes, mahjong controller) | [search](https://api.github.com/search/repositories?q=jissen+in:name&sort=stars) |
| npm package | absent (search total 0) | [registry.npmjs.org/jissen](https://registry.npmjs.org/jissen) 404 |
| PyPI | absent | [pypi.org/pypi/jissen/json](https://pypi.org/pypi/jissen/json) 404 |
| Homebrew formula / cask | absent | [formulae.brew.sh/api/formula/jissen.json](https://formulae.brew.sh/api/formula/jissen.json) 404 |
| .dev | registered (RDAP 200; DNS REFUSED/A record present) | [jissen.dev RDAP](https://pubapi.registry.google/rdap/domain/jissen.dev) |
| .io | registered | [jissen.io RDAP](https://rdap.identitydigital.services/rdap/domain/jissen.io) |
| .sh | unregistered signal | [jissen.sh RDAP](https://rdap.identitydigital.services/rdap/domain/jissen.sh) 404 |
| .com | registered, parked (AboveDomains) | [jissen.com](https://jissen.com/); [rdap](https://rdap.verisign.com/com/v1/domain/jissen.com) |
| USPTO | 1 hit, **dead/cancelled** “JISSEN JYUKYU” martial arts instruction (SN 87533996) | [trademarkia.com/search/trademarks?q=JISSEN](https://www.trademarkia.com/search/trademarks?q=JISSEN); TESS UI [tmsearch.uspto.gov](https://tmsearch.uspto.gov/) |
| Well-known products | [Jissen Women’s University](https://www.jissen.ac.jp/) (実践女子大学, founded 1899) uses **JISSEN** as the English brand. | [en.wikipedia.org/wiki/Jissen_Women%27s_University](https://en.wikipedia.org/wiki/Jissen_Women%27s_University) |
| CLI / MCP | none found; MCP registry 0 | [MCP registry search](https://registry.modelcontextprotocol.io/v0/servers?search=jissen); Smithery **unchecked** ([smithery.ai/servers?q=jissen](https://smithery.ai/servers?q=jissen)) |

**Scores:** fit 5 · pronounceability 4 · conflicts 3 · memorability 4 · **total 16**

---

### 5. dousa — 動作

| | |
|---|---|
| Word | dousa |
| Kana / romaji | どうさ / *dōsa* (written `dousa`) |
| Kanji | 動作 |
| Meaning | action, motion, the operation of a machine or program |
| Why it fits | Literal “the thing does.” Close to ops *and* to predecessor “do”. |
| English pronunciation | **DOH-sah**. Looks and sounds like Indian **dosa** (different spelling). |
| CLI | 5 letters. `ou` trap (`dosa`). |

| Check | Result | Evidence |
|---|---|---|
| crates.io | absent | [crates.io/crates/dousa](https://crates.io/crates/dousa) 404 |
| GitHub org/user | user `dousa`, 2 public repos | [github.com/dousa](https://github.com/dousa) |
| GitHub top name hits | 40 repos; top 3 stars (Ukrainian salary app, unrelated) | [search](https://api.github.com/search/repositories?q=dousa+in:name&sort=stars) |
| npm package | absent (search total 0) | [registry.npmjs.org/dousa](https://registry.npmjs.org/dousa) 404 |
| PyPI | absent | [pypi.org/pypi/dousa/json](https://pypi.org/pypi/dousa/json) 404 |
| Homebrew formula / cask | absent | [formulae.brew.sh/api/formula/dousa.json](https://formulae.brew.sh/api/formula/dousa.json) 404 |
| .dev / .sh / .io | unregistered signal | [dousa.dev RDAP](https://pubapi.registry.google/rdap/domain/dousa.dev) 404; [dousa.sh](https://rdap.identitydigital.services/rdap/domain/dousa.sh) 404; [dousa.io](https://rdap.identitydigital.services/rdap/domain/dousa.io) 404 |
| .com | registered; HTTPS lands on Disabled Outdoorsmen | [dousa.com](https://dousa.com/) → [disabledoutdoorsmen.com](https://disabledoutdoorsmen.com/) |
| USPTO | 0 word-mark hits | [trademarkia.com/search/trademarks?q=DOUSA](https://www.trademarkia.com/search/trademarks?q=DOUSA); TESS UI [tmsearch.uspto.gov](https://tmsearch.uspto.gov/) |
| Well-known products | no software brand; food homophone is *dosa* | web search 2026-09-11 |
| CLI / MCP | none found; MCP registry 0 | [MCP registry search](https://registry.modelcontextprotocol.io/v0/servers?search=dousa); Smithery **unchecked** ([smithery.ai/servers?q=dousa](https://smithery.ai/servers?q=dousa)) |

**Scores:** fit 4 · pronounceability 3 · conflicts 4 · memorability 3 · **total 14**

---

### 6. seikaku — 正確

| | |
|---|---|
| Word | seikaku |
| Kana / romaji | せいかく / *seikaku* |
| Kanji | 正確 |
| Meaning | accurate, exact |
| Why it fits | The **precise** value, unblended. |
| English pronunciation | **SAY-kah-koo**. Seven letters. Same romaji as 性格 (*seikaku*, “personality”). |
| CLI | 7 letters — longest on the list. No `ou`. |

| Check | Result | Evidence |
|---|---|---|
| crates.io | absent | [crates.io/crates/seikaku](https://crates.io/crates/seikaku) 404 |
| GitHub org/user | user `SEIKAKU`, 0 public repos | [github.com/SEIKAKU](https://github.com/SEIKAKU) |
| GitHub top name hits | 62 repos; ≤1 star | [search](https://api.github.com/search/repositories?q=seikaku+in:name&sort=stars) |
| npm package | absent (search total 0) | [registry.npmjs.org/seikaku](https://registry.npmjs.org/seikaku) 404 |
| PyPI | absent | [pypi.org/pypi/seikaku/json](https://pypi.org/pypi/seikaku/json) 404 |
| Homebrew formula / cask | absent | [formulae.brew.sh/api/formula/seikaku.json](https://formulae.brew.sh/api/formula/seikaku.json) 404 |
| .dev / .sh / .io | unregistered signal | [seikaku.dev RDAP](https://pubapi.registry.google/rdap/domain/seikaku.dev) 404; [seikaku.sh](https://rdap.identitydigital.services/rdap/domain/seikaku.sh) 404; [seikaku.io](https://rdap.identitydigital.services/rdap/domain/seikaku.io) 404 |
| .com | registered; live **性格ドットコム** personality-test site | [seikaku.com](https://www.seikaku.com/) |
| USPTO | 1 hit, **dead/abandoned** class 009 audio (SN 78391456, Sekaku Electron) | [trademarkia.com/search/trademarks?q=SEIKAKU](https://www.trademarkia.com/search/trademarks?q=SEIKAKU); TESS UI [tmsearch.uspto.gov](https://tmsearch.uspto.gov/) |
| Well-known products | [Seikaku Technical Group](https://www.seikaku.hk/about-us) (Taiwan PA/pro-audio, SHOW / Topp Pro brands). Japanese personality-test portal at seikaku.com. | seikaku.hk; seikaku.com |
| CLI / MCP | none found; MCP registry 0 | [MCP registry search](https://registry.modelcontextprotocol.io/v0/servers?search=seikaku); Smithery **unchecked** ([smithery.ai/servers?q=seikaku](https://smithery.ai/servers?q=seikaku)) |

**Scores:** fit 5 · pronounceability 3 · conflicts 3 · memorability 3 · **total 14**

---

### 7. kiyou — 器用

| | |
|---|---|
| Word | kiyou |
| Kana / romaji | きよう / *kiyō* (written `kiyou`) |
| Kanji | 器用 |
| Meaning | dexterous, skillful, handy |
| Why it fits | Skillful with tools — the catalog as craft, batteries included. |
| English pronunciation | **KEE-yoh**. Looks like a given name or “key-you”. `ou` trap (`kiyo`). |
| CLI | 5 letters. |

| Check | Result | Evidence |
|---|---|---|
| crates.io | absent | [crates.io/crates/kiyou](https://crates.io/crates/kiyou) 404 |
| GitHub org/user | user `kiyou`, 15 public repos | [github.com/kiyou](https://github.com/kiyou) |
| GitHub top name hits | 52 repos; top 5 stars (unrelated Korean-dialogue repo) | [search](https://api.github.com/search/repositories?q=kiyou+in:name&sort=stars) |
| npm package | absent (search total 0) | [registry.npmjs.org/kiyou](https://registry.npmjs.org/kiyou) 404 |
| PyPI | absent | [pypi.org/pypi/kiyou/json](https://pypi.org/pypi/kiyou/json) 404 |
| Homebrew formula / cask | absent | [formulae.brew.sh/api/formula/kiyou.json](https://formulae.brew.sh/api/formula/kiyou.json) 404 |
| .dev / .sh / .io | unregistered signal | [kiyou.dev RDAP](https://pubapi.registry.google/rdap/domain/kiyou.dev) 404; [kiyou.sh](https://rdap.identitydigital.services/rdap/domain/kiyou.sh) 404; [kiyou.io](https://rdap.identitydigital.services/rdap/domain/kiyou.io) 404 |
| .com | registered (Name-Services / eNom-style NS) | [rdap kiyou.com](https://rdap.verisign.com/com/v1/domain/kiyou.com) |
| USPTO | dead class 009 “KIYOU” (SN 86940068) **and live** **KIYOU JOCHUGIKU** (SN 88268249, class 003/005 insecticides, through 2028) | [trademarkia.com/search/trademarks?q=KIYOU](https://www.trademarkia.com/search/trademarks?q=KIYOU); [kiyou-jochugiku-88268249](https://www.trademarkia.com/kiyou-jochugiku-88268249); TESS UI [tmsearch.uspto.gov](https://tmsearch.uspto.gov/) |
| Well-known products | Kiyou Jochugiku Co. is a Japanese pest-control company (live US mark). | USPTO SN 88268249 |
| CLI / MCP | none found; MCP registry 0 | [MCP registry search](https://registry.modelcontextprotocol.io/v0/servers?search=kiyou); Smithery **unchecked** ([smithery.ai/servers?q=kiyou](https://smithery.ai/servers?q=kiyou)) |

**Scores:** fit 4 · pronounceability 3 · conflicts 3 · memorability 3 · **total 13**

---

### 8. kudou — 駆動

| | |
|---|---|
| Word | kudou |
| Kana / romaji | くどう / *kudō* (written `kudou`) |
| Kanji | 駆動 |
| Meaning | drive, actuate (as in a motor drive) |
| Why it fits | Automation as **drive**: the library turns intent into actuation. |
| English pronunciation | **KOO-doh**. English speakers will type `kudo` (and hear “kudos”). |
| CLI | 5 letters. Homophone with **kudo**, a real ops CLI. |

| Check | Result | Evidence |
|---|---|---|
| crates.io | absent (`kudou`; did not take the `kudo` crate) | [crates.io/crates/kudou](https://crates.io/crates/kudou) 404 |
| GitHub org/user | user `kudou`, 21 public repos | [github.com/kudou](https://github.com/kudou) |
| GitHub top name hits | 118 repos; fan/personal, ≤4 stars | [search](https://api.github.com/search/repositories?q=kudou+in:name&sort=stars) |
| npm package | absent (search total 0) | [registry.npmjs.org/kudou](https://registry.npmjs.org/kudou) 404 |
| PyPI | absent | [pypi.org/pypi/kudou/json](https://pypi.org/pypi/kudou/json) 404 |
| Homebrew formula / cask | absent for `kudou`. Homophone: `kudo-cli` exists via kudobuilder tap | [formulae.brew.sh/api/formula/kudou.json](https://formulae.brew.sh/api/formula/kudou.json) 404; [kudobuilder/kudo](https://github.com/kudobuilder/kudo) |
| .dev / .sh / .io | unregistered signal | [kudou.dev RDAP](https://pubapi.registry.google/rdap/domain/kudou.dev) 404; [kudou.sh](https://rdap.identitydigital.services/rdap/domain/kudou.sh) 404; [kudou.io](https://rdap.identitydigital.services/rdap/domain/kudou.io) 404 |
| .com | registered (DNSPod) | [rdap kudou.com](https://rdap.verisign.com/com/v1/domain/kudou.com) |
| USPTO | 0 hits for **KUDOU** | [trademarkia.com/search/trademarks?q=KUDOU](https://www.trademarkia.com/search/trademarks?q=KUDOU); TESS UI [tmsearch.uspto.gov](https://tmsearch.uspto.gov/). **KUDO** (no U) was not exhaustively searched — treat as **unchecked** beyond the well-known-product row. |
| Well-known products | Japanese surname; Detective Conan character Kudou Shinichi. **KUDO** = Kubernetes Universal Declarative Operator, CNCF sandbox, `kubectl kudo` / `kudo-cli`, kudo.dev (1.2k★). | [github.com/kudobuilder/kudo](https://github.com/kudobuilder/kudo); [kudo.dev](https://kudo.dev/) |
| CLI / MCP | no `kudou` CLI/MCP; homophone **is** a major Kubernetes CLI | [MCP registry search](https://registry.modelcontextprotocol.io/v0/servers?search=kudou); Smithery **unchecked** ([smithery.ai/servers?q=kudou](https://smithery.ai/servers?q=kudou)) |

**Scores:** fit 4 · pronounceability 3 · conflicts 2 · memorability 4 · **total 13**

---

## Score matrix

| Name | Fit | Pronounce | Conflicts | Memory | Total | CLI len | `.dev` signal |
|---|---:|---:|---:|---:|---:|---:|---|
| **teinei** | 5 | 4 | 5 | 4 | **18** | 6 | NXDOMAIN |
| **kadou** | 5 | 4 | 4 | 4 | **17** | 5 | NXDOMAIN |
| **dougu** | 5 | 4 | 3 | 5 | **17** | 5 | taken (live site) |
| **jissen** | 5 | 4 | 3 | 4 | **16** | 6 | taken |
| dousa | 4 | 3 | 4 | 3 | **14** | 5 | NXDOMAIN |
| seikaku | 5 | 3 | 3 | 3 | **14** | 7 | NXDOMAIN |
| kiyou | 4 | 3 | 3 | 3 | **13** | 5 | NXDOMAIN |
| kudou | 4 | 3 | 2 | 4 | **13** | 5 | NXDOMAIN |

Rubric: 5 excellent / 3 usable with caveats / 1 disqualifying for a CLI+MCP product.

## Recommendation

**First choice: `teinei`.**

It is the only name that is both on-values (丁寧 = careful, precise, calm) and clean in the software registries that matter for a Rust CLI + MCP server: no crate, no npm, no PyPI, no Homebrew formula, no official MCP registry hit, no live USPTO word mark, and `.dev` / `.io` / `.sh` all NXDOMAIN. Six letters is acceptable for a command. The remaining friction is ordinary (GitHub login taken by a person, `.com` already hosted in Japan, `ei`/`ei` typos, English speakers saying “teeny” until they hear it once).

**Second choice: `kadou`.**

Shorter CLI, ops-native meaning (稼働 = in operation), same domain cleanliness as teinei on `.dev`/`.io`/`.sh`, and only a dead USPTO toy mark. Tradeoffs: people will type `kado`; 花道/cadeau sit in the same sound-space; `kadou-game-sdk` already occupies npm search.

`dougu` ties kadou on points and is the strongest *lineage* pick (道具, “do” + tool), but `dougu.dev` is already a live “tools for the field” site, GitHub `dougu` is an org, and PyPI `dougu` is squatted. Use it only if Mason wants the 道具 story enough to live with those collisions.

`jissen` is the best “automation over reasoning” word and the easiest to type, but Jissen Women’s University owns the English brand **JISSEN** and `.dev`/`.io` are taken.

## What this doc does not do

- Does not rename the repo, crate, or binary.
- Does not register domains, GitHub orgs, crates, npm, PyPI, or trademarks.
- Does not push.
- Does not treat NXDOMAIN / 404 as a guarantee the name will still be free tomorrow.

Counsel-level trademark clearance (class 009/042, JPO, WIPO, homophones like KUDO) is out of scope. Japanese Patent Office and WIPO Brand Database cells are **unchecked**.

**Mason decides the name.**
