# aliexpress-cli development notes

A tool that filters AliExpress search results in the CLI / a desktop window. For the specification and usage, see
README.md. This file covers only development decisions and caveats.

## Language policy

This is a public repository, so **the README, code comments, UI strings, error messages, and this AGENTS.md are all in English**.
**Only `_issues/` may be written in Japanese**.

"よりどり" is shown as **"100-yen Shop"** in AliExpress's own English UI (`b_locale=en_US`)
(a literal translation of its former name "百円ショップ"). Code, UI, and the README use this English name (`hundred_yen_shop`,
`--100-yen-shop`). `--yoridori` is kept as a CLI alias. Apart from this file, which quotes it as data, the Japanese word "よりどり"
may be written only in the one place in the README that explains where the alias comes from, and in the
description and flag table of the SKILL.md under `skills/` (so that when a user says "よりどり" in Japanese, an agent can
map it to `--100-yen-shop`).

## Key design points

- **The core has no UI.** `aliexpress-core` handles URL construction, fetching, parsing, filtering,
  near-duplicate removal, and scoring; the CLI and the window (`crates/aliexpress-cli`) only
  call `aliexpress_core::search` and display the results. Do not add AliExpress-specific knowledge
  to the front end. Duplicating it would make keeping up with spec changes break down.
- **Read the JSON embedded in the search page HTML** (`parse.rs`).
  What follows `data:` in `window._dida_config_._init_data_= { data: {...} }` is JSON, and
  only the first value is read with `serde_json`'s stream deserializer. The HTML class names are
  obfuscated and unreliable, but this JSON is what the front end uses for rendering, so
  the field names can be expected to be stable (observed as of 2026-09; if it breaks, re-capture `tests/fixtures/search.html`
  from a real page).
  The string `_init_data_` also appears in other scripts, so match all the way to the form `= { data: {`
  before reading.
- **A product is 100-yen Shop (よりどり) when `rainbow.rainbowType == "channelProduct"` and
  `rainbow.url` contains the channel ID `/ssr/300000512/`.** The ribbon `title` changes with the locale
  (`よりどり` / `100-yen Shop`), but the URL does not. The ribbons observed are of two kinds, `channelProduct` and
  `buyFree` (a free extra with any one item). It cannot be ruled out that `channelProduct` is also used for other channel products,
  so the channel ID is checked as well.
- **Cards without a price are discarded.** About 2 out of 60 cards have no `prices` (items treated as
  banners). They are not products, so `product()` returns `None`.
- **Send a browser UA and the `aep_usuc_f` cookie.** Without a UA, a bot check page is returned.
  Without the cookie, the region and currency fall back to the server-side default (apparently derived from the client IP), so
  set them explicitly according to `--site` (`site=jpn&c_tp=JPY&region=JP&b_locale=ja_JP`, etc.).
  For the US site, `site=glo&c_tp=USD&region=US&b_locale=en_US` was confirmed with curl to give USD.
  User-specified cookies are placed first. For cookies with the same name, the server takes the first one.
  The UA is also set at the agent level. This avoids ureq's default
  `User-Agent: ureq/x.y.z` being attached to the CONNECT to the proxy (the Claude Code sandbox proxy
  sometimes did not respond with that).
- **Wait 1.2 seconds between pages** (`PAGE_PAUSE`). To match the speed at which a person reads.
- **The shipping fee cannot be obtained.** It is not in the search result JSON, and the product detail page does not have it in static HTML either
  (a signed mtop API is required). `--free-shipping` relies on the server-side filter
  (`selectedSwitches=filterCode:freeshipping`).
- **The price range is filtered both on the server and locally.** The server's `minPrice`/`maxPrice` apply to
  all variations of a product, so cards whose displayed price is outside the range are also returned.
- **Duplicates with the same product ID are always dropped** (`reduce`). The server returns the same product
  several times across pages.
