use super::IntoU128BeBytes;
use crate::{
    errors::OFTError,
    seeds::{RATE_LIMIT_EXEMPTION_SEED, RATE_LIMIT_SEED},
    OFTStore,
};
use anchor_lang::prelude::*;

pub const DEFAULT_RATE_LIMIT_ID: u128 = 0;

/// RateLimit PDA (config + state) for id=0 default and per-EID ids.
/// EVM alignment: `mapping(uint256 id => RateLimit) rateLimits` in RateLimiterBaseUpgradeable.sol
///
/// The default (eid=0) RateLimit lives in its own PDA. Per-EID config falls back to
/// the default PDA when `override_default_config=false`.
/// PDA seeds: [RATE_LIMIT_SEED, oft_store.key(), id.to_u128_be_bytes()] (16-byte big-endian u128)
#[account]
#[derive(Default, InitSpace)]
pub struct RateLimit {
    pub config: RateLimitConfig,
    pub state: RateLimitState,
}

impl RateLimit {
    /// PDA seeds for the per-eid rate limit.
    pub fn seeds<I: IntoU128BeBytes>(oft_store: &Pubkey, id: I) -> &'static [&'static [u8]] {
        let key_bytes: &'static [u8; 32] = Box::leak(Box::new(oft_store.to_bytes()));
        let id_bytes: &'static [u8; 16] = Box::leak(Box::new(id.to_u128_be_bytes()));
        Box::leak(Box::new([RATE_LIMIT_SEED, key_bytes.as_slice(), id_bytes.as_slice()]))
    }
}

/// Rate limit address exemption PDA.
/// Account existence indicates the user is exempt from rate limiting.
/// PDA seeds: [RATE_LIMIT_EXEMPTION_SEED, oft_store.key(), user.key()]
///
/// `initialized` lets the account constraint reject idempotent calls with
/// `ExemptionStateIdempotent` — mirroring EVM `_setRateLimitAddressExemptions`.
/// A freshly created PDA (via `init_if_needed`) is zero-initialized
/// (`initialized == false`); the handler sets it to `true` on grant and closes
/// the PDA on revoke, keeping existence and `initialized` aligned.
#[account]
#[derive(Default, InitSpace)]
pub struct RateLimitAddressExemption {
    pub initialized: bool,
}

impl RateLimitAddressExemption {
    /// PDA seeds for the per-user rate-limit address exemption.
    pub fn seeds(oft_store: &Pubkey, user: &Pubkey) -> &'static [&'static [u8]] {
        let key_bytes: &'static [u8; 32] = Box::leak(Box::new(oft_store.to_bytes()));
        let user_bytes: &'static [u8; 32] = Box::leak(Box::new(user.to_bytes()));
        Box::leak(Box::new([
            RATE_LIMIT_EXEMPTION_SEED,
            key_bytes.as_slice(),
            user_bytes.as_slice(),
        ]))
    }
}

// ================================ Composing State Types ================================

#[derive(Clone, AnchorSerialize, AnchorDeserialize, InitSpace)]
pub struct RateLimitConfig {
    /// If false, config from eid=0 RateLimit is used. Ignored on eid=0.
    /// Reference: RateLimiterUtils.sol - OVERRIDE_DEFAULT_CONFIG_BIT
    pub override_default_config: bool,
    pub outbound_enabled: bool,
    pub inbound_enabled: bool,
    pub net_accounting_enabled: bool,
    pub address_exemption_enabled: bool,
    pub outbound_limit: u64,
    pub inbound_limit: u64,
    pub outbound_window: u64,
    pub inbound_window: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            override_default_config: false,
            outbound_enabled: true,
            inbound_enabled: true,
            net_accounting_enabled: true,
            address_exemption_enabled: false,
            outbound_limit: 0,
            inbound_limit: 0,
            outbound_window: 0,
            inbound_window: 0,
        }
    }
}

