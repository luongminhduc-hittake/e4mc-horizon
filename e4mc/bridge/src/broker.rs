//! Which relay to talk to: e4mc's broker picks the nearest one
//! (`GET https://broker.e4mc.link/getBestRelay` → `{"id":"ap","host":…,
//! "port":25575}`), and a hard-coded relay is the fallback when the broker is
//! off or answers nothing.

use serde::Deserialize;

/// The relay to dial.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Relay {
    /// Short name the broker gives it (`ap`, `eu`…); empty for a fallback.
    #[serde(default)]
    pub id: String,
    pub host: String,
    pub port: u16,
}

/// Where to ask, and what to use when the asking fails.
#[derive(Debug, Clone)]
pub struct Broker {
    /// `true` to ask `url` first.
    pub use_broker: bool,
    pub url: String,
    /// The relay used when the broker is off or fails: e4mc's own default.
    pub fallback_host: String,
    pub fallback_port: u16,
}

impl Default for Broker {
    fn default() -> Self {
        Self {
            use_broker: true,
            url: "https://broker.e4mc.link/getBestRelay".into(),
            fallback_host: "test.e4mc.link".into(),
            fallback_port: 25575,
        }
    }
}

impl Broker {
    /// The relay the broker names, else the fallback.
    pub fn relay(&self) -> Relay {
        if self.use_broker
            && let Some(relay) = self.ask().ok().flatten()
        {
            return relay;
        }
        self.fallback()
    }

    pub fn fallback(&self) -> Relay {
        Relay {
            id: "fallback".into(),
            host: self.fallback_host.clone(),
            port: self.fallback_port,
        }
    }

    /// The broker's answer, `None` when it answered something that is not a
    /// relay (an error page, an empty body).
    fn ask(&self) -> Result<Option<Relay>, String> {
        let body = http_get(&self.url)?;
        Ok(parse_relay(&body))
    }
}

/// The broker's body as a relay: `None` when it doesn't name a host and port.
pub fn parse_relay(body: &str) -> Option<Relay> {
    let relay: Relay = serde_json::from_str(body.trim()).ok()?;
    if relay.host.is_empty() || relay.port == 0 {
        return None;
    }
    Some(relay)
}

/// One plain HTTPS GET, with a timeout, through rustls and the web roots.
fn http_get(url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build();
    let response = agent
        .get(url)
        .set("Accept", "application/json")
        .call()
        .map_err(|e| format!("broker {url}: {e}"))?;
    response
        .into_string()
        .map_err(|e| format!("broker {url}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_real_answer_names_a_relay() {
        // Copied from a live `curl https://broker.e4mc.link/getBestRelay`.
        let relay = parse_relay(r#"{"id":"ap","host":"ap.e4mc.link","port":25575}"#).unwrap();
        assert_eq!(
            relay,
            Relay {
                id: "ap".into(),
                host: "ap.e4mc.link".into(),
                port: 25575
            }
        );
    }

    #[test]
    fn a_body_without_a_host_is_not_a_relay() {
        assert_eq!(parse_relay(r#"{"id":"ap","port":25575}"#), None);
        assert_eq!(parse_relay(r#"{"host":"","port":25575}"#), None);
        assert_eq!(parse_relay(r#"{"host":"ap.e4mc.link","port":0}"#), None);
        assert_eq!(parse_relay("<html>502 Bad Gateway</html>"), None);
        assert_eq!(parse_relay(""), None);
    }

    #[test]
    fn a_trailing_newline_does_not_break_it() {
        let relay =
            parse_relay("{\"id\":\"eu\",\"host\":\"eu.e4mc.link\",\"port\":25575}\n").unwrap();
        assert_eq!(relay.host, "eu.e4mc.link");
    }

    #[test]
    fn without_the_broker_the_fallback_relay_is_used() {
        let broker = Broker {
            use_broker: false,
            url: "https://127.0.0.1:1/never".into(),
            fallback_host: "test.e4mc.link".into(),
            fallback_port: 25575,
        };
        assert_eq!(
            broker.relay(),
            Relay {
                id: "fallback".into(),
                host: "test.e4mc.link".into(),
                port: 25575
            }
        );
    }

    #[test]
    fn a_broker_that_answers_nothing_falls_back_instead_of_failing() {
        let broker = Broker {
            use_broker: true,
            url: "https://127.0.0.1:1/never".into(),
            fallback_host: "eu.e4mc.link".into(),
            fallback_port: 25575,
        };
        assert_eq!(broker.relay().host, "eu.e4mc.link");
    }
}
