//! Sends one step again and checks each stage on the way to the server,
//! to tell where a request fails: the proxy, name lookup, the connection,
//! TLS, or the server's answer.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use stormix_flow::{Flow, TraceMode};
use tokio::net::TcpStream;

use crate::connection::ProxyChoice;
use crate::result::{Certificate, HeaderPair, StepResult};
use crate::runner::{Exchanges, Runner};
use crate::trace::{self, TraceParent};
use crate::Variables;

/// Where a request fails, stage by stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    /// In the order a request goes through them.
    pub stages: Vec<DiagnosisStage>,
    /// One or two sentences: where it failed and what to try, or that it
    /// went through.
    pub verdict: String,
    /// The request got a response that passed.
    pub ok: bool,
    /// The step as it ran this time, with its request and response.
    pub step: StepResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisStage {
    pub name: String,
    pub outcome: StageOutcome,
    pub summary: String,
    pub duration_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum StageOutcome {
    Passed,
    Failed,
    Warning,
    /// Not a check, only what was found, like the proxy in use.
    Info,
    /// After a stage that failed.
    NotReached,
}

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

impl Runner {
    /// Runs the enabled steps before `step_name` for the variables they set,
    /// then sends that step without its cache and checks each stage of its
    /// request. Fails only when the flow has no enabled step by that name.
    pub async fn diagnose(
        &self,
        flow: &Flow,
        overrides: &Variables,
        step_name: &str,
    ) -> Result<Diagnosis, String> {
        // Every request and response is kept, passed or not.
        let runner = self.keeping(Exchanges::All);
        let steps: Vec<_> = flow.enabled_steps().collect();
        let Some(index) = steps.iter().position(|s| s.name == step_name) else {
            return Err(format!("the flow has no enabled step named `{step_name}`"));
        };
        let mut vars = Variables::new();
        vars.extend(flow.variables.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        vars.extend(overrides.entries());
        let run_trace = (flow.trace == Some(TraceMode::Flow)).then(trace::new_trace_id);
        let tracing = || match (flow.trace, &run_trace) {
            (Some(_), Some(trace_id)) => Some(TraceParent::child_of(trace_id)),
            (Some(_), None) => Some(TraceParent::new()),
            (None, _) => None,
        };

        let mut stages = Vec::new();
        if index > 0 {
            let mut failed = Vec::new();
            for step in &steps[..index] {
                let result = runner
                    .run_step(&flow.name, step, &mut vars, tracing().as_ref())
                    .await;
                if !result.passed() {
                    failed.push(format!("`{}`", result.name));
                }
            }
            stages.push(if failed.is_empty() {
                DiagnosisStage::new(
                    "Earlier steps",
                    StageOutcome::Passed,
                    if index == 1 {
                        "Ran the step before it for the variables it sets.".into()
                    } else {
                        format!("Ran the {index} steps before it for the variables they set.")
                    },
                )
            } else {
                DiagnosisStage::new(
                    "Earlier steps",
                    StageOutcome::Warning,
                    format!(
                        "{} failed, so variables it sets, like a token, may be missing or stale.",
                        failed.join(", ")
                    ),
                )
            });
        }

        // Sent for real, so a cached response does not hide the network.
        let mut step = steps[index].clone();
        step.cache = false;
        let before = vars.clone();
        let result = runner
            .run_step(&flow.name, &step, &mut vars, tracing().as_ref())
            .await;
        let (more, mut verdict, ok) = self.probe(&result).await;
        stages.extend(more);

        // Through a proxy and refused: is it the proxy that refuses?
        let proxied = reqwest::Url::parse(&result.url)
            .ok()
            .and_then(|url| Some((self.proxies.for_url(&url)?, url.host_str()?.to_owned())));
        if let (false, Some((proxy, host))) = (ok, proxied) {
            if let Ok(direct) = runner.with_proxy(ProxyChoice::None) {
                let mut vars = before;
                let again = direct
                    .run_step(&flow.name, &step, &mut vars, tracing().as_ref())
                    .await;
                let (stage, advice) = compare_direct(&proxy.shown, &host, &result, &again);
                stages.push(stage);
                verdict = advice.unwrap_or(verdict);
            }
        }
        Ok(Diagnosis {
            stages,
            verdict,
            ok,
            step: result,
        })
    }

    async fn probe(&self, result: &StepResult) -> (Vec<DiagnosisStage>, String, bool) {
        let mut stages = Vec::new();
        let url = match reqwest::Url::parse(&result.url) {
            Ok(url) => url,
            Err(err) => {
                let summary = format!("`{}` is not a valid URL: {err}.", result.url);
                stages.push(DiagnosisStage::new(
                    "URL",
                    StageOutcome::Failed,
                    summary.clone(),
                ));
                return (stages, summary, false);
            }
        };
        let Some(host) = url.host_str().map(str::to_owned) else {
            let summary = format!("`{}` has no host.", result.url);
            stages.push(DiagnosisStage::new(
                "URL",
                StageOutcome::Failed,
                summary.clone(),
            ));
            return (stages, summary, false);
        };
        let port = url.port_or_known_default().unwrap_or(80);
        let https = url.scheme() == "https";
        let proxy = self.proxies.for_url(&url);
        let error = result.error.as_deref().unwrap_or_default().to_lowercase();
        let mut failed: Option<String> = None;

        // Proxy
        match &proxy {
            Some(p) => stages.push(DiagnosisStage::new(
                "Proxy",
                StageOutcome::Info,
                format!(
                    "Through {}{}, from {}.",
                    p.shown,
                    if p.auth.is_some() {
                        " with a user name and password"
                    } else {
                        ""
                    },
                    p.source
                ),
            )),
            None => stages.push(DiagnosisStage::new(
                "Proxy",
                StageOutcome::Info,
                if *self.proxy() == ProxyChoice::None {
                    "None: Stormix is set to send requests straight to the server.".into()
                } else {
                    "None: straight to the server. Choose a proxy, or set HTTPS_PROXY, if your \
                     network needs one."
                        .into()
                },
            )),
        }
        let (dial_host, dial_port) = match &proxy {
            Some(p) => (p.host.clone(), p.port),
            None => (host.clone(), port),
        };
        let of_what = if proxy.is_some() { "the proxy " } else { "" };

        // Name lookup
        let started = Instant::now();
        let lookup = tokio::time::timeout(
            PROBE_TIMEOUT,
            tokio::net::lookup_host((dial_host.as_str(), dial_port)),
        )
        .await;
        let addresses: Vec<SocketAddr> = match lookup {
            Ok(Ok(found)) => found.collect(),
            Ok(Err(err)) => {
                let summary = format!("Could not look up {of_what}{dial_host}: {err}.");
                failed = Some(format!(
                    "{summary} Check the name, or connect to the VPN if it is an internal host."
                ));
                stages.push(DiagnosisStage::timed(
                    "Name lookup",
                    StageOutcome::Failed,
                    summary,
                    started,
                ));
                Vec::new()
            }
            Err(_) => {
                let summary = format!("Looking up {of_what}{dial_host} took over 10 s.");
                failed = Some(summary.clone());
                stages.push(DiagnosisStage::timed(
                    "Name lookup",
                    StageOutcome::Failed,
                    summary,
                    started,
                ));
                Vec::new()
            }
        };
        if failed.is_none()
            && dial_host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok()
        {
            stages.push(DiagnosisStage::new(
                "Name lookup",
                StageOutcome::Passed,
                capitalize(&format!(
                    "{of_what}{dial_host} is an IP address, so there is nothing to look up."
                )),
            ));
        } else if failed.is_none() {
            let shown: Vec<String> = addresses
                .iter()
                .take(3)
                .map(|a| a.ip().to_string())
                .collect();
            let mut summary = format!(
                "{}{dial_host} is at {}.",
                if proxy.is_some() { "The proxy " } else { "" },
                shown.join(", ")
            );
            if proxy.is_some() {
                summary.push_str(&format!(" The proxy looks up {host} itself."));
            }
            stages.push(DiagnosisStage::timed(
                "Name lookup",
                StageOutcome::Passed,
                summary,
                started,
            ));
        }

        // Connection
        let mut stream = None;
        if failed.is_none() {
            let started = Instant::now();
            let mut last_error = String::from("no address to connect to");
            for address in &addresses {
                match tokio::time::timeout(PROBE_TIMEOUT, TcpStream::connect(address)).await {
                    Ok(Ok(s)) => {
                        stream = Some((s, *address));
                        break;
                    }
                    Ok(Err(err)) => last_error = err.to_string(),
                    Err(_) => last_error = "no answer within 10 s".into(),
                }
            }
            match &stream {
                Some((_, address)) => stages.push(DiagnosisStage::timed(
                    "Connection",
                    StageOutcome::Passed,
                    capitalize(&format!("Connected to {of_what}{address}.")),
                    started,
                )),
                None => {
                    let summary = format!(
                        "Could not connect to {of_what}{dial_host}:{dial_port}: {last_error}."
                    );
                    failed = Some(format!(
                        "{summary} A firewall may block it, or the server is down or needs the VPN."
                    ));
                    stages.push(DiagnosisStage::timed(
                        "Connection",
                        StageOutcome::Failed,
                        summary,
                        started,
                    ));
                }
            }
        } else {
            stages.push(DiagnosisStage::not_reached("Connection"));
        }
        let mut stream = stream.map(|(s, _)| s);

        // The proxy opening a tunnel for HTTPS, asked the way the client asks.
        if let (true, Some(p), Some(tcp)) = (https, &proxy, stream.as_mut()) {
            let started = Instant::now();
            match tokio::time::timeout(
                PROBE_TIMEOUT,
                open_tunnel(tcp, &host, port, p.auth.as_ref()),
            )
            .await
            {
                Ok(Ok(())) => stages.push(DiagnosisStage::timed(
                    "Proxy tunnel",
                    StageOutcome::Passed,
                    format!("The proxy opened a connection to {host}:{port}."),
                    started,
                )),
                Ok(Err(refusal)) => {
                    let (summary, advice) = refusal.explain(&p.shown, &host, port);
                    failed = Some(format!("{summary} {advice}"));
                    stages.push(DiagnosisStage::timed(
                        "Proxy tunnel",
                        StageOutcome::Failed,
                        summary,
                        started,
                    ));
                    stream = None;
                }
                Err(_) => {
                    let summary = format!(
                        "The proxy {} did not answer a request to reach {host}:{port} within 10 s.",
                        p.shown
                    );
                    failed = Some(summary.clone());
                    stages.push(DiagnosisStage::timed(
                        "Proxy tunnel",
                        StageOutcome::Failed,
                        summary,
                        started,
                    ));
                    stream = None;
                }
            }
        }

        // TLS: the certificate is read over a connection of its own, since
        // the client does not report it through a proxy.
        if https {
            match stream.take() {
                None => stages.push(DiagnosisStage::not_reached("TLS")),
                Some(tcp) => {
                    let started = Instant::now();
                    let shown =
                        tokio::time::timeout(PROBE_TIMEOUT, peer_certificate(tcp, &host)).await;
                    let cert = match &shown {
                        Ok(Ok(cert)) => cert.clone(),
                        _ => None,
                    };
                    let issued = cert
                        .as_ref()
                        .map(|c| format!("Certificate for {}, issued by {}.", c.subject, c.issuer))
                        .unwrap_or_default();
                    if is_tls_error(&error) {
                        let summary = format!(
                            "{issued} Not trusted: {}.",
                            result.error.as_deref().unwrap_or_default()
                        )
                        .trim_start()
                        .to_owned();
                        failed = Some(format!(
                            "The certificate {host} presented is not trusted. If your company inspects HTTPS, \
                             its root certificate must be in the system certificate store; turning on \
                             accepting invalid certificates (--insecure) confirms it."
                        ));
                        stages.push(DiagnosisStage::timed(
                            "TLS",
                            StageOutcome::Failed,
                            summary,
                            started,
                        ));
                    } else {
                        match shown {
                            Ok(Ok(Some(_))) => stages.push(DiagnosisStage::timed(
                                "TLS",
                                StageOutcome::Passed,
                                issued,
                                started,
                            )),
                            Ok(Ok(None)) => stages.push(DiagnosisStage::timed(
                                "TLS",
                                StageOutcome::Passed,
                                "Secure connection made.".into(),
                                started,
                            )),
                            Ok(Err(err)) => stages.push(DiagnosisStage::timed(
                                "TLS",
                                StageOutcome::Warning,
                                format!("Could not read the certificate: {err}."),
                                started,
                            )),
                            Err(_) => stages.push(DiagnosisStage::timed(
                                "TLS",
                                StageOutcome::Warning,
                                "Could not read the certificate within 10 s.".into(),
                                started,
                            )),
                        }
                    }
                }
            }
        }

        // Response
        if failed.is_some() {
            stages.push(DiagnosisStage::not_reached("Response"));
        } else if let Some(status) = result.status {
            let response = result.exchange.as_ref().and_then(|e| e.response.as_ref());
            let status_text = response.map(|r| r.status_text.as_str()).unwrap_or_default();
            let headers = response.map(|r| r.headers.as_slice()).unwrap_or_default();
            let body = response
                .and_then(|r| r.body.as_ref())
                .map(|b| b.text.as_str())
                .unwrap_or_default();
            let version = result
                .exchange
                .as_ref()
                .and_then(|e| e.connection.as_ref())
                .and_then(|c| c.http_version.clone())
                .unwrap_or_else(|| "HTTP".into());
            let line = format!("{status} {status_text}").trim().to_owned();
            if status >= 400 {
                let from = answered_by(status, headers, body);
                let title = html_title(body);
                let mut summary = format!("{version} {line}");
                if let Some(from) = &from {
                    summary.push_str(&format!(", from {from}"));
                }
                if let Some(title) = &title {
                    summary.push_str(&format!(": \"{title}\""));
                }
                summary.push('.');
                failed = Some(match (&from, status) {
                    (Some(from), _) if proxy.is_some() => format!(
                        "The {line} seems to come from {from}, not from the API. The proxy may block this \
                         site or this app: ask your network team, or compare with a client that gets \
                         through, like Bruno."
                    ),
                    (Some(from), _) => format!(
                        "The {line} seems to come from {from}, not from the API. Compare its headers with a \
                         client that gets through, like Bruno."
                    ),
                    (None, 401 | 403) if proxy.is_none() => format!(
                        "The server answered {line}. If another app gets through on this network, it may \
                         use a proxy that Stormix does not: check the Proxy stage."
                    ),
                    (None, _) => format!(
                        "The server answered {line}. The request reached it; compare the headers and body \
                         with a working client."
                    ),
                });
                stages.push(DiagnosisStage::new(
                    "Response",
                    StageOutcome::Failed,
                    summary,
                ));
            } else {
                stages.push(DiagnosisStage::new(
                    "Response",
                    StageOutcome::Passed,
                    format!("{version} {line} in {} ms.", result.duration_ms.round()),
                ));
            }
        } else if !error.is_empty() {
            let message = result.error.clone().unwrap_or_default();
            let summary = if error.contains("timed out") {
                format!("No response: {message}.")
            } else {
                format!("The request failed: {message}.")
            };
            failed = Some(summary.clone());
            stages.push(DiagnosisStage::new(
                "Response",
                StageOutcome::Failed,
                summary,
            ));
        }

        let passed = failed.is_none() && result.passed();
        let verdict = match failed {
            Some(verdict) => verdict,
            None if passed => format!(
                "The request went through: {} in {} ms.",
                result.status.map(|s| s.to_string()).unwrap_or_default(),
                result.duration_ms.round()
            ),
            None => {
                "The request went through, but checks on the response failed: see Checks.".into()
            }
        };
        (stages, verdict, passed)
    }
}

impl DiagnosisStage {
    fn new(name: &str, outcome: StageOutcome, summary: String) -> Self {
        Self {
            name: name.into(),
            outcome,
            summary,
            duration_ms: None,
        }
    }

    fn timed(name: &str, outcome: StageOutcome, summary: String, started: Instant) -> Self {
        Self {
            duration_ms: Some(started.elapsed().as_secs_f64() * 1000.0),
            ..Self::new(name, outcome, summary)
        }
    }

    fn not_reached(name: &str) -> Self {
        Self::new(name, StageOutcome::NotReached, "Not reached.".into())
    }
}

/// A proxy's answer to `CONNECT` other than 200.
struct Refusal {
    status: Option<u16>,
    status_line: String,
    headers: Vec<HeaderPair>,
    body: String,
}

impl Refusal {
    fn explain(&self, proxy: &str, host: &str, port: u16) -> (String, String) {
        let mut summary = format!(
            "The proxy {proxy} answered {} when asked for {host}:{port}",
            self.status_line
        );
        if let Some(from) = answered_by(self.status.unwrap_or(0), &self.headers, &self.body) {
            summary.push_str(&format!(", from {from}"));
        }
        if let Some(title) = html_title(&self.body) {
            summary.push_str(&format!(": \"{title}\""));
        }
        summary.push('.');
        let advice = match self.status {
            Some(407) => "Put a user name and password in the proxy URL, like \
                          HTTPS_PROXY=http://user:password@host:port. Windows sign-in to a proxy \
                          (NTLM or Kerberos) is not supported."
                .to_owned(),
            _ => format!(
                "It may block this site, or allow only some apps. Ask your network team, or set \
                 NO_PROXY={host} if the site is reachable without the proxy."
            ),
        };
        (summary, advice)
    }
}

/// Asks the proxy to connect to `host:port`, as the HTTP client does for HTTPS.
async fn open_tunnel(
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    auth: Option<&http::HeaderValue>,
) -> Result<(), Refusal> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let failed = |message: String| Refusal {
        status: None,
        status_line: message,
        headers: Vec::new(),
        body: String::new(),
    };
    let target = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(auth) = auth.and_then(|a| a.to_str().ok()) {
        request.push_str(&format!("Proxy-Authorization: {auth}\r\n"));
    }
    request.push_str(&format!(
        "User-Agent: stormix/{}\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    ));
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| failed(format!("nothing (sending failed: {e})")))?;

    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|e| failed(format!("nothing (reading failed: {e})")))?;
        if n == 0 {
            return Err(failed("nothing and closed the connection".into()));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > 64 * 1024 {
            return Err(failed("with headers too long to read".into()));
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let first = lines.next().unwrap_or_default();
    let status = first
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse::<u16>().ok());
    if status.is_some_and(|s| (200..300).contains(&s)) {
        return Ok(());
    }
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| HeaderPair {
            name: n.trim().to_owned(),
            value: v.trim().to_owned(),
        })
        .collect();
    // Whatever body came with the head is enough to name the proxy.
    let body = String::from_utf8_lossy(&buf[head_end + 4..]).into_owned();
    Err(Refusal {
        status,
        status_line: first
            .split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" "),
        headers,
        body,
    })
}