impl RateLimitConfig {
    /// An outbound send applies no rate limiting and touches no state: outbound is
    /// disabled and there is no inbound net-accounting credit to write (inbound off or
    /// net accounting off). Mirrors the early-return gate in `apply_rate_limit`.
    fn outbound_skips_state(&self) -> bool {
        !self.outbound_enabled && (!self.inbound_enabled || !self.net_accounting_enabled)
    }

    /// Inbound mirror of [`Self::outbound_skips_state`].
    fn inbound_skips_state(&self) -> bool {
        !self.inbound_enabled && (!self.outbound_enabled || !self.net_accounting_enabled)
    }
}

#[derive(Default, Clone, AnchorSerialize, AnchorDeserialize, InitSpace)]
pub struct RateLimitState {
    /// Current usage of the outbound rate limit.
    /// Reference: IRateLimiter.sol - RateLimit.outboundUsage
    pub outbound_usage: u64,
    /// Current usage of the inbound rate limit.
    /// Reference: IRateLimiter.sol - RateLimit.inboundUsage
    pub inbound_usage: u64,
    /// Timestamp of the last rate limit update (seconds).
    /// Reference: IRateLimiter.sol - RateLimit.lastUpdated
    pub last_updated: u64,
}

// ================================ View Return Types ================================

/// Global configuration for the rate limiter.
#[derive(Clone, AnchorSerialize, AnchorDeserialize)]
pub struct RateLimitGlobalConfig {
    /// Whether to use global state for the rate limiter, instead of per-ID rules.
    pub use_global_state: bool,
    /// Whether the rate limiter is globally disabled.
    pub is_globally_disabled: bool,
}

/// Decayed usages and available capacities for a rate limit.
/// If a rate limit is disabled in a given direction, the available amount is `u64::MAX`.
#[derive(Default, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct RateLimitUsages {
    /// Current decayed usage of the outbound rate limit.
    pub outbound_usage: u64,
    /// Available capacity of the outbound rate limit.
    pub outbound_available: u64,
    /// Current decayed usage of the inbound rate limit.
    pub inbound_usage: u64,
    /// Available capacity of the inbound rate limit.
    pub inbound_available: u64,
}

impl RateLimitUsages {
    /// Zero usage with unlimited capacity for requests without state.
    fn unlimited() -> Self {
        Self {
            outbound_usage: 0,
            outbound_available: u64::MAX,
            inbound_usage: 0,
            inbound_available: u64::MAX,
        }
    }
}

// ================================ Rate Limiter Logic ================================
/// Calculates decayed usage and remaining capacity for a single direction.
///
/// # Reference
/// Based on `_getRateLimitUsage` in RateLimiterBaseUpgradeable.sol
/// ```solidity
/// function _getRateLimitUsage(
///     uint40 _lastUpdated, uint96 _amountInFlight, uint96 _limit, uint32 _window
/// ) returns (uint256 currentUsage, uint256 availableAmount) {
///     uint256 timeSinceLastUpdate = block.timestamp - _lastUpdated;
///     uint256 decay = (_limit * timeSinceLastUpdate) / (_window > 0 ? _window : 1);
///     currentUsage = Math.saturatingSub(_amountInFlight, decay);
///     availableAmount = Math.saturatingSub(_limit, currentUsage);
/// }
/// ```
fn compute_rate_limit_usage(
    last_updated: u64,
    amount_in_flight: u64,
    limit: u64,
    window: u64,
) -> Result<(u64, u64)> {
    let now = now()?;
    let elapsed = now.checked_sub(last_updated).ok_or(OFTError::LastUpdatedInFuture)?;
    let window = if window > 0 { window } else { 1 };
    let decay = u128::from(limit).saturating_mul(u128::from(elapsed)) / u128::from(window);
    let current_usage = u64::try_from(u128::from(amount_in_flight).saturating_sub(decay))?;
    let available = limit.saturating_sub(current_usage);
    Ok((current_usage, available))
}

