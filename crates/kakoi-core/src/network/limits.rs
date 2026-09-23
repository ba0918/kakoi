use serde::{Deserialize, Serialize};

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
        self.udp_idle_timeout_seconds.is_some()
            || self.dns_zero_ttl_grace_milliseconds.is_some()
            || self.dns_server_timeout_seconds.is_some()
            || self.dns_resolution_timeout_seconds.is_some()
            || self.dns_max_cname_hops.is_some()
            || self.dns_max_upstream_queries.is_some()
            || self.dns_max_concurrent_resolutions.is_some()
            || self.dns_max_waiters_per_resolution.is_some()
            || self.dns_failure_cache_seconds.is_some()
            || self.recovery_attempt_timeout_seconds.is_some()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self
            .udp_idle_timeout_seconds
            .is_some_and(|v| !(1..=86400).contains(&v))
        {
            return Err("network.udp-idle-timeout-seconds must be in 1..=86400".into());
        }
        if self
            .dns_zero_ttl_grace_milliseconds
            .is_some_and(|v| !(100..=10000).contains(&v))
        {
            return Err("network.dns-zero-ttl-grace-milliseconds must be in 100..=10000".into());
        }
        if self
            .dns_server_timeout_seconds
            .is_some_and(|v| !(1..=300).contains(&v))
        {
            return Err("network.dns-server-timeout-seconds must be in 1..=300".into());
        }
        if self
            .dns_resolution_timeout_seconds
            .is_some_and(|v| !(1..=3600).contains(&v))
        {
            return Err("network.dns-resolution-timeout-seconds must be in 1..=3600".into());
        }
        if self
            .dns_max_cname_hops
            .is_some_and(|v| !(1..=128).contains(&v))
        {
            return Err("network.dns-max-cname-hops must be in 1..=128".into());
        }
        if self
            .dns_max_upstream_queries
            .is_some_and(|v| !(1..=4096).contains(&v))
        {
            return Err("network.dns-max-upstream-queries must be in 1..=4096".into());
        }
        if self
            .dns_max_concurrent_resolutions
            .is_some_and(|v| !(1..=4096).contains(&v))
        {
            return Err("network.dns-max-concurrent-resolutions must be in 1..=4096".into());
        }
        if self
            .dns_max_waiters_per_resolution
            .is_some_and(|v| !(1..=1024).contains(&v))
        {
            return Err("network.dns-max-waiters-per-resolution must be in 1..=1024".into());
        }
        if self
            .dns_failure_cache_seconds
            .is_some_and(|v| !(1..=300).contains(&v))
        {
            return Err("network.dns-failure-cache-seconds must be in 1..=300".into());
        }
        if self
            .recovery_attempt_timeout_seconds
            .is_some_and(|v| !(1..=300).contains(&v))
        {
            return Err("network.recovery-attempt-timeout-seconds must be in 1..=300".into());
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
