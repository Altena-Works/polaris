//! Role bindings select existing fixed tier providers; they create no provider.
use polaris_core::session_settings::SavedSessionSettingsV1;
use polaris_provider::{Provider, role::RoleProvider};
use polaris_skills::{AgentType, Tier};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

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
            return Err(format!("role {role} is not in the catalog"));
        }
        let actual = provider
            .resolve_tier(tier)
            .map_err(|e| format!("role {role} cannot use tier {tier}: {e}"))?
            .ok_or_else(|| {
                format!("role {role} is bound to tier {tier}, which has no provider here")
            })?;
        if actual.model().is_none() {
            return Err(format!(
                "role {role} is bound to tier {tier}, whose model cannot be verified"
            ));
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
            tiers.get(tier).cloned().ok_or_else(|| {
                format!("role {role} is bound to tier {tier}, which changed after it was checked")
            })?,
        );
    }
    Ok(Arc::new(
        RoleProvider::new(provider, roles).with_tiers(tiers),
    ))
}

/// A saved binding that no longer resolves must not end the TUI. The run keeps
/// its provider and allows no role, which is what an active binding with
/// nothing resolvable already means; the reason names the role for the notice.
pub(crate) fn activation_or_stop(
    provider: Arc<dyn Provider>,
    settings: &SavedSessionSettingsV1,
    catalog: &[AgentType],
) -> (Arc<dyn Provider>, Option<String>) {
    match activation(provider.clone(), settings, catalog) {
        Ok(child) => (child, None),
        Err(reason) => (
            Arc::new(RoleProvider::new(provider, HashMap::new())),
            Some(reason),
        ),
    }
}

/// The conversations already told that a saved role binding no longer
/// resolves, so each hears it once however many turns follow.
#[derive(Default)]
pub(crate) struct StoppedNotices(HashSet<PathBuf>);

impl StoppedNotices {
    /// The one-line notice for `reason`, the first time `conversation` has one.
    pub(crate) fn once(&mut self, conversation: &Path, reason: Option<String>) -> Option<String> {
        let reason = reason?;
        self.0.insert(conversation.to_path_buf()).then(|| {
            format!("Delegation is off in this chat: {reason}. Start a new chat to clear it.")
        })
    }
}

#[cfg(test)]
#[path = "role_commands/tests.rs"]
mod tests;