/// Calculates decayed usages for both directions at current time.
/// Reference: `_getRateLimitUsages` in RateLimiterBaseUpgradeable.sol
pub fn compute_rate_limit_usages(
    state: &RateLimitState,
    config: &RateLimitConfig,
) -> Result<RateLimitUsages> {
    let (outbound_usage, outbound_available) = compute_rate_limit_usage(
        state.last_updated,
        state.outbound_usage,
        config.outbound_limit,
        config.outbound_window,
    )?;
    let (inbound_usage, inbound_available) = compute_rate_limit_usage(
        state.last_updated,
        state.inbound_usage,
        config.inbound_limit,
        config.inbound_window,
    )?;
    Ok(RateLimitUsages { outbound_usage, outbound_available, inbound_usage, inbound_available })
}

pub(crate) fn now() -> Result<u64> {
    Clock::get()?
        .unix_timestamp
        .try_into()
        .map_err(|_| OFTError::InvalidTimestamp.into())
}

// ================================ Config Resolution ================================

/// Returns effective config: per-EID's own config if override_default_config, else the
/// default from the id=0 RateLimit PDA. Reference: `_getRateLimitStateAndConfig` in
/// RateLimiterBaseUpgradeable.sol
fn get_effective_config<'a>(
    default_rate_limit: &'a RateLimit,
    rate_limit: &'a RateLimit,
) -> &'a RateLimitConfig {
    if rate_limit.config.override_default_config {
        &rate_limit.config
    } else {
        &default_rate_limit.config
    }
}

// ================================ Read API ================================

/// Gets current usages and available capacities for both directions.
/// Behavior:
/// - `outbound_available` and/or `inbound_available` are `u64::MAX` when disabled
/// - when globally disabled, both availabilities are `u64::MAX`, but usage values still reflect
///   decayed on-chain state when state exists; missing state returns zero usages
///
/// In per-EID state mode, a missing PDA returns `RateLimitNotInitialized` unless
/// rate limiting is globally disabled or both directions need no state. Address
/// exemptions are not considered because this view has no user argument, so an
/// exempt send may still succeed.
///
/// Reference: `getRateLimitUsages` in RateLimiterBaseUpgradeable.sol
pub fn get_rate_limit_usages(
    oft_store: &OFTStore,
    default_rate_limit: &RateLimit,
    rate_limit: Option<&RateLimit>,
) -> Result<RateLimitUsages> {
    let (state, config) =
        get_effective_state_and_config(oft_store.use_global_state, default_rate_limit, rate_limit);

    // Missing state is valid when globally disabled or both directions bypass state.
    let Some(state) = state else {
        require!(
            oft_store.is_globally_disabled
                || (config.outbound_skips_state() && config.inbound_skips_state()),
            OFTError::RateLimitNotInitialized
        );
        return Ok(RateLimitUsages::unlimited());
    };

    let mut usages = compute_rate_limit_usages(state, &config)?;
    if oft_store.is_globally_disabled {
        usages.outbound_available = u64::MAX;
        usages.inbound_available = u64::MAX;
    } else {
        if !config.outbound_enabled {
            usages.outbound_available = u64::MAX;
        }
        if !config.inbound_enabled {
            usages.inbound_available = u64::MAX;
        }
    }

    Ok(usages)
}

