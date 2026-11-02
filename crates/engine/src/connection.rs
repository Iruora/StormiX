//! How a request reached the server: which proxy it went through, the
//! address it connected to and the certificate it was shown.

use std::fmt;
use std::net::SocketAddr;

use hyper_util::client::proxy::matcher::Matcher;
use reqwest::Version;
use serde::{Deserialize, Serialize};

use crate::result::{Certificate, Connection};

/// Which proxy requests go through.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ProxyChoice {
    /// `HTTPS_PROXY`, `HTTP_PROXY` and `ALL_PROXY` first, then the Windows
    /// or macOS proxy settings. `NO_PROXY` lists hosts reached directly.
    #[default]
    System,
    /// Straight to every server, whatever the system says.
    None,
    /// This proxy for every request, like `http://proxy.corp:8080`.
    /// Environment variables and system settings are ignored.
    Url(String),
}

impl ProxyChoice {
    /// Applies the choice to a client being built.
    pub(crate) fn apply(
        &self,
        builder: reqwest::ClientBuilder,
    ) -> reqwest::Result<reqwest::ClientBuilder> {
        Ok(match self {
            Self::System => builder,
            Self::None => builder.no_proxy(),
            Self::Url(url) => builder.no_proxy().proxy(reqwest::Proxy::all(url.as_str())?),
        })
    }
}

/// The same proxy choice the client makes, to report it.
pub(crate) struct Proxies {
    matcher: Matcher,
    choice: ProxyChoice,
}

impl Proxies {
    pub(crate) fn new(choice: &ProxyChoice) -> Self {
        let matcher = match choice {
            ProxyChoice::System => Matcher::from_system(),
            ProxyChoice::None => Matcher::builder().build(),
            ProxyChoice::Url(url) => Matcher::builder().all(url.as_str()).build(),
        };
        Self {
            matcher,
            choice: choice.clone(),
        }
    }

    /// The proxy a request to `url` goes through, shown without any
    /// password, and where that setting came from.
    pub(crate) fn for_url(&self, url: &reqwest::Url) -> Option<Proxy> {
        let uri: http::Uri = url.as_str().parse().ok()?;
        let intercept = self.matcher.intercept(&uri)?;
        let proxy = intercept.uri();
        let host = proxy.host()?.to_owned();
        let scheme = proxy.scheme_str().unwrap_or("http").to_owned();
        let port = proxy
            .port_u16()
            .unwrap_or(if scheme == "https" { 443 } else { 80 });
        Some(Proxy {
            shown: format!("{scheme}://{host}:{port}"),
            host,
            port,
            auth: intercept.basic_auth().cloned(),
            source: match self.choice {
                ProxyChoice::Url(_) => "Stormix's proxy setting".into(),
                _ => proxy_source(url.scheme() == "https"),
            },
        })
    }
}

impl fmt::Debug for Proxies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Proxies")
    }
}

pub(crate) struct Proxy {
    /// `scheme://host:port`, never a password.
    pub shown: String,
    pub host: String,
    pub port: u16,
    /// `Proxy-Authorization` for a proxy URL with a user name and password.
    pub auth: Option<http::HeaderValue>,
    pub source: String,
}

/// Mirrors the order hyper-util reads them in.
fn proxy_source(https: bool) -> String {
    let names: [&str; 4] = if https {
        ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
    } else {
        ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"]
    };
    for name in names {
        if std::env::var_os(name).is_some_and(|v| !v.is_empty()) {
            return format!("the {name} environment variable");
        }
    }
    if cfg!(windows) {
        "Windows proxy settings".into()
    } else if cfg!(target_os = "macos") {
        "macOS proxy settings".into()
    } else {
        "system proxy settings".into()
    }
}

/// What a response tells about the connection it came over, kept until
/// the step knows whether it keeps the exchange.
pub(crate) struct Wire {
    pub url: reqwest::Url,
    pub remote: Option<SocketAddr>,
    pub version: Option<Version>,
    pub tls: Option<reqwest::tls::TlsInfo>,
}

impl Wire {
    pub(crate) fn of(url: reqwest::Url, response: &mut reqwest::Response) -> Self {
        Self {
            url,
            remote: response.remote_addr(),
            version: Some(response.version()),
            // Moved out rather than cloned: it holds the whole certificate.
            tls: response.extensions_mut().remove::<reqwest::tls::TlsInfo>(),
        }
    }

    /// For a request that got no response: only the route is known.
    pub(crate) fn unanswered(url: reqwest::Url) -> Self {
        Self {
            url,
            remote: None,
            version: None,
            tls: None,
        }
    }

    pub(crate) fn connection(self, proxies: &Proxies) -> Connection {
        let proxy = proxies.for_url(&self.url);
        Connection {
            proxy_source: proxy.as_ref().map(|p| p.source.clone()),
            proxy: proxy.map(|p| p.shown),
            remote_address: self.remote.map(|a| a.to_string()),
            http_version: self.version.map(version_text),
            certificate: self
                .tls
                .as_ref()
                .and_then(|t| t.peer_certificate())
                .and_then(certificate),
        }
    }
}

pub(crate) fn version_text(version: Version) -> String {
    match version {
        Version::HTTP_09 => "HTTP/0.9",
        Version::HTTP_10 => "HTTP/1.0",
        Version::HTTP_11 => "HTTP/1.1",
        Version::HTTP_2 => "HTTP/2",
        Version::HTTP_3 => "HTTP/3",
        _ => "HTTP",
    }
    .into()
}

pub(crate) fn certificate(der: &[u8]) -> Option<Certificate> {
    let (_, cert) = x509_parser::parse_x509_certificate(der).ok()?;
    Some(Certificate {
        subject: cert.subject().to_string(),
        issuer: cert.issuer().to_string(),
        not_after: cert.validity().not_after.to_string(),
    })
}
