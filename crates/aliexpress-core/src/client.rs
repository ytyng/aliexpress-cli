//! Fetching search pages.

use std::time::Duration;

use crate::model::SearchPage;
use crate::parse::parse_search_html;
use crate::url::{form_encode, path_slug};
use crate::{Error, SearchOptions};

/// Which regional AliExpress site to search, and in which currency.
///
/// The same product has a different price and a different set of programmes
/// (Yoridori is Japan only) depending on the site, so this is part of the
/// query rather than a display preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// Host of the search pages, e.g. `ja.aliexpress.com`.
    pub host: String,
    /// `site` value of the `aep_usuc_f` cookie, e.g. `jpn`.
    pub site: String,
    /// Two letter country code the products are shipped to.
    pub region: String,
    /// Currency code prices are shown in.
    pub currency: String,
    /// Locale of the page text, e.g. `ja_JP`.
    pub locale: String,
}

impl Site {
    /// The Japanese site, in yen. The default because Yoridori exists there.
    pub fn japan() -> Self {
        Site {
            host: "ja.aliexpress.com".to_string(),
            site: "jpn".to_string(),
            region: "JP".to_string(),
            currency: "JPY".to_string(),
            locale: "ja_JP".to_string(),
        }
    }

    /// The cookie that tells AliExpress which region, currency and language to
    /// render for. Without it the server picks from the client's IP address.
    fn preference_cookie(&self) -> String {
        format!(
            "aep_usuc_f=site={}&c_tp={}&region={}&b_locale={}",
            self.site, self.currency, self.region, self.locale
        )
    }

    /// The `Accept-Language` value matching the locale, e.g. `ja-JP,ja;q=0.9`.
    fn accept_language(&self) -> String {
        let language = self.locale.split('_').next().unwrap_or("en");
        format!(
            "{},{language};q=0.9,en;q=0.5",
            self.locale.replace('_', "-")
        )
    }

    /// Absolute URL of a product page.
    pub fn product_url(&self, id: &str) -> String {
        format!("https://{}/item/{id}.html", self.host)
    }
}

/// A browser's user agent: the search page is served to browsers, and a bare
/// client is answered with a bot check instead.
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

const TIMEOUT: Duration = Duration::from_secs(30);

/// Fetches search pages from one site. Cloning shares the connection pool.
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    site: Site,
    /// Cookies the user supplied, sent as-is. A logged in session's cookies make
    /// the server render the prices that account sees.
    cookie: Option<String>,
}

impl Client {
    pub fn new(site: Site, cookie: Option<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            // Set on the agent as well as on each request so that a proxy
            // CONNECT carries the same user agent as the request behind it.
            .user_agent(USER_AGENT)
            .build();
        Client {
            agent: config.into(),
            site,
            cookie: cookie
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty()),
        }
    }

    pub fn site(&self) -> &Site {
        &self.site
    }

    /// URL of one search page. Public so the front ends can show it.
    pub fn search_url(&self, options: &SearchOptions, page: u32) -> String {
        let mut url = format!(
            "https://{}/w/wholesale-{}.html?SearchText={}&page={page}&sortType={}",
            self.site.host,
            path_slug(&options.keyword),
            form_encode(options.keyword.trim()),
            options.sort.server_param()
        );
        let mut switches = Vec::new();
        if options.choice {
            switches.push("filterCode:choice_atm");
        }
        if options.free_shipping {
            switches.push("filterCode:freeshipping");
        }
        // The server only knows "4 stars and up"; a stricter minimum is applied
        // locally on top of it.
        if options
            .filter
            .min_rating
            .is_some_and(|rating| rating >= 4.0)
        {
            switches.push("filterCode:4StarRating");
        }
        if !switches.is_empty() {
            url.push_str("&selectedSwitches=");
            url.push_str(&form_encode(&switches.join(",")));
        }
        if let Some(min) = options.filter.min_price {
            url.push_str(&format!("&minPrice={}", format_price_param(min)));
        }
        if let Some(max) = options.filter.max_price {
            url.push_str(&format!("&maxPrice={}", format_price_param(max)));
        }
        url
    }

    /// Fetches and parses one page.
    pub fn search_page(&self, options: &SearchOptions, page: u32) -> Result<SearchPage, Error> {
        let url = self.search_url(options, page);
        let html = self.get(&url)?;
        Ok(parse_search_html(&html, &self.site)?)
    }

    fn get(&self, url: &str) -> Result<String, ureq::Error> {
        let cookie = match &self.cookie {
            // The user's cookies come first so that a preference they carry wins
            // over ours; a server reads the first value of a repeated name.
            Some(cookie) => format!("{cookie}; {}", self.site.preference_cookie()),
            None => self.site.preference_cookie(),
        };
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Accept-Language", &self.site.accept_language())
            .header("Cookie", &cookie)
            .call()?;
        response.body_mut().read_to_string()
    }
}

/// A price for the query string: whole numbers without a decimal point, since
/// that is what the site's own filter sends.
fn format_price_param(price: f64) -> String {
    if price.fract() == 0.0 {
        format!("{}", price as i64)
    } else {
        format!("{price}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Filter, Kind, Sort};

    fn options() -> SearchOptions {
        SearchOptions {
            keyword: "usb c cable".to_string(),
            pages: 1,
            sort: Sort::Best,
            filter: Filter {
                kind: Kind::Any,
                min_price: None,
                max_price: None,
                min_rating: None,
                exclude_ads: false,
            },
            choice: false,
            free_shipping: false,
            dedupe: false,
            limit: None,
        }
    }

    #[test]
    fn search_url_carries_only_the_filters_asked_for() {
        // Arrange
        let client = Client::new(Site::japan(), None);
        // Act
        let url = client.search_url(&options(), 2);
        // Assert
        assert_eq!(
            url,
            "https://ja.aliexpress.com/w/wholesale-usb-c-cable.html?SearchText=usb+c+cable&page=2&sortType=default"
        );
    }

    #[test]
    fn search_url_maps_every_filter_to_the_site_parameter() {
        // Arrange
        let client = Client::new(Site::japan(), None);
        let mut options = options();
        options.sort = Sort::PriceAsc;
        options.choice = true;
        options.free_shipping = true;
        options.filter.min_rating = Some(4.5);
        options.filter.min_price = Some(100.0);
        options.filter.max_price = Some(500.0);
        // Act
        let url = client.search_url(&options, 1);
        // Assert
        assert!(url.contains("&sortType=price_asc"), "{url}");
        assert!(
            url.contains(
                "&selectedSwitches=filterCode%3Achoice_atm%2CfilterCode%3Afreeshipping%2CfilterCode%3A4StarRating"
            ),
            "{url}"
        );
        assert!(url.ends_with("&minPrice=100&maxPrice=500"), "{url}");
    }

    #[test]
    fn a_low_minimum_rating_is_not_sent_to_the_server() {
        // Arrange
        let client = Client::new(Site::japan(), None);
        let mut options = options();
        options.filter.min_rating = Some(3.0);
        // Act
        let url = client.search_url(&options, 1);
        // Assert
        assert!(!url.contains("4StarRating"), "{url}");
    }

    #[test]
    fn value_sort_is_fetched_in_the_default_order() {
        // Arrange
        let client = Client::new(Site::japan(), None);
        let mut options = options();
        options.sort = Sort::Value;
        // Act
        let url = client.search_url(&options, 1);
        // Assert
        assert!(url.ends_with("sortType=default"), "{url}");
    }
}
