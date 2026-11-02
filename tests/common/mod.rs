//! Shared by the offline tests.

use agentai::Agent;
use agentai::genai::Client;
use agentai::genai::adapter::AdapterKind;
use agentai::genai::resolver::{AuthData, Endpoint};

/// An agent that talks to the OpenAI-compatible server at `base_url`, e.g. a fake one.
pub fn local_agent(base_url: &str) -> Agent {
    // A client built by hand needs the rustls provider, which agentai installs only for its own
    agentai::tls::install_crypto_provider();
    let client = Client::builder()
        .with_adapter_kind(AdapterKind::OpenAI)
        .append_provider_config(
            AdapterKind::OpenAI,
            (
                Endpoint::from_owned(base_url.to_owned()),
                AuthData::from_single("key"),
            ),
        )
        .build()
        .unwrap();
    Agent::from_client(client, "model", "system")
}