- **Grouping for near-duplicate removal is spu_id → pic_group_id → title bigram Jaccard (0.6).**
  Many products have no `spu_id` (`"-1"` means none), and `"0"` for `pic_group_id` means "none".
  If one item matches multiple groups, the groups are merged (items can arrive in an order where A and B are in separate groups and C matches
  both). The threshold 0.6 was chosen by eyeballing every pair in real data (usb c cable, 98 items):
  0.6 and above were only "sets of 3/4/5" or separate listings of the same cable,
  and 0.56 and below were different products.
- **After near-duplicate removal, sorting by price / sales is reapplied locally.** When a group's representative is replaced by a listing
  that arrived later, the server's ordering breaks.
- **The value score is `(rating/5)^2 * ln(1+sales) / price`.** If the rating or sales count is missing, it is 0
  (products without evidence go to the end). The formula is also written in the README, so if you change it, fix both.

## GUI (Tauri)

- Same setup as runandlog: `gui` is a cargo feature (on by default), `tauri` is used as a library, and
  `build.rs` calls `tauri-build`. The `cargo-tauri` CLI is not needed. The UI is plain HTML/JS
  (`withGlobalTauri: true`), and innerHTML is forbidden (product titles are text that comes from AliExpress).
- **The result grid uses `grid-auto-rows: max-content`.** Without it, WebKit shrinks the rows to fit the definite height of
  the grid, which is a flex item, and the `overflow: hidden` cards (minimum height 0) are
  cut to 1/3 of the window height so the price and title become invisible. It does not reproduce when the page is opened on its own in Safari
  (it does not shrink when the items fit in one row), so check in the real window with 12 or more results.
- **`--web` searches on the CLI side and then passes the results to the window** (`preset` of `gui::run`).
  The window receives it only once in `initial_form` (`Mutex<Option<Results>>::take`),
  and on reload it searches again by itself. `--gui` opens from the form and searches by itself if there is a keyword.
  Both use the same rendering path, `render()`.
- **Searches are handed to `spawn_blocking`.** Fetching multiple pages takes several seconds. Only one runs at a time
  (`busy` flag).
- **Product pages are opened by the Rust-side `open_product` command.** It calls `tauri_plugin_opener::open_url`,
  but rejects anything other than `/item/<digits>.html` on the site being searched. Do not give the window
  the opener capability (that would let it open arbitrary URLs).
- The CSP allows `img-src https:`. This is to load product images directly from aliexpress-media.
- capabilities are empty (`capabilities/default.json`). Our own commands go through without a capability.

**The window cannot be checked visually in the Claude Code sandbox** (it cannot connect to WindowServer, and
`screencapture` also fails). What is guaranteed is that the build and `cargo test` pass; checking on a real machine is
left to the user.

Windows is not supported. `tauri-build` requires `icons/icon.ico` on Windows, but it is not provided
(it cannot be built or verified in this environment).

## Agent skill

`skills/aliexpress-search/SKILL.md` is the usage guide for agents, distributed with `npx skills add ytyng/aliexpress-cli`.
If you change the CLI flags or output format, fix it here together with the README.
The format follows https://github.com/vercel-labs/skills (`name` / `description` in the frontmatter are required).

## Release (`.github/workflows/release.yml` + `test.yml` + `scripts/release.sh`)

Same setup as runandlog / pullkit (cyberneura). The detailed design rationale is in the `release-rust-cli` skill.

- **The version in main's Cargo.toml decides the release.** `release.yml` runs on every push to main,
  and `plan` calls `GET /releases/tags/v<version>`; if 404, it runs `test` → `build` →
  `release`. If 200, it does nothing. Anything else (rate limit / outage) is treated as undeterminable and
  fails (reading it as unpublished would publish an already published version twice). It does not look at the diff (idempotent).
  It is not narrowed with `paths:` (so that a failed release can be retried by "fix the cause and push").
- **`scripts/release.sh [patch|minor|major]`** bumps the workspace version, the
  dependency version of `aliexpress-core`, and the version in `crates/aliexpress-cli/tauri.conf.json` together,
  makes Cargo.lock follow with `cargo update --workspace`, and just commits and pushes.
  The push is the trigger, so do not bump the version again after a failure.
