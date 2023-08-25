use log::debug;
use std::fmt::{Display, Formatter};
use std::io::Result;
use std::net::SocketAddr;
use time::{Duration, Instant, OffsetDateTime};

#[derive(Debug, Clone, Default)]
pub(crate) struct UpstreamAddress {
    address: String,
    resolved_addresses: Vec<SocketAddr>,
    resolved_time: Option<Instant>,
    ttl: Option<Duration>,
}

impl Display for UpstreamAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.address.fmt(f)
    }
}

impl UpstreamAddress {
    pub fn new(address: String) -> Self {
        UpstreamAddress {
            address,
            ..Default::default()
        }
    }

    pub fn is_valid(&self) -> bool {
        if let Some(resolved) = self.resolved_time {
            if let Some(ttl) = self.ttl {
                return resolved.elapsed() < ttl;
            }
        }

        false
    }

    fn is_resolved(&self) -> bool {
        self.resolved_addresses.len() > 0
    }

    fn time_remaining(&self) -> Duration {
        if !self.is_valid() {
            return Duration::seconds(0);
        }

        self.ttl.unwrap() - self.resolved_time.unwrap().elapsed()
    }

    pub async fn resolve(&mut self, mode: ResolutionMode) -> Result<Vec<SocketAddr>> {
        if self.is_resolved() && self.is_valid() {
            debug!(
                "Already got address {:?}, still valid for {:.3}s",
                &self.resolved_addresses,
                self.time_remaining().as_seconds_f64()
            );
            return Ok(self.resolved_addresses.clone());
        }

        debug!(
            "Resolving addresses for {} with mode {:?}",
            &self.address, &mode
        );

        let lookup_result = tokio::net::lookup_host(&self.address).await;

        let resolved_addresses: Vec<SocketAddr> = match lookup_result {
            Ok(resolved_addresses) => resolved_addresses.into_iter().collect(),
            Err(e) => {
                debug!("Failed looking up {}: {}", &self.address, &e);
                // Protect against DNS flooding. Cache the result for 1 second.
                self.resolved_time = Some(Instant::now());
                self.ttl = Some(Duration::seconds(3));
                return Err(e);
            }
        };

        debug!("Resolved addresses: {:?}", &resolved_addresses);

        let addresses: Vec<SocketAddr> = match mode {
            ResolutionMode::Ipv4 => resolved_addresses
                .into_iter()
                .filter(|a| a.is_ipv4())
                .collect(),

            ResolutionMode::Ipv6 => resolved_addresses
                .into_iter()
                .filter(|a| a.is_ipv6())
                .collect(),

            _ => resolved_addresses,
        };

        debug!(
            "Got {} addresses for {}: {:?}",
            &mode, &self.address, &addresses
        );
        debug!(
            "Resolved at {}",
            OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .expect("Format")
        );

        self.resolved_addresses = addresses;
        self.resolved_time = Some(Instant::now());
        self.ttl = Some(Duration::minutes(1));

        Ok(self.resolved_addresses.clone())
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) enum ResolutionMode {
    #[default]
    Ipv4AndIpv6,
    Ipv4,
    Ipv6,
}

impl From<&str> for ResolutionMode {
    fn from(value: &str) -> Self {
        match value {
            "tcp4" => ResolutionMode::Ipv4,
            "tcp6" => ResolutionMode::Ipv6,
            "tcp" => ResolutionMode::Ipv4AndIpv6,
            _ => panic!("This should never happen. Please check configuration parser."),
        }
    }
}

impl Display for ResolutionMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionMode::Ipv4 => write!(f, "IPv4Only"),
            ResolutionMode::Ipv6 => write!(f, "IPv6Only"),
            ResolutionMode::Ipv4AndIpv6 => write!(f, "IPv4 and IPv6"),
        }
    }
}