/// The certificate the other end presents, trusted or not: under TLS
/// inspection it is the company's, not the server's.
async fn peer_certificate(stream: TcpStream, host: &str) -> Result<Option<Certificate>, String> {
    use std::sync::{Arc, Mutex};

    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::crypto::{
        aws_lc_rs, verify_tls12_signature, verify_tls13_signature, CryptoProvider,
    };
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, SignatureScheme};

    #[derive(Debug)]
    struct Record {
        provider: Arc<CryptoProvider>,
        seen: Mutex<Option<Vec<u8>>>,
    }

    impl ServerCertVerifier for Record {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            _: &ServerName<'_>,
            _: &[u8],
            _: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            *self.seen.lock().expect("not poisoned") = Some(end_entity.to_vec());
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            verify_tls12_signature(
                message,
                cert,
                dss,
                &self.provider.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            verify_tls13_signature(
                message,
                cert,
                dss,
                &self.provider.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.provider
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    let provider = Arc::new(aws_lc_rs::default_provider());
    let record = Arc::new(Record {
        provider: provider.clone(),
        seen: Mutex::new(None),
    });
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(record.clone())
        .with_no_client_auth();
    let name = ServerName::try_from(host.to_owned()).map_err(|e| e.to_string())?;
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let handshake = connector.connect(name, stream).await;
    let seen = record.seen.lock().expect("not poisoned").take();
    match (seen, handshake) {
        (Some(der), _) => Ok(crate::connection::certificate(&der)),
        (None, Ok(_)) => Ok(None),
        (None, Err(err)) => Err(err.to_string()),
    }
}

/// The same request sent straight to the server, after it failed through
/// the proxy, and what that says about who refuses it.
fn compare_direct(
    proxy: &str,
    host: &str,
    through: &StepResult,
    direct: &StepResult,
) -> (DiagnosisStage, Option<String>) {
    let name = "Without the proxy";
    let status_line = |r: &StepResult| {
        let text = r
            .exchange
            .as_ref()
            .and_then(|e| e.response.as_ref())
            .map(|r| r.status_text.clone())
            .unwrap_or_default();
        format!("{} {text}", r.status.unwrap_or_default())
            .trim()
            .to_owned()
    };
    match direct.status {
        Some(status) if status < 400 && direct.passed() => (
            DiagnosisStage::new(
                name,
                StageOutcome::Passed,
                format!(
                    "Straight to {host}: {} in {} ms.",
                    status_line(direct),
                    direct.duration_ms.round()
                ),
            ),
            Some(format!(
                "The proxy {proxy} refuses this request: sent straight to {host}, it gets {}. \
                 Choose No proxy for requests, or add {host} to NO_PROXY. A client that works, \
                 like Bruno, probably reaches it without this proxy.",
                status_line(direct)
            )),
        ),
        Some(status) if Some(status) == through.status => (
            DiagnosisStage::new(
                name,
                StageOutcome::Failed,
                format!("Straight to {host}: {} as well.", status_line(direct)),
            ),
            Some(format!(
                "{host} answers {} with or without the proxy {proxy}, so the API, or a firewall in \
                 front of it, refuses the request itself. Compare its headers and body with a \
                 client that gets through.",
                status_line(direct)
            )),
        ),
        Some(_) => (
            DiagnosisStage::new(
                name,
                StageOutcome::Warning,
                format!("Straight to {host}: {}.", status_line(direct)),
            ),
            None,
        ),
        None => (
            DiagnosisStage::new(
                name,
                StageOutcome::Info,
                format!(
                    "{host} cannot be reached without a proxy: {}.",
                    direct.error.as_deref().unwrap_or("no response")
                ),
            ),
            None,
        ),
    }
}

fn is_tls_error(error: &str) -> bool {
    ["certificate", "unknownissuer", "tls", "handshake", "ssl"]
        .iter()
        .any(|w| error.contains(w))
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Proxies, firewalls and CDNs leave recognisable headers on the errors
/// they answer with, where the API behind them would not.
fn answered_by(status: u16, headers: &[HeaderPair], body: &str) -> Option<String> {
    let header = |name: &str| {
        headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.to_lowercase())
    };
    let has_prefix = |prefix: &str| {
        headers
            .iter()
            .any(|h| h.name.to_lowercase().starts_with(prefix))
    };
    let server = header("server").unwrap_or_default();
    let body = body.to_lowercase();
    let known: &[(&str, &str)] = &[
        ("zscaler", "Zscaler, a corporate proxy"),
        ("bluecoat", "Blue Coat, a corporate proxy"),
        ("forcepoint", "Forcepoint, a corporate proxy"),
        ("websense", "Websense, a corporate proxy"),
        ("fortigate", "a FortiGate firewall"),
        (
            "blocked by a firewall policy",
            "a FortiGate or FortiProxy web filter",
        ),
        (
            "send request to urlfilter",
            "a FortiGate or FortiProxy web filter",
        ),
        ("fortinet", "a FortiGate firewall"),
        ("palo alto", "a Palo Alto firewall"),
        (
            "mcafee web gateway",
            "McAfee Web Gateway, a corporate proxy",
        ),
        ("squid", "a Squid proxy"),
        ("netskope", "Netskope, a corporate proxy"),
    ];
    for (needle, who) in known {
        if server.contains(needle) || body.contains(needle) || has_prefix(&format!("x-{needle}")) {
            return Some((*who).into());
        }
    }
    if status == 407 || header("proxy-authenticate").is_some() {
        return Some("a proxy that wants a user name and password".into());
    }
    if server.contains("cloudflare") || header("cf-ray").is_some() {
        return Some("Cloudflare, in front of the API".into());
    }
    if server.contains("akamai") || has_prefix("x-akamai") {
        return Some("Akamai, in front of the API".into());
    }
    if header("x-amz-cf-id").is_some() {
        return Some("Amazon CloudFront, in front of the API".into());
    }
    if header("x-amzn-errortype").is_some() || header("x-amzn-requestid").is_some() {
        return Some("AWS API Gateway".into());
    }
    if server.contains("awselb") {
        return Some("an AWS load balancer".into());
    }
    if header("x-azure-ref").is_some() {
        return Some("Azure Front Door, in front of the API".into());
    }
    if server.contains("bigip") || headers.iter().any(|h| h.value.contains("BIGipServer")) {
        return Some("an F5 BIG-IP load balancer or firewall".into());
    }
    if server.contains("kong") || has_prefix("x-kong") {
        return Some("the Kong API gateway".into());
    }
    if let Some(via) = header("via") {
        return Some(format!("something in between (Via: {via})"));
    }
    None
}

fn html_title(body: &str) -> Option<String> {
    let lower = body.to_lowercase();
    let start = lower.find("<title>")? + "<title>".len();
    let end = start + lower[start..].find("</title>")?;
    let title = body
        .get(start..end)?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()).then(|| title.chars().take(120).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(name: &str, value: &str) -> HeaderPair {
        HeaderPair {
            name: name.into(),
            value: value.into(),
        }
    }

    #[test]
    fn recognises_who_answered() {
        assert_eq!(
            answered_by(403, &[h("Server", "Zscaler/6.2")], "").as_deref(),
            Some("Zscaler, a corporate proxy")
        );
        assert_eq!(
            answered_by(403, &[h("cf-ray", "8a1")], "").as_deref(),
            Some("Cloudflare, in front of the API")
        );
        assert!(answered_by(403, &[h("Via", "1.1 gateway")], "")
            .unwrap()
            .contains("Via: 1.1 gateway"));
        assert_eq!(answered_by(403, &[h("Server", "nginx")], "{}"), None);
        let forti = "<h1>Access Denied</h1><p>The page you requested has been blocked by a firewall policy restriction.</p>";
        assert_eq!(
            answered_by(403, &[h("content-type", "text/html")], forti).as_deref(),
            Some("a FortiGate or FortiProxy web filter")
        );
    }

    #[test]
    fn reads_the_html_title() {
        assert_eq!(
            html_title("<html><head><TITLE>\n Access  Denied </TITLE>").as_deref(),
            Some("Access Denied")
        );
        assert_eq!(html_title("{\"error\":\"forbidden\"}"), None);
    }
}
