//! `/api/oauth/account` — membership-level API guardrails (out_of_credits, etc.)

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct AccountOrg {
    pub uuid: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub api_disabled_reason: Option<String>,
    #[serde(default)]
    pub api_disabled_until: Option<String>,
    #[serde(default)]
    pub billable_usage_paused_until: Option<String>,
    #[serde(default)]
    pub free_credits_status: Option<String>,
    #[serde(default)]
    pub rate_limit_tier: Option<String>,
    #[serde(default)]
    pub rate_limit_upsell: Option<String>,
    #[serde(default)]
    pub raven_type: Option<String>,
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
    #[serde(default)]
    pub billing_type: Option<String>,
    #[serde(default)]
    pub parent_organization_uuid: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct MembershipRow {
    pub organization: AccountOrg,
    #[serde(default)]
    pub role: Option<String>,
}

/// Response from `GET https://api.anthropic.com/api/oauth/account`
#[derive(Deserialize, Debug)]
pub struct AccountResponse {
    #[serde(default)]
    pub memberships: Vec<MembershipRow>,
}

/// Find the membership row matching the active profile organization UUID.
pub fn org_for_profile_uuid<'a>(
    account: &'a AccountResponse,
    profile_org_uuid: Option<&str>,
) -> Option<&'a AccountOrg> {
    let u = profile_org_uuid?;
    account
        .memberships
        .iter()
        .find(|m| m.organization.uuid.as_deref() == Some(u))
        .map(|m| &m.organization)
}

/// Anthropic returns one membership row per org the user belongs to. On
/// Enterprise, the active profile org is often a personal sub-org while the
/// hard-block (api_disabled_reason / billable_usage_paused_until) lives on the
/// parent enterprise membership row. Use this when the matched org has no
/// block: scan every membership and return the first that does.
pub fn first_blocked_org(account: &AccountResponse) -> Option<&AccountOrg> {
    account.memberships.iter().find_map(|m| {
        let o = &m.organization;
        let has_disabled = o
            .api_disabled_reason
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let has_paused = o
            .billable_usage_paused_until
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        if has_disabled || has_paused {
            Some(o)
        } else {
            None
        }
    })
}

/// Find the membership pointing to a parent org (for Enterprise sub-orgs).
pub fn parent_org<'a>(
    account: &'a AccountResponse,
    matched: &AccountOrg,
) -> Option<&'a AccountOrg> {
    let parent_uuid = matched.parent_organization_uuid.as_deref()?;
    account
        .memberships
        .iter()
        .find(|m| m.organization.uuid.as_deref() == Some(parent_uuid))
        .map(|m| &m.organization)
}