/// Returns outbound availability for `quote_oft`.
///
/// `quote_oft` is a send-side view: it only needs the amount that an outbound
/// send could pass through. That is narrower than `get_rate_limit_usages`, which
/// reports both directions.
///
/// A missing per-EID PDA is allowed only when a normal outbound send would not
/// touch state. If net accounting needs to update inbound usage, quote returns
/// `RateLimitNotInitialized` just as send does.
///
/// Address exemptions are not considered because this view has no user
/// argument. An exempt send may therefore succeed even when this view returns
/// `RateLimitNotInitialized`.
pub fn get_outbound_available_for_quote(
    oft_store: &OFTStore,
    default_rate_limit: &RateLimit,
    rate_limit: Option<&RateLimit>,
) -> Result<u64> {
    if oft_store.is_globally_disabled {
        return Ok(u64::MAX);
    }

    let (state, config) =
        get_effective_state_and_config(oft_store.use_global_state, default_rate_limit, rate_limit);

    // A missing per-EID PDA is fine only when the real send would also skip state;
    // otherwise net accounting still needs it, so treat it as uninitialized.
    let Some(state) = state else {
        require!(config.outbound_skips_state(), OFTError::RateLimitNotInitialized);
        return Ok(u64::MAX);
    };

    // Outbound is off, so a send is never blocked by it -> unlimited. Keep this below the
    // None check: a missing per-EID PDA can still be an error even when outbound is off.
    if !config.outbound_enabled {
        return Ok(u64::MAX);
    }

    Ok(compute_rate_limit_usages(state, &config)?.outbound_available)
}

// ================================ Write API ================================

/// Applies outflow rate limiting with global/per-EID fallback.
pub fn outflow(
    oft_store: &OFTStore,
    default_rate_limit: &mut RateLimit,
    rate_limit: Option<&mut RateLimit>,
    amount: u64,
    is_address_exempt: bool,
) -> Result<()> {
    let use_global_state = oft_store.use_global_state;
    let is_globally_disabled = oft_store.is_globally_disabled;
    apply_rate_limit(
        use_global_state,
        is_globally_disabled,
        default_rate_limit,
        rate_limit,
        amount,
        true,
        is_address_exempt,
    )
}

/// Applies inflow rate limiting with global/per-EID fallback.
pub fn inflow(
    oft_store: &OFTStore,
    default_rate_limit: &mut RateLimit,
    rate_limit: Option<&mut RateLimit>,
    amount: u64,
    is_address_exempt: bool,
) -> Result<()> {
    let use_global_state = oft_store.use_global_state;
    let is_globally_disabled = oft_store.is_globally_disabled;
    apply_rate_limit(
        use_global_state,
        is_globally_disabled,
        default_rate_limit,
        rate_limit,
        amount,
        false,
        is_address_exempt,
    )
}

/// Resolves effective state + config, mirroring EVM `_getRateLimitStateAndConfig`.
/// - use_global_state=true: state=config=default (from id=0 RateLimit PDA)
/// - use_global_state=false: state=per-EID; config=per-EID when override_default_config, else
///   default (from id=0 RateLimit PDA)
fn get_effective_state_and_config_mut<'a>(
    use_global_state: bool,
    default_rate_limit: &'a mut RateLimit,
    rate_limit: Option<&'a mut RateLimit>,
) -> (Option<&'a mut RateLimitState>, RateLimitConfig) {
    match (use_global_state, rate_limit) {
        (true, _) => (Some(&mut default_rate_limit.state), default_rate_limit.config.clone()),
        (false, Some(rate_limit)) => {
            let config = get_effective_config(default_rate_limit, rate_limit).clone();
            (Some(&mut rate_limit.state), config)
        },
        (false, None) => (None, default_rate_limit.config.clone()),
    }
}

/// Read-only variant of rate-limit state/config resolution.
fn get_effective_state_and_config<'a>(
    use_global_state: bool,
    default_rate_limit: &'a RateLimit,
    rate_limit: Option<&'a RateLimit>,
) -> (Option<&'a RateLimitState>, RateLimitConfig) {
    match (use_global_state, rate_limit) {
        (true, _) => (Some(&default_rate_limit.state), default_rate_limit.config.clone()),
        (false, Some(rate_limit)) => {
            let config = get_effective_config(default_rate_limit, rate_limit).clone();
            (Some(&rate_limit.state), config)
        },
        (false, None) => (None, default_rate_limit.config.clone()),
    }
}

