use serde::{Deserialize, Serialize};

/// Upper bounds the settings are validated against. The network runtime relies on
/// them too, and checks what it is given against the same values.
pub const MAX_UDP_IDLE_TIMEOUT_SECONDS: u32 = 86400;
pub const MAX_DNS_RESOLUTION_TIMEOUT_SECONDS: u32 = 3600;
pub const MAX_DNS_CONCURRENT_RESOLUTIONS: u32 = 4096;
pub const MAX_DNS_WAITERS_PER_RESOLUTION: u32 = 1024;
pub const MAX_RECOVERY_ATTEMPT_TIMEOUT_SECONDS: u32 = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkLimits {
    pub udp_idle_timeout_seconds: u32,
    pub dns_zero_ttl_grace_milliseconds: u32,
    pub dns_server_timeout_seconds: u32,
    pub dns_resolution_timeout_seconds: u32,
    pub dns_max_cname_hops: u32,
    pub dns_max_upstream_queries: u32,
    pub dns_max_concurrent_resolutions: u32,
    pub dns_max_waiters_per_resolution: u32,
    pub dns_failure_cache_seconds: u32,
    pub recovery_attempt_timeout_seconds: u32,
}

impl Default for NetworkLimits {
    fn default() -> Self {
        Self {
            udp_idle_timeout_seconds: 120,
            dns_zero_ttl_grace_milliseconds: 1000,
            dns_server_timeout_seconds: 2,
            dns_resolution_timeout_seconds: 10,
            dns_max_cname_hops: 16,
            dns_max_upstream_queries: 64,
            dns_max_concurrent_resolutions: 256,
            dns_max_waiters_per_resolution: 64,
            dns_failure_cache_seconds: 5,
            recovery_attempt_timeout_seconds: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LimitOverrides {
    pub udp_idle_timeout_seconds: Option<u32>,
    pub dns_zero_ttl_grace_milliseconds: Option<u32>,
    pub dns_server_timeout_seconds: Option<u32>,
    pub dns_resolution_timeout_seconds: Option<u32>,
    pub dns_max_cname_hops: Option<u32>,
    pub dns_max_upstream_queries: Option<u32>,
    pub dns_max_concurrent_resolutions: Option<u32>,
    pub dns_max_waiters_per_resolution: Option<u32>,
    pub dns_failure_cache_seconds: Option<u32>,
    pub recovery_attempt_timeout_seconds: Option<u32>,
}

impl LimitOverrides {
    pub fn is_present(&self) -> bool {
        *self != Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        let bounds = [
            (
                self.udp_idle_timeout_seconds,
                "udp-idle-timeout-seconds",
                1,
                MAX_UDP_IDLE_TIMEOUT_SECONDS,
            ),
            (
                self.dns_zero_ttl_grace_milliseconds,
                "dns-zero-ttl-grace-milliseconds",
                100,
                10000,
            ),
            (
                self.dns_server_timeout_seconds,
                "dns-server-timeout-seconds",
                1,
                300,
            ),
            (
                self.dns_resolution_timeout_seconds,
                "dns-resolution-timeout-seconds",
                1,
                MAX_DNS_RESOLUTION_TIMEOUT_SECONDS,
            ),
            (self.dns_max_cname_hops, "dns-max-cname-hops", 1, 128),
            (
                self.dns_max_upstream_queries,
                "dns-max-upstream-queries",
                1,
                4096,
            ),
            (
                self.dns_max_concurrent_resolutions,
                "dns-max-concurrent-resolutions",
                1,
                MAX_DNS_CONCURRENT_RESOLUTIONS,
            ),
            (
                self.dns_max_waiters_per_resolution,
                "dns-max-waiters-per-resolution",
                1,
                MAX_DNS_WAITERS_PER_RESOLUTION,
            ),
            (
                self.dns_failure_cache_seconds,
                "dns-failure-cache-seconds",
                1,
                300,
            ),
            (
                self.recovery_attempt_timeout_seconds,
                "recovery-attempt-timeout-seconds",
                1,
                MAX_RECOVERY_ATTEMPT_TIMEOUT_SECONDS,
            ),
        ];
        for (value, key, min, max) in bounds {
            if value.is_some_and(|value| !(min..=max).contains(&value)) {
                return Err(format!("network.{key} must be in {min}..={max}"));
            }
        }
        Ok(())
    }
    pub fn apply(&self, lower: &NetworkLimits) -> Result<NetworkLimits, String> {
        self.validate()?;
        Ok(NetworkLimits {
            udp_idle_timeout_seconds: self
                .udp_idle_timeout_seconds
                .unwrap_or(lower.udp_idle_timeout_seconds),
            dns_zero_ttl_grace_milliseconds: self
                .dns_zero_ttl_grace_milliseconds
                .unwrap_or(lower.dns_zero_ttl_grace_milliseconds),
            dns_server_timeout_seconds: self
                .dns_server_timeout_seconds
                .unwrap_or(lower.dns_server_timeout_seconds),
            dns_resolution_timeout_seconds: self
                .dns_resolution_timeout_seconds
                .unwrap_or(lower.dns_resolution_timeout_seconds),
            dns_max_cname_hops: self.dns_max_cname_hops.unwrap_or(lower.dns_max_cname_hops),
            dns_max_upstream_queries: self
                .dns_max_upstream_queries
                .unwrap_or(lower.dns_max_upstream_queries),
            dns_max_concurrent_resolutions: self
                .dns_max_concurrent_resolutions
                .unwrap_or(lower.dns_max_concurrent_resolutions),
            dns_max_waiters_per_resolution: self
                .dns_max_waiters_per_resolution
                .unwrap_or(lower.dns_max_waiters_per_resolution),
            dns_failure_cache_seconds: self
                .dns_failure_cache_seconds
                .unwrap_or(lower.dns_failure_cache_seconds),
            recovery_attempt_timeout_seconds: self
                .recovery_attempt_timeout_seconds
                .unwrap_or(lower.recovery_attempt_timeout_seconds),
        })
    }
}

impl NetworkLimits {
    pub fn validate_deadlines(&self) -> Result<(), String> {
        if self.dns_server_timeout_seconds > self.dns_resolution_timeout_seconds {
            return Err(
                "dns-server-timeout-seconds exceeds dns-resolution-timeout-seconds after merging"
                    .into(),
            );
        }
        Ok(())
    }
}
