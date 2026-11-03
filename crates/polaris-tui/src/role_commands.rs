//! Role bindings select existing fixed tier providers; they create no provider.
use polaris_core::session_settings::SavedSessionSettingsV1;
use polaris_provider::{Provider, role::RoleProvider};
use polaris_skills::{AgentType, Tier};
use std::{collections::HashMap, sync::Arc};

fn validate_bindings(
    provider: &dyn Provider,
    settings: &SavedSessionSettingsV1,
    catalog: &[AgentType],
) -> Result<(), String> {
    settings.validate().map_err(|e| e.to_string())?;
    if !settings.role_bindings_active {
        return Ok(());
    }
    for (role, tier) in &settings.role_bindings {
        if !catalog.iter().any(|agent| agent.name == *role) {
            return Err(format!("Saved role is not in the catalog: {role}"));
        }
        let actual = provider
            .resolve_tier(tier)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("No provider configured for saved tier: {tier}"))?;
        if actual.model().is_none() {
            return Err(format!("Tier model unverified: {tier}"));
        }
    }
    Ok(())
}

pub(crate) fn activation(
    provider: Arc<dyn Provider>,
    settings: &SavedSessionSettingsV1,
    catalog: &[AgentType],
) -> Result<Arc<dyn Provider>, String> {
    validate_bindings(provider.as_ref(), settings, catalog)?;
    if !settings.role_bindings_active {
        return Ok(provider);
    }
    let mut tiers = HashMap::new();
    for tier in Tier::ALL {
        if let Some(actual) = provider
            .resolve_tier(tier.name())
            .map_err(|e| e.to_string())?
        {
            tiers.insert(tier.name().to_owned(), actual);
        }
    }
    let mut roles = HashMap::new();
    for (role, tier) in &settings.role_bindings {
        roles.insert(
            role.clone(),
            tiers
                .get(tier)
                .cloned()
                .ok_or("The tier provider changed after confirmation")?,
        );
    }
    Ok(Arc::new(
        RoleProvider::new(provider, roles).with_tiers(tiers),
    ))
}

#[cfg(test)]
#[path = "role_commands/tests.rs"]
mod tests;
