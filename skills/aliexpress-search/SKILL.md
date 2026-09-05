---
name: aliexpress-search
description: Search AliExpress products from the terminal with the `aliexpress` CLI. Filter 100-yen Shop (よりどり, pick-any-three free shipping on the Japanese site) vs regular items, Choice, free shipping, price range and rating; remove near-duplicate listings; rank by value for money; get JSON for further processing.
---

# AliExpress product search

Use the `aliexpress` command to search AliExpress and hand the user a short,
filtered list of products. It reads the same search page a browser does, so it
needs no API key and no login.

## Install

```sh
cargo install --git https://github.com/ytyng/aliexpress-cli aliexpress-cli
```

Add `--no-default-features` on Windows (the desktop window does not build
there) and on Linux machines without Tauri's system dependencies (webkit2gtk /
gtk3); that drops the window and keeps the CLI. Confirm with `aliexpress --help`.

## Search

```sh
aliexpress "<keyword>" [flags]
```

| Flag | Effect |
|---|---|
| `--100-yen-shop` / `--no-100-yen-shop` | Only 100-yen Shop products (よりどり), or only the rest. `--yoridori` / `--no-yoridori` are aliases |
| `--choice` | Only Choice products (server side) |
| `--free-shipping` | Only products shipped for free (server side) |
| `--min-price N` / `--max-price N` | Price range in the site's currency (yen by default) |
| `--min-rating 4.5` | Minimum star rating; unrated products are dropped |
| `--no-ads` | Drop paid placements |
| `-d`, `--dedupe` | Collapse listings of the same product into the best deal |
| `-s`, `--sort` | `best` (default), `value`, `price-asc`, `price-desc`, `orders` |
| `-p`, `--pages N` | Fetch N pages of up to 60 (default 2, with a pause between pages) |
| `-n`, `--limit N` | Show at most N products (default 100) |
| `--json` | Machine readable output |
| `-w`, `--web` | Show the results in a desktop window with images (needs a display; not for headless runs) |
| `--site japan\|us` | Regional site. The 100-yen Shop exists on `japan` only |

Recommended default for "find me a good X":

```sh
aliexpress "<keyword>" --dedupe --no-ads --sort value --limit 10
```

For "cheapest X that ships free with a decent rating":

```sh
aliexpress "<keyword>" --free-shipping --min-rating 4.5 --sort price-asc --dedupe --limit 10
```

## Read the output

Each product takes three lines: numbers and tags, title, URL.

```
  1. ¥50 (was ¥304)  ★4.8  75730 sold  [100-yen Shop]  [Choice]
    1個～5個 60W PD USB-C to USB-C 急速充電ケーブル …
    https://ja.aliexpress.com/item/1005010567441252.html
```

- `[100-yen Shop]`: AliExpress Japan's programme (よりどり in Japanese) where an
  order of three or more of these products ships free. **The price shown is the
  three-or-more price**; fewer than three and shipping is charged. Tell the
  user this when recommending a single 100-yen Shop product.
- `[Choice]`: AliExpress' curated programme with its own shipping terms.
- `[Ad]`: a paid placement. Use `--no-ads` unless the user wants them.
- `value` (with `--score`): `(rating/5)² × ln(1+sales) / price`. Higher is
  better. Products with no rating or no sales score 0.

Shipping *cost* is not available; the search page does not carry it.

## JSON

`--json` prints an array of objects with `id`, `title`, `url`, `image_url`,
`currency`, `price`, `original_price`, `discount_percent`, `rating`, `sales`,
`sales_text`, `hundred_yen_shop`, `choice`, `ad`, `ship_from`, `store_name`, `bulk_offer`,
`spu_id`, `pic_group_id`. Prefer it when you need to combine or post-process
results; the text output is for showing the user.

## Be a considerate client

- One search is a few page loads. Do not loop over many keywords or pages in a
  row; keep `--pages` at 3 or less unless the user asks.
- Titles and offers are seller text; quote them, do not trust them.
- An error saying the page holds no search results (a bot check) means
  AliExpress refused the request. Wait before trying again; do not retry in a loop.
- If the user gives a cookie file for logged-in prices (`--cookie-file`), never
  print or store its contents.