- **Tests call `test.yml` via `workflow_call`.** PRs run the same definition. The matrix is
  the same as build (macOS uses the default features = including the GUI, Linux uses `--no-default-features`).
  fmt / clippy (`-D warnings`) / test are the gatekeepers, so code that does not pass clippy is not released even if it
  lands on main.
- **The artifacts are two tar.gz files: macOS (aarch64, including the GUI, Developer ID signed + notarized) and Linux (x86_64, CLI only,
  dynamically linked against glibc).** The archive name is `aliexpress-cli-v<version>-<target>.tar.gz`,
  and the directory inside contains the `aliexpress` binary plus README, LICENSE, and THIRD-PARTY-NOTICES.txt.
  The Homebrew cask's `binary` spells out this directory name, so if you rename it, fix the tap side too.
- **A bare binary cannot be stapled**, so the notarization ticket stays on Apple's side and Gatekeeper fetches it online.
  Distributing via a cask requires signing + notarization because a quarantine attribute is attached (not needed for a formula).
- **The Release is created as a draft and published only after counting the assets (2).** The draft of a failed run is
  deleted by id and recreated by the next run (drafts cannot be looked up by tag, so they are found from the list).
- **The Homebrew cask is fetched hourly by the tap side (`ytyng/homebrew-tap`).** Do not push from here to the tap
  (so as not to distribute a token that can write to the tap). After the first Release is published, write the cask to the tap by hand
  once, and from then on it follows automatically.
- The signing secrets (APPLE_*) are registered. If any are missing, build fails at the very beginning.
- Every `uses:` is pinned to a commit SHA (because there is a job that handles certificates). For `dtolnay/rust-toolchain`,
  pick a SHA from the master history.

## License notices (LICENSE / THIRD-PARTY-NOTICES.txt)

- **When adding or upgrading dependencies, rerun `scripts/generate-third-party-notices.sh` and commit the result.**
  `crates/aliexpress-cli/tests/third_party_notices.rs` checks that direct dependencies are listed with their Cargo.lock version
  and that the listed crates exist in Cargo.lock, so the tests fail if you forget.
  Dependabot PRs do not update the notices either, so regenerate them on the same branch before merging.
- Generation uses cargo-about (`cargo install cargo-about --locked --features cli`) and jq. The configuration is
  `crates/aliexpress-cli/about.toml`, and the format is `about.hbs` (the same format as cyberneura/astragal).
- The list is **for the macOS build (default features = including the GUI)**. The Linux build
  (`--no-default-features`) is a subset of it, so the script checks that every Linux-side crate is
  listed and fails if any are missing.
- To keep our own crates out of the list, both crates have `publish = false` and are excluded with
  `[private] ignore = true` in about.toml. If publishing to crates.io, first decide on an alternative exclusion method to replace
  removing `publish = false`, and how to handle `include_str!("../../../THIRD-PARTY-NOTICES.txt")` (outside the package).
- Do not widen `accepted` on your own. If anything GPL / LGPL / AGPL-family comes in, the distribution terms change.
  `CDLA-Permissive-2.0` is there for webpki-roots (ureq → rustls root certificates).
- They are shown by `aliexpress --license` (LICENSE + notices to stdout) and by "Third-Party Licenses" right below About in the window's app menu
  (macOS; Help elsewhere) (`app_menu` /
  `show_licenses` in `gui.rs`, `ui/licenses.html`). The licenses window only calls our own command
  `third_party_notices`, so no capability has been added.
- The release tar.gz also includes LICENSE and THIRD-PARTY-NOTICES.txt.

## Building in the sandbox

`~/.cargo/registry` is not writable, so point to a writable location, as in
`CARGO_HOME=$TMPDIR/cargo-home cargo build`. Dependencies are downloaded again.

## App icon

`crates/aliexpress-cli/icons/icon.png` is generated with `scripts/generate-icon.py`
(derived from runandlog's script; the macOS template dimensions are kept as-is, the color is changed to orange,
and the mark is a magnifying glass). Do not draw it by hand; fix the script and regenerate.
