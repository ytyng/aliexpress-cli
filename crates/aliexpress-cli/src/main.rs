//! Entry point of `aliexpress`.

use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use aliexpress_core::{Client, Filter, Kind, SearchOptions, Site, Sort, search};
use clap::{Parser, ValueEnum};

/// Search AliExpress from the terminal.
///
/// Prices are in yen from the Japanese site unless --site says otherwise.
#[derive(Parser, Debug)]
#[command(name = "aliexpress", version, about, long_about = None)]
struct Args {
    /// What to search for. Optional with --gui.
    keyword: Option<String>,

    /// Open the desktop window instead of printing.
    #[arg(short, long, conflicts_with = "web")]
    gui: bool,

    /// Search now, then show the results in the desktop window (images,
    /// click to open the product page) instead of printing them.
    #[arg(short, long)]
    web: bool,

    /// Show only Yoridori products (the Japanese site's pick-any-three, free shipping programme).
    #[arg(long, conflicts_with = "no_yoridori")]
    yoridori: bool,

    /// Show only products that are not Yoridori.
    #[arg(long)]
    no_yoridori: bool,

    /// Ask AliExpress for Choice products only.
    #[arg(long)]
    choice: bool,

    /// Ask AliExpress for products with free shipping only.
    #[arg(long)]
    free_shipping: bool,

    /// Lowest price to show, in the site's currency.
    #[arg(long, value_name = "PRICE")]
    min_price: Option<f64>,

    /// Highest price to show, in the site's currency.
    #[arg(long, value_name = "PRICE")]
    max_price: Option<f64>,

    /// Lowest star rating to show, 0 to 5. Unrated products are dropped.
    #[arg(long, value_name = "STARS")]
    min_rating: Option<f64>,

    /// Drop paid placements.
    #[arg(long)]
    no_ads: bool,

    /// Collapse listings of the same product into the best deal.
    #[arg(short, long)]
    dedupe: bool,

    /// Order of the results.
    #[arg(short, long, value_enum, default_value_t = SortArg::Best)]
    sort: SortArg,

    /// How many result pages to fetch (60 products each).
    #[arg(short, long, default_value_t = 1, value_name = "N")]
    pages: u32,

    /// Show at most this many products.
    #[arg(short = 'n', long, value_name = "N")]
    limit: Option<NonZeroUsize>,

    /// Print the products as JSON instead of text.
    #[arg(long)]
    json: bool,

    /// Show the value score next to each product.
    #[arg(long)]
    score: bool,

    /// Regional site to search.
    #[arg(long, value_enum, default_value_t = SiteArg::Japan)]
    site: SiteArg,

    /// A file holding the value of a Cookie header, sent with every request.
    ///
    /// Paste the cookies of a logged in browser session there to see the
    /// prices your account sees. The tool never writes to this file.
    #[arg(long, value_name = "PATH")]
    cookie_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SortArg {
    /// AliExpress' relevance ranking.
    Best,
    /// Best value for money first (rating, sales and price combined).
    Value,
    PriceAsc,
    PriceDesc,
    /// Most sold first.
    Orders,
}

impl From<SortArg> for Sort {
    fn from(sort: SortArg) -> Self {
        match sort {
            SortArg::Best => Sort::Best,
            SortArg::Value => Sort::Value,
            SortArg::PriceAsc => Sort::PriceAsc,
            SortArg::PriceDesc => Sort::PriceDesc,
            SortArg::Orders => Sort::Orders,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SiteArg {
    /// ja.aliexpress.com, in yen. The only site with Yoridori.
    Japan,
    /// www.aliexpress.com, in US dollars.
    Us,
}

impl From<SiteArg> for Site {
    fn from(site: SiteArg) -> Self {
        match site {
            SiteArg::Japan => Site::japan(),
            SiteArg::Us => Site {
                host: "www.aliexpress.com".to_string(),
                site: "glo".to_string(),
                region: "US".to_string(),
                currency: "USD".to_string(),
                locale: "en_US".to_string(),
            },
        }
    }
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("aliexpress: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let cookie = match &args.cookie_file {
        Some(path) => Some(
            fs::read_to_string(path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?,
        ),
        None => None,
    };
    let client = Client::new(args.site.into(), cookie);
    let options = options(&args);

    if args.gui {
        // The window takes the flags even without a keyword: they fill its form.
        return open_gui(client, options, None);
    }

    if options.keyword.is_empty() {
        return Err("a keyword is needed (or --gui to open the window)".into());
    }
    if args.web && !cfg!(feature = "gui") {
        // Refused before the search, not after: a build without a window
        // cannot show the result, so fetching it would only cost time and
        // hide this message behind any network error.
        return Err(NO_GUI.into());
    }
    let result = search(&client, &options)?;
    if args.web {
        return open_gui(client, options, Some(result));
    }
    if args.json {
        println!("{}", serde_json::to_string_pretty(&result.products)?);
    } else {
        print!("{}", aliexpress_cli::report::render(&result, args.score));
    }
    Ok(())
}

/// The search the flags describe. The keyword is empty when none was given.
fn options(args: &Args) -> SearchOptions {
    let keyword = args.keyword.as_deref().unwrap_or("").trim();
    let kind = if args.yoridori {
        Kind::Yoridori
    } else if args.no_yoridori {
        Kind::Normal
    } else {
        Kind::Any
    };
    SearchOptions {
        keyword: keyword.to_string(),
        pages: args.pages,
        sort: args.sort.into(),
        filter: Filter {
            kind,
            min_price: args.min_price,
            max_price: args.max_price,
            min_rating: args.min_rating,
            exclude_ads: args.no_ads,
        },
        choice: args.choice,
        free_shipping: args.free_shipping,
        dedupe: args.dedupe,
        limit: args.limit.map(NonZeroUsize::get),
    }
}

#[cfg(feature = "gui")]
fn open_gui(
    client: Client,
    initial: SearchOptions,
    preset: Option<aliexpress_core::SearchResult>,
) -> Result<(), Box<dyn std::error::Error>> {
    aliexpress_cli::gui::run(client, initial, preset)?;
    Ok(())
}

/// Built without the `gui` feature the binary still accepts `--gui`, so that the
/// message explains what happened rather than clap reporting an unknown flag.
/// Why `--gui` / `--web` cannot work in a build without the `gui` feature.
const NO_GUI: &str = "this build has no window: it was built without the `gui` feature";

#[cfg(not(feature = "gui"))]
fn open_gui(
    _: Client,
    _: SearchOptions,
    _: Option<aliexpress_core::SearchResult>,
) -> Result<(), Box<dyn std::error::Error>> {
    Err(NO_GUI.into())
}
