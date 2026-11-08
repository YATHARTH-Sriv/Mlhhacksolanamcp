use reqwest::{
    Client,
    header::{HeaderMap, HeaderValue},
};
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashMap},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

#[derive(Debug, Deserialize)]
pub struct Quote {
    #[serde(rename = "usdPrice")]
    pub usd_price: f64,
    #[serde(rename = "blockId")]
    pub block_id: Option<u64>,
}

#[derive(Default)]
pub struct PriceSnapshot {
    pub quotes: HashMap<String, Quote>,
    pub warning: Option<&'static str>,
    pub fetched_at_unix: Option<u64>,
}

#[derive(Clone)]
pub struct PriceClient {
    client: Option<Client>,
}

impl PriceClient {
    pub fn new(api_key: Option<String>) -> Result<Self, Box<dyn std::error::Error>> {
        let Some(key) = api_key.filter(|key| !key.trim().is_empty()) else {
            return Ok(Self { client: None });
        };
        let mut value = HeaderValue::from_str(key.trim())?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", value);
        Ok(Self {
            client: Some(
                Client::builder()
                    .default_headers(headers)
                    .timeout(Duration::from_secs(5))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()?,
            ),
        })
    }

    pub async fn fetch(&self, mints: impl IntoIterator<Item = String>) -> PriceSnapshot {
        let Some(client) = &self.client else {
            return PriceSnapshot {
                warning: Some("JUPITER_API_KEY is not configured."),
                ..Default::default()
            };
        };
        let mints: Vec<_> = mints
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut snapshot = PriceSnapshot::default();
        for batch in mints.chunks(50) {
            let response = client
                .get("https://api.jup.ag/price/v3")
                .query(&[("ids", batch.join(","))])
                .send()
                .await;
            let response = match response {
                Ok(response) => match response.error_for_status() {
                    Ok(response) => response,
                    Err(_) => {
                        snapshot.warning =
                            Some("Jupiter rejected a price request; check API key or rate limits.");
                        continue;
                    }
                },
                Err(_) => {
                    snapshot.warning = Some("Jupiter prices are temporarily unavailable.");
                    continue;
                }
            };
            match response.json::<HashMap<String, Option<Quote>>>().await {
                Ok(quotes) => {
                    snapshot.fetched_at_unix = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .ok()
                        .map(|time| time.as_secs());
                    for mint in batch {
                        // Missing/null/invalid prices remain unavailable, never fabricated as zero.
                        if let Some(Some(quote)) = quotes.get(mint) {
                            if quote.usd_price.is_finite() && quote.usd_price > 0.0 {
                                snapshot.quotes.insert(
                                    mint.clone(),
                                    Quote {
                                        usd_price: quote.usd_price,
                                        block_id: quote.block_id,
                                    },
                                );
                            }
                        }
                    }
                }
                Err(_) => snapshot.warning = Some("Jupiter returned an invalid price response."),
            }
        }
        snapshot
    }
}
