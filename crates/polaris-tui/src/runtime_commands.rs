//! The proof a runtime management request carries: the stop record and journalled approvals.
use polaris_core::{
    management_effect::ManagementEffectStoreV1, project_mutation::ProjectActivityLeaseV1,
};

/// The host supplies the bound run activity lease. Only the restricted
/// runtime journal API accepts this proof; source mutation remains excluded.
pub struct RuntimeMutationContext<'a> {
    pub effects: &'a ManagementEffectStoreV1,
    pub lease: &'a ProjectActivityLeaseV1,
    pub session_id: &'a str,
    pub generation: u64,
    pub accepted: bool,
}
