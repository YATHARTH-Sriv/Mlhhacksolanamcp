use super::transactions::TransferError;
use reqwest::{
    Client,
    header::{HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, time::Duration};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(rename = "isVerified", default)]
    pub verified: bool,
    #[serde(default)]
    pub audit: serde_json::Value,
}

#[derive(Clone)]
pub struct TokenClient {
    client: Client,
    #[cfg(test)]
    offline: bool,
}

impl TokenClient {
    pub async fn quote_swap(
        &self,
        input: super::swap::SwapInput,
        network: &str,
    ) -> Result<super::swap::SwapQuote, TransferError> {
        super::swap::quote(&self.client, self, input, network).await
    }

    pub(super) async fn quote_token(&self, query: &str) -> Result<TokenInfo, TransferError> {
        if query.trim().eq_ignore_ascii_case("SOL") || query.trim() == super::price::SOL_MINT {
            return Ok(TokenInfo {
                id: super::price::SOL_MINT.into(),
                name: "Solana".into(),
                symbol: "SOL".into(),
                decimals: 9,
                verified: true,
                audit: serde_json::Value::Null,
            });
        }
        self.resolve_inner(query, true).await
    }
    /// Optional display labels. Mint addresses remain authoritative even when
    /// metadata lookup is unavailable or a symbol is shared by multiple mints.
    pub async fn symbols(&self, mints: &[String]) -> std::collections::HashMap<String, String> {
        let mut symbols = std::collections::HashMap::new();
        for batch in mints.chunks(100) {
            let Ok(response) = self
                .client
                .get("https://api.jup.ag/tokens/v2/search")
                .query(&[("query", batch.join(","))])
                .send()
                .await
            else {
                continue;
            };
            if !response.status().is_success() {
                continue;
            }
            let Ok(tokens) = response.json::<Vec<TokenInfo>>().await else {
                continue;
            };
            for token in tokens {
                if batch.contains(&token.id) && !token.symbol.is_empty() && token.symbol.len() <= 32
                {
                    symbols.insert(token.id, token.symbol);
                }
            }
        }
        symbols
    }

    pub fn new(key: Option<String>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut headers = HeaderMap::new();
        if let Some(key) = key.filter(|key| !key.trim().is_empty()) {
            let mut value = HeaderValue::from_str(key.trim())?;
            value.set_sensitive(true);
            headers.insert("x-api-key", value);
        }
        Ok(Self {
            client: Client::builder()
                .default_headers(headers)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(8))
                .build()?,
            #[cfg(test)]
            offline: false,
        })
    }

    pub async fn resolve(&self, query: &str) -> Result<TokenInfo, TransferError> {
        self.resolve_inner(query, false).await
    }

    async fn resolve_inner(
        &self,
        query: &str,
        require_metadata: bool,
    ) -> Result<TokenInfo, TransferError> {
        let query = query.trim();
        if query.is_empty() || query.len() > 100 {
            return Err(TransferError::bad(
                "invalid_asset",
                "Provide SOL, an exact token symbol/name, or a mint address.",
            ));
        }
        // An explicit mint is authoritative on-chain even when absent from Jupiter.
        let mint = Pubkey::from_str(query).ok();
        #[cfg(test)]
        if self.offline {
            return Ok(TokenInfo {
                id: mint
                    .ok_or_else(|| TransferError::bad("invalid_mint", "Test requires a mint"))?
                    .to_string(),
                name: "Test token".into(),
                symbol: "TEST".into(),
                decimals: 6,
                verified: false,
                audit: serde_json::Value::Null,
            });
        }
        let response = self
            .client
            .get("https://api.jup.ag/tokens/v2/search")
            .query(&[("query", query)])
            .send()
            .await;
        let results = match response {
            Ok(response) if response.status().is_success() => {
                response.json::<Vec<TokenInfo>>().await.ok()
            }
            _ => None,
        };
        if let Some(mint) = mint {
            let found = results
                .unwrap_or_default()
                .into_iter()
                .find(|token| token.id == mint.to_string());
            if require_metadata && found.is_none() {
                return Err(TransferError::bad(
                    "token_metadata_unavailable",
                    "Jupiter has no metadata for that mint. Quote decimals cannot be guessed; retry or choose a listed token.",
                ));
            }
            return Ok(found.unwrap_or(TokenInfo {
                id: mint.to_string(),
                name: "Token identified by mint".into(),
                symbol: "SPL".into(),
                decimals: 0,
                verified: false,
                audit: serde_json::Value::Null,
            }));
        }
        let results = results.ok_or_else(|| {
            TransferError::bad(
                "token_lookup_unavailable",
                "Jupiter token search is unavailable. Retry or provide the exact mint address.",
            )
        })?;
        select(query, results)
    }
}

fn select(query: &str, results: Vec<TokenInfo>) -> Result<TokenInfo, TransferError> {
    let mut verified: Vec<_> = results
        .iter()
        .filter(|token| {
            token.verified
                && (token.symbol.eq_ignore_ascii_case(query)
                    || token.name.eq_ignore_ascii_case(query))
                && token.audit.get("isSus").is_none()
        })
        .cloned()
        .collect();
    verified.sort_by(|a, b| a.id.cmp(&b.id));
    verified.dedup_by(|a, b| a.id == b.id);
    if verified.len() == 1 {
        return Ok(verified.remove(0));
    }
    let mut error = TransferError::bad(
        if results.is_empty() {
            "token_not_found"
        } else {
            "ambiguous_token"
        },
        "No unique verified exact match. Ask the user to choose a mint from candidates; never guess.",
    );
    error.candidates = Some(results.into_iter().take(20).collect());
    Err(error)
}
