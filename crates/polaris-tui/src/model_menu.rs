//! Model inventory and selection using the existing provider lifetime.
use super::*;

#[derive(Clone, Copy)]
enum ModelMenuChoice {
    Hosted(&'static str),
    Local,
}

struct ConnectedLocalModel {
    runtime: Runtime,
    adapter: Arc<LocalAdapter>,
    observed: ModelMetadata,
}

impl ConnectedLocalModel {
    fn provider_name(&self) -> &'static str {
        match self.runtime {
            Runtime::Ollama => "ollama",
            Runtime::LmStudio => "lmstudio",
            Runtime::LlamaCpp => "llamacpp",
        }
    }

    fn runtime_label(&self) -> &'static str {
        match self.runtime {
            Runtime::Ollama => "Ollama",
            Runtime::LmStudio => "LM Studio",
            Runtime::LlamaCpp => "llama.cpp",
        }
    }

    fn display_name(&self) -> String {
        format!("{} · {}", self.runtime_label(), self.observed.id())
    }
}

const OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434/";
const LM_STUDIO_ENDPOINT: &str = "http://127.0.0.1:1234/";

/// Where each local runtime is looked for. Held apart so a test can point the
/// same code at a server of its own.
#[cfg(unix)]
pub(super) struct LocalEndpoints<'a> {
    pub(super) ollama: &'a str,
    pub(super) lm_studio: &'a str,
}

#[cfg(unix)]
pub(super) const LOOPBACK_ENDPOINTS: LocalEndpoints<'static> = LocalEndpoints {
    ollama: OLLAMA_ENDPOINT,
    lm_studio: LM_STUDIO_ENDPOINT,
};

#[cfg(unix)]
impl<'a> LocalEndpoints<'a> {
    /// The runtime a saved provider name stands for, and where it is looked for.
    /// Only what the `/model` picker lists: llama.cpp is started by the process.
    pub(super) fn runtime_for(&self, provider: &str) -> Option<(Runtime, &'a str)> {
        match provider {
            "ollama" => Some((Runtime::Ollama, self.ollama)),
            "lmstudio" => Some((Runtime::LmStudio, self.lm_studio)),
            _ => None,
        }
    }
}

/// `None` when the server cannot be reached or does not answer in the expected shape.
async fn discover_local_runtime(
    runtime: Runtime,
    endpoint: &str,
) -> Option<Vec<ConnectedLocalModel>> {
    let endpoint = Endpoint::parse(endpoint).ok()?;
    let adapter = Arc::new(LocalAdapter::new(runtime, endpoint).ok()?);
    let models = adapter.inventory().await.ok()?;
    Some(
        models
            .into_iter()
            .filter(|model| model.capabilities().completion != Capability::Unsupported)
            .map(|observed| ConnectedLocalModel {
                runtime,
                adapter: adapter.clone(),
                observed,
            })
            .collect(),
    )
}

async fn discover_connected_local_models() -> Vec<ConnectedLocalModel> {
    let (ollama, lmstudio) = tokio::join!(
        discover_local_runtime(Runtime::Ollama, OLLAMA_ENDPOINT),
        discover_local_runtime(Runtime::LmStudio, LM_STUDIO_ENDPOINT),
    );
    ollama
        .unwrap_or_default()
        .into_iter()
        .chain(lmstudio.unwrap_or_default())
        .collect()
}

/// The `Local model` row from the chosen model onward: select it, require the
/// tool support polaris needs, and bind it through the shared router.
async fn bind_local_model(
    picked: &ConnectedLocalModel,
    usage_meter: &polaris_provider::UsageMeter,
    local_router: &polaris_core::local_execution::LocalRouter,
) -> Result<Arc<dyn Provider>, String> {
    let selection = picked
        .adapter
        .select(&picked.observed)
        .await
        .map_err(|error| format!("local model is unavailable: {error}"))?;
    if selection.model().capabilities().tools != Capability::Supported {
        return Err("local model does not report the tool support required by polaris".to_string());
    }
    let binding = local_router
        .bind(selection)
        .map_err(|error| format!("local model cannot be used: {error}"))?;
    Ok(Arc::new(usage_meter.wrap(binding)))
}

/// Why a conversation's saved local model could not be bound.
#[cfg(unix)]
pub(super) enum LocalMiss {
    NotReachable,
    NotListed,
    Unusable(String),
}

/// The `Local model` row for a model named in advance, as a conversation that
/// was held with one needs it: the same discovery, selection, tool check and
/// bind, without the picker.
#[cfg(unix)]
pub(super) async fn bind_saved_local_model(
    runtime: Runtime,
    endpoint: &str,
    model: &str,
    usage_meter: &polaris_provider::UsageMeter,
    local_router: &polaris_core::local_execution::LocalRouter,
) -> Result<Arc<dyn Provider>, LocalMiss> {
    let models = discover_local_runtime(runtime, endpoint)
        .await
        .ok_or(LocalMiss::NotReachable)?;
    let picked = models
        .iter()
        .find(|held| held.observed.id() == model)
        .ok_or(LocalMiss::NotListed)?;
    bind_local_model(picked, usage_meter, local_router)
        .await
        .map_err(LocalMiss::Unusable)
}

