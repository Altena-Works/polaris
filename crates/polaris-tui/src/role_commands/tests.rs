//! TUI saved role and tier bindings with fixed-provider checks.

use super::*;
use polaris_provider::{CompletionRequest, CompletionResponse, ProviderError};
use polaris_skills::AgentAccess;

struct Fixed(&'static str);
#[async_trait::async_trait]
impl Provider for Fixed {
    async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        panic!("role controls do not infer")
    }
    fn model(&self) -> Option<String> {
        Some(self.0.into())
    }
    fn set_model(&self, _: &str) {
        panic!("fixed tier must not be retuned")
    }
}

fn agent_type(name: &str) -> AgentType {
    let path = std::path::PathBuf::from("/fixture");
    AgentType {
        name: name.into(),
        description: "test".into(),
        body: String::new(),
        path: path.clone(),
        allowed_tools: vec![],
        access: AgentAccess::Read,
        tier: Tier::Subgiant,
        tier_min: Tier::Dwarf,
        wall_seconds: 30,
        max_turns: 1,
        workflow_phase: None,
        continuation: false,
        output_schema: path.join("schema.json"),
    }
}

fn settings(bindings: &[(&str, &str)]) -> SavedSessionSettingsV1 {
    let mut saved =
        SavedSessionSettingsV1::new("fixed".into(), "parent-model".into(), "auto".into()).unwrap();
    saved.role_bindings_active = true;
    saved.role_bindings = bindings
        .iter()
        .map(|(role, tier)| ((*role).to_owned(), (*tier).to_owned()))
        .collect();
    saved
}

fn provider() -> Arc<dyn Provider> {
    let tier: Arc<dyn Provider> = Arc::new(Fixed("fixed-tier-model"));
    Arc::new(RoleProvider::tiers_only(
        Arc::new(Fixed("parent-model")),
        HashMap::from([("subgiant".into(), tier)]),
    ))
}

#[test]
fn saved_role_bindings_route_each_role_to_the_actual_fixed_tier() {
    let provider = provider();
    let catalog = vec![agent_type("worker")];

    let disabled =
        SavedSessionSettingsV1::new("fixed".into(), "parent-model".into(), "auto".into()).unwrap();
    assert!(!disabled.role_bindings_active);
    let unchanged = activation(provider.clone(), &disabled, &[]).unwrap();
    assert!(Arc::ptr_eq(&unchanged, &provider));
    assert!(unchanged.resolve_role("worker").unwrap().is_none());

    let active = activation(
        provider.clone(),
        &settings(&[("worker", "subgiant")]),
        &catalog,
    )
    .unwrap();
    let routed = active.resolve_role("worker").unwrap().unwrap();
    assert_eq!(routed.model().as_deref(), Some("fixed-tier-model"));
    assert!(Arc::ptr_eq(
        &routed,
        &provider.resolve_tier("subgiant").unwrap().unwrap()
    ));
    assert!(active.resolve_role("missing-role").is_err());
    assert!(active.resolve_tier("subgiant").unwrap().is_some());
}

#[test]
fn saved_role_bindings_that_no_longer_resolve_refuse_the_run() {
    let provider = provider();
    let catalog = vec![agent_type("worker")];
    for bindings in [
        &[("absent", "subgiant")][..],
        &[("worker", "giant")][..],
        &[("worker", "unavailable")][..],
    ] {
        assert!(
            activation(provider.clone(), &settings(bindings), &catalog).is_err(),
            "{bindings:?}"
        );
    }
    // Active with no bindings left: delegation to an unbound role is denied.
    let cleared = activation(provider.clone(), &settings(&[]), &catalog).unwrap();
    assert!(cleared.resolve_role("worker").is_err());
}