/// Resolves global vs per-EID state/config, then delegates to core apply logic.
/// Reference: `_applyRateLimit` in RateLimiterBaseUpgradeable.sol
fn apply_rate_limit(
    use_global_state: bool,
    is_globally_disabled: bool,
    default_rate_limit: &mut RateLimit,
    rate_limit: Option<&mut RateLimit>,
    amount: u64,
    is_outflow: bool,
    is_address_exempt: bool,
) -> Result<()> {
    // EVM parity: `_applyRateLimit` returns early when globally disabled.
    if is_globally_disabled {
        return Ok(());
    }

    let (state, config) =
        get_effective_state_and_config_mut(use_global_state, default_rate_limit, rate_limit);

    // EVM parity: optimistically assign outflow directions first, then swap for inflows.
    // Outflow:  forward=outbound, backward=inbound
    // Inflow:   forward=inbound,  backward=outbound
    let (mut forward_enabled, mut backward_enabled) =
        (config.outbound_enabled, config.inbound_enabled);
    if !is_outflow {
        (forward_enabled, backward_enabled) = (backward_enabled, forward_enabled);
    }
    let net_accounting_enabled = config.net_accounting_enabled;
    let address_exemption_enabled = config.address_exemption_enabled;

    // EVM parity: same early return gates as `_applyRateLimit`.
    if (!forward_enabled && (!backward_enabled || !net_accounting_enabled))
        || (address_exemption_enabled && is_address_exempt)
    {
        return Ok(());
    }

    let state = state.ok_or(OFTError::RateLimitNotInitialized)?;
    let now = now()?;

    // EVM parity: usages are computed in outflow directions first.
    let usages = compute_rate_limit_usages(state, &config)?;
    let (
        mut forward_usage,
        mut forward_available_amount,
        mut backward_usage,
        mut _backward_available_amount,
    ) = (
        usages.outbound_usage,
        usages.outbound_available,
        usages.inbound_usage,
        usages.inbound_available,
    );

    // EVM parity: swap usage/capacity directions for inflows.
    if !is_outflow {
        (forward_usage, forward_available_amount, backward_usage, _backward_available_amount) =
            (backward_usage, _backward_available_amount, forward_usage, forward_available_amount);
    }

    // EVM parity: forward direction is capacity-checked and consumed.
    if forward_enabled {
        require!(forward_available_amount >= amount, OFTError::RateLimitExceeded);
        forward_usage = forward_usage.saturating_add(amount);
    }

    // EVM parity: backward direction is credited only when net accounting is enabled.
    if backward_enabled && net_accounting_enabled {
        backward_usage = backward_usage.saturating_sub(amount);
    }

    // EVM parity: write back in outflow shape, swap for inflow.
    (state.outbound_usage, state.inbound_usage) =
        if is_outflow { (forward_usage, backward_usage) } else { (backward_usage, forward_usage) };
    state.last_updated = now;

    Ok(())
}

