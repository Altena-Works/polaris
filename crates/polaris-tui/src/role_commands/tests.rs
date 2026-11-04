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
    provider_over(Arc::new(Fixed("parent-model")))
}

fn provider_over(parent: Arc<dyn Provider>) -> Arc<dyn Provider> {
    let tier: Arc<dyn Provider> = Arc::new(Fixed("fixed-tier-model"));
    Arc::new(RoleProvider::tiers_only(
        parent,
        HashMap::from([("subgiant".into(), tier)]),
    ))
}

/// A parent that answers, so a test can see that a run still reaches it.
struct Answers;
#[async_trait::async_trait]
impl Provider for Answers {
    async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        Ok(CompletionResponse {
            text: "parent answered".into(),
            ..Default::default()
        })
    }
    fn model(&self) -> Option<String> {
        Some("parent-model".into())
    }
}

fn request() -> CompletionRequest {
    CompletionRequest {
        system: String::new(),
        messages: Vec::new(),
        tools: Vec::new(),
    }
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

#[tokio::test]
async fn saved_role_bindings_that_no_longer_resolve_stop_delegation_but_not_the_run() {
    let provider = provider_over(Arc::new(Answers));
    let catalog = vec![agent_type("worker")];
    // Each binding fails for a different reason; the first two name their role.
    for (bindings, role) in [
        (&[("absent", "subgiant")][..], Some("absent")),
        (&[("worker", "giant")][..], Some("worker")),
        (&[("worker", "unavailable")][..], None),
    ] {
        let saved = settings(bindings);
        assert!(
            activation(provider.clone(), &saved, &catalog).is_err(),
            "{bindings:?}"
        );
        let (run, reason) = activation_or_stop(provider.clone(), &saved, &catalog);
        // The turn still runs, on the parent.
        let reply = run.complete(request()).await.unwrap();
        assert_eq!(reply.text, "parent answered", "{bindings:?}");
        // No role is allowed, the one that is bound or any other.
        for refused in ["worker", "absent"] {
            assert!(run.resolve_role(refused).is_err(), "{bindings:?} {refused}");
        }
        let reason = reason.expect("the reason is returned");
        if let Some(role) = role {
            assert!(reason.contains(role), "{reason}");
        }
    }
    // Active with no bindings left: delegation to an unbound role is denied.
    let cleared = activation(provider.clone(), &settings(&[]), &catalog).unwrap();
    assert!(cleared.resolve_role("worker").is_err());
}

#[test]
fn a_binding_that_resolves_delegates_as_before_and_gives_no_reason() {
    let provider = provider();
    let catalog = vec![agent_type("worker")];
    let saved = settings(&[("worker", "subgiant")]);
    let (run, reason) = activation_or_stop(provider.clone(), &saved, &catalog);
    assert!(reason.is_none());
    let routed = run.resolve_role("worker").unwrap().unwrap();
    assert_eq!(routed.model().as_deref(), Some("fixed-tier-model"));
    assert!(run.resolve_role("not-bound").is_err());
    // Without active bindings the run is the provider itself.
    let disabled =
        SavedSessionSettingsV1::new("fixed".into(), "parent-model".into(), "auto".into()).unwrap();
    let (run, reason) = activation_or_stop(provider.clone(), &disabled, &[]);
    assert!(reason.is_none());
    assert!(Arc::ptr_eq(&run, &provider));
}

#[test]
fn a_stopped_delegation_is_announced_once_per_conversation() {
    let mut notices = StoppedNotices::default();
    let first = Path::new("/sessions/a.jsonl");
    let other = Path::new("/sessions/b.jsonl");
    let reason =
        || Some("role worker is bound to tier giant, which has no provider here".to_owned());
    let notice = notices
        .once(first, reason())
        .expect("the first turn announces it");
    assert!(notice.contains("role worker"), "{notice}");
    assert!(notice.contains("Delegation is off"), "{notice}");
    assert!(
        notices.once(first, reason()).is_none(),
        "later turns stay quiet"
    );
    assert!(
        notices.once(other, reason()).is_some(),
        "another conversation hears it"
    );
    assert!(notices.once(Path::new("/sessions/c.jsonl"), None).is_none());
}
