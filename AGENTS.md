# aliexpress-cli 開発メモ

AliExpress の検索結果を CLI / デスクトップウインドウで絞り込むツール。仕様と使い方は
README.md を参照。ここには開発上の判断と注意点だけを書く。

## 言語の方針

public リポジトリなので、**README・コードコメント・UI 文字列・エラーメッセージはすべて英語**。
**この AGENTS.md と `_issues/` だけは日本語**。

「よりどり」は AliExpress 自身の英語 UI (`b_locale=en_US`) では **"100-yen Shop"** と表示される
(旧称「百円ショップ」の直訳)。コード・UI・README ではこの英語名を使う (`hundred_yen_shop`,
`--100-yen-shop`)。`--yoridori` は CLI のエイリアスとして残している。日本語の「よりどり」を
書いてよいのは、README でエイリアスの由来を説明する 1 箇所と、`skills/` の SKILL.md の
description とフラグ表だけ (ユーザーが日本語で「よりどり」と言ったときにエージェントが
`--100-yen-shop` に結びつけられるようにするため)。

## 設計の要点

- **コアは UI を持たない。** `aliexpress-core` が URL 組み立て・取得・パース・フィルター・
  類似除去・スコアリングを担当し、CLI とウインドウ (`crates/aliexpress-cli`) は
  `aliexpress_core::search` を呼んで結果を表示するだけ。AliExpress 固有の知識を
  フロント側に足さないこと。二重化すると仕様の追従が破綻する。
- **検索ページの HTML に埋め込まれた JSON を読む** (`parse.rs`)。
  `window._dida_config_._init_data_= { data: {...} }` の `data:` の後ろが JSON で、
  `serde_json` のストリームデシリアライザで先頭 1 値だけ読む。HTML の class 名は
  難読化されていて当てにならないが、この JSON はフロントエンドが描画に使うものなので
  項目名が安定していると期待できる (2026-09 時点の観測。壊れたら `tests/fixtures/search.html`
  を実ページから取り直す)。
  `_init_data_` という文字列は他のスクリプトにも現れるので、`= { data: {` の形まで
  照合してから読む。
- **100-yen Shop (よりどり) の判定は `rainbow.rainbowType == "channelProduct"` かつ
  `rainbow.url` にチャネル ID `/ssr/300000512/` を含むこと。** リボンの `title` はロケールで変わる
  (`よりどり` / `100-yen Shop`) が URL は変わらない。観測したリボンは `channelProduct` と
  `buyFree` (任意の 1 点でおまけ) の 2 種。`channelProduct` が他のチャネル商品にも使われる
  可能性を否定できないので、チャネル ID まで見る。
- **価格の無いカードは捨てる。** 60 件中 2 件ほど `prices` の無いカード (バナー扱いの
  item) が混ざる。商品ではないので `product()` が `None` を返す。
- **ブラウザの UA と `aep_usuc_f` Cookie を送る。** UA が無いと bot チェックページが返る。
  Cookie が無いと地域・通貨はサーバー側の既定 (接続元 IP 由来と思われる) になるので、
  `--site` の通り (`site=jpn&c_tp=JPY&region=JP&b_locale=ja_JP` 等) を明示する。
  US サイトは `site=glo&c_tp=USD&region=US&b_locale=en_US` で USD になることを curl で確認済み。
  Cookie はユーザー指定のものを先に置く。同名 Cookie はサーバーが先頭を採る。
  UA は agent レベルにも設定している。プロキシへの CONNECT に ureq 既定の
  `User-Agent: ureq/x.y.z` が付くのを避けるため (Claude Code サンドボックスのプロキシは
  それだと応答しないことがあった)。
- **ページ間は 1.2 秒空ける** (`PAGE_PAUSE`)。人が読む速さに合わせるため。
- **送料額は取れない。** 検索結果 JSON に無く、商品詳細ページも静的 HTML には持たない
  (署名付きの mtop API が要る)。`--free-shipping` はサーバー側フィルター
  (`selectedSwitches=filterCode:freeshipping`) に任せている。
- **価格帯はサーバーとローカルの両方で絞る。** サーバーの `minPrice`/`maxPrice` は
  商品の全バリエーションに対して効くので、表示価格が範囲外のカードも返ってくる。
