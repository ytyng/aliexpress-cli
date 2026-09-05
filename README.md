# aliexpress-cli

Search AliExpress from the terminal, or from a small desktop window, with the
filters the site does not give you:

- **Yoridori** (よりどり) only, or everything *but* Yoridori. Yoridori is the
  Japanese site's "pick any three, shipping is free" programme.
- Choice only, free shipping only, price range, minimum rating, no ads.
- **Remove similar listings**: the same product sold by twenty stores collapses
  into the best deal.
- **Rank by value**: rating, sales and price combined, so the cheap-and-proven
  products come first.
- A window that shows the product images: `--web` shows the results of the
  search you just typed, `--gui` opens the search form. Click a card to open the
  product page in your browser.

Written in Rust. No account, no API key: it reads the same search page a
browser does.

## Install

```sh
cargo install --path crates/aliexpress-cli
```

The window needs [Tauri's system dependencies](https://tauri.app/start/prerequisites/)
at build time (webkit2gtk / gtk3 on Linux; nothing extra on macOS). Windows is
not supported. For a command line only binary:

```sh
cargo install --path crates/aliexpress-cli --no-default-features
```

## Use

```sh
aliexpress "usb c cable"
aliexpress "usb c cable" --yoridori --sort value --dedupe --pages 3
aliexpress "usb c cable" --no-yoridori --free-shipping --max-price 500 --min-rating 4.5
aliexpress "usb c cable" --json > cables.json
aliexpress "usb c cable" --yoridori --dedupe --web
aliexpress --gui
```

Every product prints as three lines: price, rating, units sold and tags; the
title; the URL.

```
  1. ¥50 (was ¥304)  ★4.8  75730 sold  [Yoridori]  [Choice]
    1個～5個 60W PD USB-C to USB-C 急速充電ケーブル …
    https://ja.aliexpress.com/item/1005010567441252.html
```

| Flag | What it does |
|---|---|
| `--yoridori` / `--no-yoridori` | Keep only Yoridori products, or only the rest. |
| `--choice`, `--free-shipping` | Ask AliExpress for Choice / free shipping products only. |
| `--min-price`, `--max-price` | Price range, in the site's currency (yen by default). |
| `--min-rating 4.5` | Minimum star rating. Unrated products are dropped. |
| `--no-ads` | Drop paid placements. |
| `-d`, `--dedupe` | Collapse listings of the same product into the best deal. |
| `-s`, `--sort` | `best` (default), `value`, `price-asc`, `price-desc`, `orders`. |
| `-p`, `--pages N` | Fetch N result pages (60 products each), with a pause between them. |
| `-n`, `--limit N` | Show at most N products. |
| `--json` | Print the products as JSON. |
| `-w`, `--web` | Search, then show the results in the desktop window instead of printing. |
| `-g`, `--gui` | Open the desktop window on its search form. With a keyword, it searches at once. |
| `--score` | Show the value score next to each product. |
| `--site japan\|us` | Which regional site to search. Yoridori exists on the Japanese one only. |
| `--cookie-file PATH` | Send the cookies in this file. See *Logged in prices*. |

### What "value" means

`value = (rating / 5)² × ln(1 + units sold) / price`

Cheaper, better rated and more sold all raise it. A product with no rating or
no sales scores 0 and sorts last: there is no evidence it is any good.

### What "similar" means

Two listings are the same product when AliExpress groups them itself (a shared
product family id or image set), or when their titles are nearly identical
(character bigram similarity of 0.6 or more). The listing with the best value
score represents the group. A product repeated across pages is always shown
once, with or without `--dedupe`.

### Yoridori prices

The price shown for a Yoridori product is the price when buying three or more
Yoridori products in one order; that is what the site shows on its cards too.
Fewer than three and shipping is charged. Some cards also carry a bulk offer
(`3点以上注文で1点あたり142円`), shown as-is.

Shipping *cost* is not shown: the search page does not carry it. `--free-shipping`
is applied by AliExpress itself.

### Logged in prices

Prices on AliExpress depend on the account (new user deals, coins, coupons).
To see the prices your account sees, put the value of your browser's `Cookie`
header for `ja.aliexpress.com` into a file and pass `--cookie-file`. The tool
sends it as-is with every request, and never writes to the file. It does not log
in, store passwords, or keep a session of its own.

Use this sparingly. Automated access from a logged in account is the kind of
thing AliExpress can act on.

## Terms of use

AliExpress' [Terms of Use](https://terms.alicdn.com/legal-agreement/terms/suit_bu1_aliexpress/suit_bu1_aliexpress202204182115_66077.html)
prohibit "systematic retrieval of Site Content ... to create or compile ... a
collection, compilation, database or directory" by automated means. This tool
fetches the pages of one search at a time, at a person's pace, for that person
to read. Do not point it at a loop. Whether your use is acceptable is between
you and AliExpress; this project makes no claim about it.

## Agent skill

The `skills/` directory holds an [agent skill](https://github.com/vercel-labs/skills)
that teaches a coding agent (Claude Code, Cursor, Codex, ...) how to use this
tool for product searches:

```sh
npx skills add ytyng/aliexpress-cli
```

## Development

```sh
cargo test
cargo build
target/debug/aliexpress "usb c cable" --yoridori
```

Notes for contributors are in `AGENTS.md`.

## License

MIT
