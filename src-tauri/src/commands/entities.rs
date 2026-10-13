//! Entity groups (Phase 4): list, merge, split, dismiss proposals, regroup,
//! documents mentioning an entity, and the explanation of known-entity
//! suggestions. User decisions are audit-logged with ids and counts only.

use tauri::{Emitter, Manager, State};

use crate::entities::resolve::ResolveConfig;
use crate::entities::service::{self, GroupDocument, GroupSummary, ProposalSummary, RedactionNote, RegroupReport};
use crate::AppState;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("task failed: {}", e))
}

/// Regroup with the current settings. With `embed`, new variants are
/// embedded first (starting the sidecar if needed); otherwise only vectors
/// already stored are used. Without an embedding model: rules only.
pub fn run_regroup(state: &AppState, embed: bool) -> Result<RegroupReport, String> {
    #[cfg(desktop)]
    {
        let model = state.qvac.embedding_model();
        let embedder = match (&model, embed && state.qvac.is_bundled()) {
            (Some(m), true) => Some(crate::qvac::QvacEmbedder { service: std::sync::Arc::clone(&state.qvac), model: m.clone() }),
            _ => None,
        };
        service::regroup(
            &state.db,
            embedder.as_ref().map(|e| e as &dyn crate::entities::Embedder),
            model.as_deref(),
            &ResolveConfig::default(),
        )
    }
    #[cfg(not(desktop))]
    {
        let _ = embed;
        service::regroup(&state.db, None, None, &ResolveConfig::rules_only())
    }
}

/// Regroup in the background after a scan; emits `entities-updated`.
pub fn spawn_regroup(app: tauri::AppHandle, trigger: &'static str) {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        match run_regroup(&state, true) {
            Ok(rep) => {
                let _ = state.db.insert_audit_log(None, "entity_regroup", Some(&format!("trigger={} {}", trigger, rep.audit())));
                let _ = app.emit("entities-updated", &rep);
            }
            Err(e) => log::warn!("entity regroup failed: {}", e),
        }
    });
}

#[tauri::command]
pub async fn regroup_entities(embed: Option<bool>, app: tauri::AppHandle) -> Result<RegroupReport, String> {
    let embed = embed.unwrap_or(false);
    let a = app.clone();
    let rep = blocking(move || run_regroup(&a.state::<AppState>(), embed)).await??;
    if embed {
        let st = app.state::<AppState>();
        let _ = st.db.insert_audit_log(None, "entity_regroup", Some(&format!("trigger=user {}", rep.audit())));
    }
    Ok(rep)
}

#[tauri::command]
pub async fn get_entity_groups(state: State<'_, AppState>) -> Result<Vec<GroupSummary>, String> {
    service::list_groups(&state.db)
}

#[tauri::command]
pub async fn get_entity_proposals(state: State<'_, AppState>) -> Result<Vec<ProposalSummary>, String> {
    service::list_proposals(&state.db)
}

#[tauri::command]
pub async fn get_entity_group_documents(group_id: String, state: State<'_, AppState>) -> Result<Vec<GroupDocument>, String> {
    service::group_documents(&state.db, &group_id)
}

#[tauri::command]
pub async fn merge_entity_groups(target: String, others: Vec<String>, app: tauri::AppHandle) -> Result<(), String> {
    let a = app.clone();
    let n = others.len();
    blocking(move || {
        let st = a.state::<AppState>();
        let moved = service::merge_groups(&st.db, &target, &others)?;
        let _ = st.db.insert_audit_log(None, "entity_merge", Some(&format!("groups={} variants_moved={}", n + 1, moved)));
        run_regroup(&st, false).map(|_| ())
    })
    .await?
}

#[tauri::command]
pub async fn split_entity_variant(variant_id: String, app: tauri::AppHandle) -> Result<String, String> {
    let a = app.clone();
    blocking(move || {
        let st = a.state::<AppState>();
        let g = service::split_variant(&st.db, &variant_id)?;
        let _ = st.db.insert_audit_log(None, "entity_split", Some("variants=1"));
        run_regroup(&st, false)?;
        Ok(g)
    })
    .await?
}

#[tauri::command]
pub async fn dismiss_entity_proposal(group_a: String, group_b: String, app: tauri::AppHandle) -> Result<(), String> {
    let a = app.clone();
    blocking(move || {
        let st = a.state::<AppState>();
        service::dismiss_proposal(&st.db, &group_a, &group_b)?;
        let _ = st.db.insert_audit_log(None, "entity_proposal_dismissed", Some("groups=2"));
        run_regroup(&st, false).map(|_| ())
    })
    .await?
}

#[tauri::command]
pub async fn get_redaction_notes(document_id: String, state: State<'_, AppState>) -> Result<Vec<RedactionNote>, String> {
    service::redaction_notes(&state.db, &document_id)
}