- **同じ商品 ID の重複は常に落とす** (`reduce`)。サーバーはページをまたいで同じ商品を
  数件返す。
- **類似除去のグループ化は spu_id → pic_group_id → タイトル bigram Jaccard (0.6)。**
  `spu_id` は入らない商品も多く (`"-1"` は無し)、`pic_group_id` の `"0"` は「無し」。
  1 件が複数グループに該当したらグループごと併合する (A と B が別グループ、C が両方に
  該当する順で来ることがある)。閾値 0.6 は実データ (usb c cable, 98 件) の全ペアを
  目視して決めた: 0.6 以上は「3 本/4 本/5 本セット」や同一ケーブルの別出品だけで、
  0.56 以下は別商品だった。
- **類似除去のあとは価格順・販売数順をローカルで掛け直す。** グループの代表が後から来た
  出品に置き換わると、サーバーの並び順が崩れるため。
- **value スコアは `(rating/5)^2 * ln(1+sales) / price`。** 評価か販売数が無ければ 0
  (根拠が無い商品は末尾)。式は README にも書いてあるので変えたら両方直す。

## GUI (Tauri)

- runandlog と同じ構成: `gui` は cargo feature (既定 ON)、`tauri` をライブラリとして使い、
  `build.rs` で `tauri-build` を呼ぶ。`cargo-tauri` CLI は不要。UI は素の HTML/JS
  (`withGlobalTauri: true`)、innerHTML 禁止 (商品タイトルは AliExpress 由来のテキスト)。
- **結果グリッドは `grid-auto-rows: max-content`。** これが無いと WebKit は、flex item である
  グリッドの確定した高さに合わせて行を縮め、`overflow: hidden` のカード (最小高さ 0) が
  ウインドウの 1/3 の高さに切られて価格やタイトルが見えなくなる。Safari でページ単体を開くと
  再現しない (1 行に収まる件数だと縮まない) ので、実ウインドウで 12 件以上出して確認すること。
- **`--web` は CLI 側で検索してから結果をウインドウに渡す** (`gui::run` の `preset`)。
  ウインドウは `initial_form` で 1 回だけそれを受け取り (`Mutex<Option<Results>>::take`)、
  リロード時は自分で検索し直す。`--gui` はフォームから開き、キーワードがあれば自分で検索する。
  結果の描画経路は両方とも `render()` で同じ。
- **検索は `spawn_blocking` に投げる。** 複数ページ取得は数秒かかる。同時実行は 1 本
  (`busy` フラグ)。
- **商品ページを開くのは Rust 側の `open_product` コマンド。** `tauri_plugin_opener::open_url`
  を呼ぶが、検索中サイトの `/item/<数字>.html` 以外は拒否する。ウインドウに
  opener の capability を与えない (与えると任意 URL を開ける)。
- CSP は `img-src https:` を許可している。商品画像を aliexpress-media から直接読むため。
- capabilities は空 (`capabilities/default.json`)。自前コマンドは capability 無しで通る。

**Claude Code サンドボックスではウインドウの目視確認はできない** (WindowServer に繋がらず、
`screencapture` も失敗する)。ビルドと `cargo test` が通ることまでを担保とし、実機確認は
ユーザーに委ねる。

Windows は未対応。`tauri-build` が Windows では `icons/icon.ico` を要求するが用意していない
(この環境ではビルドも検証もできない)。

## エージェントスキル

`skills/aliexpress-search/SKILL.md` は `npx skills add ytyng/aliexpress-cli` で配布する
エージェント向けの使い方。CLI のフラグや出力形式を変えたら、README と一緒にここも直す。
書式は https://github.com/vercel-labs/skills (frontmatter の `name` / `description` が必須)。

## サンドボックスでのビルド

`~/.cargo/registry` に書けないので、`CARGO_HOME=$TMPDIR/cargo-home cargo build` のように
書ける場所を指す。依存は再ダウンロードされる。

## アプリアイコン

`crates/aliexpress-cli/icons/icon.png` は `scripts/generate-icon.py` で生成する
(runandlog のスクリプトの派生。macOS テンプレートの寸法はそのまま、色をオレンジに、
マークを虫眼鏡にした)。手で描かず、スクリプトを直して再生成すること。