/// Runs the `/model` picker — three hosted choices plus one local-runtime
/// entry. Reasoning effort is intentionally absent because routing owns it.
fn run_model_picker<B: ratatui::backend::Backend, R: approver::KeyReader>(
    terminal: &RefCell<ratatui::Terminal<B>>,
    reader: &mut R,
    current: &str,
    current_is_local: bool,
) -> Option<ModelMenuChoice> {
    let catalog = render::MODEL_CATALOG;
    let mut selected = if current_is_local {
        catalog.len()
    } else {
        catalog.iter().position(|m| *m == current).unwrap_or(0)
    };
    let count = catalog.len() + 1;
    loop {
        if terminal
            .borrow_mut()
            .draw(|f| render::render_model_picker(f, current, current_is_local, selected))
            .is_err()
        {
            return None;
        }
        use ratatui::crossterm::event::KeyCode;
        match reader.read_key() {
            Ok(KeyCode::Down) => selected = (selected + 1) % count,
            Ok(KeyCode::Up) => selected = (selected + count - 1) % count,
            Ok(KeyCode::Enter) if selected == catalog.len() => {
                return Some(ModelMenuChoice::Local);
            }
            Ok(KeyCode::Enter) => return Some(ModelMenuChoice::Hosted(catalog[selected])),
            Ok(KeyCode::Esc) => return None,
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

fn run_local_model_picker<B: ratatui::backend::Backend, R: approver::KeyReader>(
    terminal: &RefCell<ratatui::Terminal<B>>,
    reader: &mut R,
    models: &[ConnectedLocalModel],
    current_provider: &str,
    current_model: &str,
) -> Option<usize> {
    if models.is_empty() {
        return None;
    }
    let labels: Vec<String> = models
        .iter()
        .map(ConnectedLocalModel::display_name)
        .collect();
    let current = models.iter().position(|model| {
        model.provider_name() == current_provider && model.observed.id() == current_model
    });
    let current_label = current.map(|index| labels[index].as_str());
    let mut selected = current.unwrap_or(0);
    loop {
        if terminal
            .borrow_mut()
            .draw(|f| render::render_local_model_picker(f, &labels, current_label, selected))
            .is_err()
        {
            return None;
        }
        use ratatui::crossterm::event::KeyCode;
        match reader.read_key() {
            Ok(KeyCode::Down) => selected = (selected + 1) % models.len(),
            Ok(KeyCode::Up) => selected = (selected + models.len() - 1) % models.len(),
            Ok(KeyCode::Enter) => return Some(selected),
            Ok(KeyCode::Esc) => return None,
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

/// Handles `/model`. Hosted choices reuse the authenticated provider. The
/// local row discovers only the same two loopback runtimes as the desktop,
/// then installs an immutable local binding for the chosen observed model.
/// Returns whether the person chose a model: an open conversation then runs on
/// that choice, whatever it was saved with.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_model<B: ratatui::backend::Backend, R: approver::KeyReader>(
    terminal: &RefCell<ratatui::Terminal<B>>,
    reader: &mut R,
    provider: &mut Arc<dyn Provider>,
    cloud_provider: &Arc<dyn Provider>,
    usage_meter: &polaris_provider::UsageMeter,
    local_router: &polaris_core::local_execution::LocalRouter,
    provider_name: &mut String,
    cloud_provider_name: &str,
    model_name: &mut String,
    cloud_effort_name: &str,
    effort_name: &mut String,
    status: &mut Status,
) -> bool {
    loop {
        #[cfg(target_os = "linux")]
        if signal::pending_linux_process_shutdown().is_some() {
            return false;
        }
        let current_is_local = is_local_provider(provider_name);
        let Some(choice) = run_model_picker(terminal, reader, model_name, current_is_local) else {
            return false;
        };
        #[cfg(target_os = "linux")]
        if signal::pending_linux_process_shutdown().is_some() {
            return false;
        }
        match choice {
            ModelMenuChoice::Hosted(picked_model) => {
                if !matches!(cloud_provider_name, "codex" | "openai") {
                    *status = Status::Error(
                        "hosted models are unavailable for this launch; restart with a hosted provider"
                            .to_string(),
                    );
                    return false;
                }
                cloud_provider.set_model(picked_model);
                *provider = cloud_provider.clone();
                *provider_name = cloud_provider_name.to_string();
                *model_name = picked_model.to_string();
                *effort_name = cloud_effort_name.to_string();
                *status = Status::Notice(format!("model set to {picked_model}"));
                return true;
            }
            ModelMenuChoice::Local => {
                let models = discover_connected_local_models().await;
                #[cfg(target_os = "linux")]
                if signal::pending_linux_process_shutdown().is_some() {
                    return false;
                }
                let Some(index) =
                    run_local_model_picker(terminal, reader, &models, provider_name, model_name)
                else {
                    if models.is_empty() {
                        *status = Status::Notice("no connected local models".to_string());
                        return false;
                    }
                    continue;
                };
                #[cfg(target_os = "linux")]
                if signal::pending_linux_process_shutdown().is_some() {
                    return false;
                }
                let picked = &models[index];
                let bound = bind_local_model(picked, usage_meter, local_router).await;
                #[cfg(target_os = "linux")]
                if signal::pending_linux_process_shutdown().is_some() {
                    return false;
                }
                *provider = match bound {
                    Ok(bound) => bound,
                    Err(message) => {
                        *status = Status::Error(message);
                        return false;
                    }
                };
                *provider_name = picked.provider_name().to_string();
                *model_name = picked.observed.id().to_string();
                effort_name.clear();
                *status = Status::Notice(format!(
                    "local model set to {} · {}",
                    picked.runtime_label(),
                    picked.observed.id()
                ));
                return true;
            }
        }
    }
}