// ===============================================================
// Tests
// ===============================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a default rate limit config for testing with specified limits and windows.
    fn make_config(
        outbound_limit: u64,
        outbound_window: u64,
        inbound_limit: u64,
        inbound_window: u64,
        address_exemption_enabled: bool,
    ) -> RateLimitConfig {
        RateLimitConfig {
            override_default_config: false,
            outbound_enabled: true,
            inbound_enabled: true,
            net_accounting_enabled: true,
            address_exemption_enabled,
            outbound_limit,
            inbound_limit,
            outbound_window,
            inbound_window,
        }
    }

    /// Creates a zero-initialized rate limit state for testing.
    fn make_state() -> RateLimitState {
        RateLimitState { outbound_usage: 0, inbound_usage: 0, last_updated: 0 }
    }

    fn make_oft_store(use_global_state: bool, is_globally_disabled: bool) -> OFTStore {
        OFTStore {
            oft_type: oft::types::OftType::LockUnlock,
            ld2sd_rate: 1,
            shared_decimals: 6,
            token_mint: Pubkey::default(),
            initializer: Pubkey::default(),
            token_program: Pubkey::default(),
            bump: 0,
            transfer_hook_program: Pubkey::default(),
            alts: vec![],
            current_default_admin: Pubkey::default(),
            pending_default_admin: Pubkey::default(),
            default_paused: false,
            default_fee_bps: 0,
            fee_deposit: Pubkey::default(),
            use_global_state,
            is_globally_disabled,
        }
    }

    fn compute_rate_limit_usage_with_now(
        last_updated: u64,
        amount_in_flight: u64,
        limit: u64,
        window: u64,
        now: u64,
    ) -> (u64, u64) {
        let elapsed = now.saturating_sub(last_updated);
        let window = if window > 0 { window } else { 1 };
        let decay = ((limit as u128).saturating_mul(elapsed as u128)) / (window as u128);
        let current_usage = (amount_in_flight as u128).saturating_sub(decay) as u64;
        let available = limit.saturating_sub(current_usage);
        (current_usage, available)
    }

    fn apply_rate_limit_to_state(
        state: &mut RateLimitState,
        config: &RateLimitConfig,
        amount: u64,
        is_outflow: bool,
        is_address_exempt: bool,
        now: u64,
    ) -> Result<()> {
        let (mut forward_enabled, mut backward_enabled) =
            (config.outbound_enabled, config.inbound_enabled);
        if !is_outflow {
            (forward_enabled, backward_enabled) = (backward_enabled, forward_enabled);
        }
        let net_accounting_enabled = config.net_accounting_enabled;
        let address_exemption_enabled = config.address_exemption_enabled;

        if (!forward_enabled && (!backward_enabled || !net_accounting_enabled))
            || (address_exemption_enabled && is_address_exempt)
        {
            return Ok(());
        }

        let (outbound_usage, outbound_available) = compute_rate_limit_usage_with_now(
            state.last_updated,
            state.outbound_usage,
            config.outbound_limit,
            config.outbound_window,
            now,
        );
        let (inbound_usage, inbound_available) = compute_rate_limit_usage_with_now(
            state.last_updated,
            state.inbound_usage,
            config.inbound_limit,
            config.inbound_window,
            now,
        );
        let usages = RateLimitUsages {
            outbound_usage,
            outbound_available,
            inbound_usage,
            inbound_available,
        };
        let (
            mut forward_usage,
            mut forward_available_amount,
            mut backward_usage,
            mut _backward_available_amount,
        ) = (
            usages.outbound_usage,
            usages.outbound_available,
            usages.inbound_usage,
            usages.inbound_available,
        );

        if !is_outflow {
            (forward_usage, forward_available_amount, backward_usage, _backward_available_amount) = (
                backward_usage,
                _backward_available_amount,
                forward_usage,
                forward_available_amount,
            );
        }

        if forward_enabled {
            require!(forward_available_amount >= amount, OFTError::RateLimitExceeded);
            forward_usage = forward_usage.saturating_add(amount);
        }

        if backward_enabled && net_accounting_enabled {
            backward_usage = backward_usage.saturating_sub(amount);
        }

        (state.outbound_usage, state.inbound_usage) = if is_outflow {
            (forward_usage, backward_usage)
        } else {
            (backward_usage, forward_usage)
        };
        state.last_updated = now;

        Ok(())
    }

    // ==================== Address Exemption Tests ====================

    #[test]
    fn test_apply_rate_limit_exempt_user_bypasses_rate_limit() {
        let config = make_config(1000, 60, 1000, 60, true);
        let mut state = make_state();

        let result = apply_rate_limit_to_state(&mut state, &config, 100, true, true, 0);
        assert!(result.is_ok());
        assert_eq!(state.outbound_usage, 0);
        assert_eq!(state.inbound_usage, 0);
        assert_eq!(state.last_updated, 0);
    }

    #[test]
    fn test_exempt_user_can_exceed_limit() {
        let config = make_config(100, 60, 100, 60, true);
        let mut state = make_state();

        let result = apply_rate_limit_to_state(&mut state, &config, 500, true, true, 0);
        assert!(result.is_ok());
        assert_eq!(state.outbound_usage, 0);
    }

    #[test]
    fn test_inbound_exemption_bypasses_rate_limit() {
        let config = make_config(1000, 60, 1000, 60, true);
        let mut state = make_state();

        let result = apply_rate_limit_to_state(&mut state, &config, 100, false, true, 0);
        assert!(result.is_ok());
        assert_eq!(state.inbound_usage, 0);
        assert_eq!(state.outbound_usage, 0);
    }

    #[test]
    fn test_exempt_user_no_net_accounting_effect() {
        let config = make_config(1000, 60, 1000, 60, true);
        let mut state = RateLimitState { outbound_usage: 0, inbound_usage: 100, last_updated: 0 };

        let result = apply_rate_limit_to_state(&mut state, &config, 50, true, true, 0);
        assert!(result.is_ok());
        assert_eq!(state.inbound_usage, 100);
    }

    // ==================== Early Return Tests ====================

    #[test]
    fn test_both_directions_disabled_early_return() {
        let mut config = make_config(1000, 60, 1000, 60, false);
        config.outbound_enabled = false;
        config.inbound_enabled = false;
        let mut state = make_state();

        let result = apply_rate_limit_to_state(&mut state, &config, 100, true, false, 0);
        assert!(result.is_ok());
        assert_eq!(state.outbound_usage, 0);
    }

    #[test]
    fn test_forward_disabled_backward_disabled_early_return() {
        let mut config = make_config(1000, 60, 1000, 60, false);
        config.outbound_enabled = false;
        config.inbound_enabled = false;
        let mut state = make_state();

        let result = apply_rate_limit_to_state(&mut state, &config, 100, true, false, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_forward_disabled_no_net_accounting_early_return() {
        let mut config = make_config(1000, 60, 1000, 60, false);
        config.outbound_enabled = false;
        config.inbound_enabled = true;
        config.net_accounting_enabled = false;
        let mut state = make_state();

        let result = apply_rate_limit_to_state(&mut state, &config, 100, true, false, 0);
        assert!(result.is_ok());
        assert_eq!(state.outbound_usage, 0);
    }

    #[test]
    fn test_missing_per_eid_state_allows_disabled_direction_noop() {
        let mut config = make_config(1000, 60, 1000, 60, false);
        config.outbound_enabled = false;
        config.inbound_enabled = true;
        config.net_accounting_enabled = false;
        let mut default_rate_limit = RateLimit { config, state: make_state() };

        let result =
            apply_rate_limit(false, false, &mut default_rate_limit, None, 100, true, false);
        assert!(result.is_ok());
        assert_eq!(default_rate_limit.state.outbound_usage, 0);
        assert_eq!(default_rate_limit.state.inbound_usage, 0);
        assert_eq!(default_rate_limit.state.last_updated, 0);
    }

    #[test]
    fn test_missing_per_eid_state_quote_allows_disabled_outflow_noop() {
        let oft_store = make_oft_store(false, false);
        let mut config = make_config(1000, 60, 1000, 60, false);
        config.outbound_enabled = false;
        config.inbound_enabled = true;
        config.net_accounting_enabled = false;
        let default_rate_limit = RateLimit { config, state: make_state() };

        let outbound_available =
            get_outbound_available_for_quote(&oft_store, &default_rate_limit, None).unwrap();

        assert_eq!(outbound_available, u64::MAX);
    }

    #[test]
    fn test_missing_per_eid_state_usages_error_when_only_outflow_noops() {
        let oft_store = make_oft_store(false, false);
        let mut config = make_config(1000, 60, 1000, 60, false);
        config.outbound_enabled = false;
        config.inbound_enabled = true;
        config.net_accounting_enabled = false;
        let default_rate_limit = RateLimit { config, state: make_state() };

        let result = get_rate_limit_usages(&oft_store, &default_rate_limit, None);

        assert!(result.is_err());
    }

    #[test]
    fn test_missing_per_eid_state_quote_errors_when_net_accounting_needs_state() {
        let oft_store = make_oft_store(false, false);
        let mut config = make_config(1000, 60, 1000, 60, false);
        config.outbound_enabled = false;
        config.inbound_enabled = true;
        config.net_accounting_enabled = true;
        let default_rate_limit = RateLimit { config, state: make_state() };

        let result = get_outbound_available_for_quote(&oft_store, &default_rate_limit, None);

        assert!(result.is_err());
    }

    #[test]
    fn test_missing_per_eid_state_allows_address_exempt_noop() {
        let config = make_config(1000, 60, 1000, 60, true);
        let mut default_rate_limit = RateLimit { config, state: make_state() };

        let result = apply_rate_limit(false, false, &mut default_rate_limit, None, 100, true, true);
        assert!(result.is_ok());
        assert_eq!(default_rate_limit.state.outbound_usage, 0);
        assert_eq!(default_rate_limit.state.inbound_usage, 0);
        assert_eq!(default_rate_limit.state.last_updated, 0);
    }

    #[test]
    fn test_missing_per_eid_state_errors_when_update_required() {
        let config = make_config(1000, 60, 1000, 60, false);
        let mut default_rate_limit = RateLimit { config, state: make_state() };

        let result =
            apply_rate_limit(false, false, &mut default_rate_limit, None, 100, true, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_global_disabled_usages_ignore_missing_per_eid_state() {
        let oft_store = make_oft_store(false, true);
        let default_rate_limit =
            RateLimit { config: make_config(1000, 60, 1000, 60, false), state: make_state() };

        let usages = get_rate_limit_usages(&oft_store, &default_rate_limit, None).unwrap();
        assert_eq!(usages.outbound_usage, 0);
        assert_eq!(usages.outbound_available, u64::MAX);
        assert_eq!(usages.inbound_usage, 0);
        assert_eq!(usages.inbound_available, u64::MAX);
    }

    // ==================== Core Rate Limit Tests ====================

    #[test]
    fn test_outflow_consumes_capacity() {
        let config = make_config(1000, 60, 1000, 60, false);
        let mut state = make_state();

        let result = apply_rate_limit_to_state(&mut state, &config, 500, true, false, 100);
        assert!(result.is_ok());
        assert_eq!(state.outbound_usage, 500);
        assert_eq!(state.last_updated, 100);
    }

    #[test]
    fn test_outflow_exceeds_limit() {
        let config = make_config(1000, 60, 1000, 60, false);
        let mut state = make_state();

        let result = apply_rate_limit_to_state(&mut state, &config, 1001, true, false, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_decay_recovers_capacity() {
        let config = make_config(1000, 60, 1000, 60, false);
        let mut state = RateLimitState { outbound_usage: 1000, inbound_usage: 0, last_updated: 0 };

        // After 30 seconds, half the capacity should have decayed (500 available)
        let result = apply_rate_limit_to_state(&mut state, &config, 500, true, false, 30);
        assert!(result.is_ok());
        // decayed usage = 1000 - (1000*30/60) = 500, then +500 = 1000
        assert_eq!(state.outbound_usage, 1000);
    }

    #[test]
    fn test_net_accounting_credits_backward() {
        let config = make_config(1000, 60, 1000, 60, false);
        let mut state = RateLimitState { outbound_usage: 0, inbound_usage: 800, last_updated: 100 };

        let result = apply_rate_limit_to_state(&mut state, &config, 300, true, false, 100);
        assert!(result.is_ok());
        assert_eq!(state.outbound_usage, 300);
        assert_eq!(state.inbound_usage, 500); // 800 - 300
    }
}
