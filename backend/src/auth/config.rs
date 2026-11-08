use std::{env, error::Error};

#[derive(Clone)]
pub struct AuthConfig {
    pub origin: String,
    pub frontend_origin: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub resource: String,
}

impl AuthConfig {
    pub fn from_env() -> Result<Self, Box<dyn Error>> {
        let origin = env::var("APP_ORIGIN")?;
        let frontend_origin = env::var("FRONTEND_ORIGIN")?;
        let client_id = env::var("OAUTH_CLIENT_ID")?;
        let redirect_uri = env::var("OAUTH_REDIRECT_URI")?;

        let resource = format!("{origin}/mcp");

        Ok(Self {
            origin,
            frontend_origin,
            client_id,
            redirect_uri,
            resource,
        })
    }
}
