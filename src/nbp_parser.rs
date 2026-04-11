use chrono::{DateTime, Utc};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

static TR_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("tr").unwrap());
static TD_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("td").unwrap());

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("XML deserialization failed: {0}")]
    Xml(#[from] quick_xml::DeError),

    #[error("Invalid pubDate {date:?}: {source}")]
    InvalidDate {
        date: String,
        #[source]
        source: chrono::format::ParseError,
    },

    #[error("Invalid rate value: {0}")]
    InvalidRate(#[from] std::num::ParseFloatError),

    #[error("Missing {0} rate")]
    MissingRate(&'static str),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExchangeRateSummary {
    pub eur: f64,
    pub usd: f64,
    pub chf: Option<f64>,
    pub gbp: Option<f64>,
    pub jpy100: Option<f64>,
}

#[derive(Clone, Serialize)]
pub struct CurrencyExchangeRateItem {
    pub title: String,
    pub pub_date: DateTime<Utc>,
    pub link: String,
    pub rates: ExchangeRateSummary,
}

pub fn parse_nbp_xml(xml: &str) -> Result<Vec<CurrencyExchangeRateItem>, ParseError> {
    let rss: Rss = quick_xml::de::from_str(xml)?;
    rss.channel
        .items
        .into_iter()
        .map(|item| {
            let pub_date = DateTime::parse_from_rfc2822(&item.pub_date)
                .map_err(|source| ParseError::InvalidDate {
                    date: item.pub_date.clone(),
                    source,
                })?
                .with_timezone(&Utc);
            Ok(CurrencyExchangeRateItem {
                title: item.title,
                link: item.link,
                pub_date,
                rates: parse_rates_html(&item.description)?,
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct Rss {
    channel: Channel,
}

#[derive(Deserialize)]
struct Channel {
    #[serde(rename = "item", default)]
    items: Vec<RssItem>,
}

#[derive(Deserialize)]
struct RssItem {
    title: String,
    link: String,
    #[serde(rename = "pubDate")]
    pub_date: String,
    description: String,
}

fn parse_rates_html(html: &str) -> Result<ExchangeRateSummary, ParseError> {
    let document = Html::parse_fragment(html);

    let mut eur = None;
    let mut usd = None;
    let mut chf = None;
    let mut gbp = None;
    let mut jpy100 = None;

    for tr in document.select(&TR_SELECTOR) {
        let tds: Vec<_> = tr.select(&TD_SELECTOR).collect();
        if tds.len() == 2 {
            let label = tds[0].text().collect::<String>();
            let value_str = tds[1].text().collect::<String>().replace(',', ".");
            let rate: f64 = value_str.trim().parse()?;

            if label.contains("EUR") {
                eur = Some(rate);
            } else if label.contains("USD") {
                usd = Some(rate);
            } else if label.contains("CHF") {
                chf = Some(rate);
            } else if label.contains("GBP") {
                gbp = Some(rate);
            } else if label.contains("JPY") {
                jpy100 = Some(rate);
            }
        }
    }

    Ok(ExchangeRateSummary {
        eur: eur.ok_or(ParseError::MissingRate("EUR"))?,
        usd: usd.ok_or(ParseError::MissingRate("USD"))?,
        chf,
        gbp,
        jpy100,
    })
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    const XML: &str = r#"<rss xmlns:atom="http://www.w3.org/2005/Atom" version="2.0">
  <channel>
    <title>NBP - Tabela A kursów średnich walut obcych</title>
    <description>Tabela A kursów średnich walut obcych</description>
    <link>http://www.nbp.pl/home.aspx?f=/Statystyka/kursy.html</link>
    <atom:link href="http://rss.nbp.pl/kursy/TabelaA.xml" rel="self" type="application/rss+xml"/>
    <copyright>Copyright © 2026. Narodowy Bank Polski</copyright>
    <language>pl</language>
    <webMaster>webmaster@nbp.pl (Webmaster NBP)</webMaster>
    <lastBuildDate>Fri, 03 Apr 2026 11:45:12 +0200</lastBuildDate>
    <image>
      <url>http://rss.nbp.pl/kursy/img/NBP_rss_new.gif</url>
      <link>http://www.nbp.pl/</link>
      <title>NBP - Tabela A kursów średnich walut obcych</title>
    </image>
    <item>
        <title>Tabela nr 069/A/NBP/2026 z dnia 2026-04-10</title>
        <description>
        Tabela A kursów średnich
        <![CDATA[ <br/><br/> <table> <tr align="right"><td colspan="2"><b>Wybrane kursy średnie:</b></td></tr> <tr align="right"><td>1 EUR =</td><td>4,2534</td></tr> <tr align="right"><td>1 USD =</td><td>3,6396</td></tr> <tr align="right"><td>1 CHF =</td><td>4,6062</td></tr> <tr align="right"><td>1 GBP =</td><td>4,8848</td></tr> <tr align="right"><td>100 JPY =</td><td>2,2845</td></tr> </table> ]]>
        </description>
        <link>http://rss.nbp.pl/kursy/TabRss.aspx?n=2026/a/26a069</link>
        <pubDate>Fri, 10 Apr 2026 11:45:02 +0200</pubDate>
        <enclosure url="http://rss.nbp.pl/kursy/xml2/2026/a/26a069.xml" length="6453" type="text/xml"/>
        <guid isPermaLink="false">26a069</guid>
    </item>
    <item>
        <title>Tabela nr 068/A/NBP/2026 z dnia 2026-04-09</title>
        <description>
        Tabela A kursów średnich
        <![CDATA[ <br/><br/> <table> <tr align="right"><td colspan="2"><b>Wybrane kursy średnie:</b></td></tr> <tr align="right"><td>1 EUR =</td><td>4,2610</td></tr> <tr align="right"><td>1 USD =</td><td>3,6506</td></tr> <tr align="right"><td>1 CHF =</td><td>4,6147</td></tr> <tr align="right"><td>1 GBP =</td><td>4,8924</td></tr> <tr align="right"><td>100 JPY =</td><td>2,2959</td></tr> </table> ]]>
        </description>
        <link>http://rss.nbp.pl/kursy/TabRss.aspx?n=2026/a/26a068</link>
        <pubDate>Thu, 09 Apr 2026 11:45:14 +0200</pubDate>
        <enclosure url="http://rss.nbp.nl/kursy/xml2/2026/a/26a068.xml" length="6453" type="text/xml"/>
        <guid isPermaLink="false">26a068</guid>
    </item>
    <item>
        <title>Tabela nr 067/A/NBP/2026 z dnia 2026-04-08</title>
        <description>
        Tabela A kursów średnich
        <![CDATA[ <br/><br/> <table> <tr align="right"><td colspan="2"><b>Wybrane kursy średnie:</b></td></tr> <tr align="right"><td>1 EUR =</td><td>4,2623</td></tr> <tr align="right"><td>1 USD =</td><td>3,6489</td></tr> <tr align="right"><td>1 CHF =</td><td>4,6256</td></tr> <tr align="right"><td>1 GBP =</td><td>4,8995</td></tr> <tr align="right"><td>100 JPY =</td><td>2,3040</td></tr> </table> ]]>
        </description>
        <link>http://rss.nbp.pl/kursy/TabRss.aspx?n=2026/a/26a067</link>
        <pubDate>Wed, 08 Apr 2026 11:45:12 +0200</pubDate>
        <enclosure url="http://rss.nbp.pl/kursy/xml2/2026/a/26a067.xml" length="6453" type="text/xml"/>
        <guid isPermaLink="false">26a067</guid>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn parses_the_xml() {
        let result = parse_nbp_xml(XML).unwrap();
        assert_eq!(
            result[0].title,
            "Tabela nr 069/A/NBP/2026 z dnia 2026-04-10"
        );
        assert_eq!(
            result[0].link,
            "http://rss.nbp.pl/kursy/TabRss.aspx?n=2026/a/26a069",
        );
        assert_eq!(
            result[0].pub_date,
            DateTime::parse_from_rfc3339("2026-04-10T11:45:02+02:00").unwrap(),
        );
        assert_eq!(
            result[0].rates,
            ExchangeRateSummary {
                eur: 4.2534,
                usd: 3.6396,
                chf: Some(4.6062),
                gbp: Some(4.8848),
                jpy100: Some(2.2845),
            }
        );
    }
}
