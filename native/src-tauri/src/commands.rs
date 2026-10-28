use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::{
    encapsulation::{session, NetworkPolicy, Registry, WrappedApp},
    error::AegisError,
    security,
};

#[derive(Serialize)]
pub struct CoreStatus { version: &'static str, policy: &'static str }

#[tauri::command]
pub fn core_status() -> CoreStatus {
    CoreStatus { version: env!("CARGO_PKG_VERSION"), policy: "deny-by-default" }
}

#[derive(Serialize)]
pub struct SiteCheck { url: String, host: String, allowed: bool }

#[tauri::command]
pub fn validate_site(url: String) -> Result<SiteCheck, AegisError> {
    let parsed = security::validate_https_url(&url)?;
    Ok(SiteCheck { host: parsed.host_str().unwrap_or_default().into(), url: parsed.to_string(), allowed: true })
}

#[tauri::command]
pub fn list_apps(reg: State<'_, Registry>) -> Vec<WrappedApp> {
    reg.apps.lock().unwrap().clone()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewApp { name: String, url: String, ephemeral: bool, network: Option<NetworkPolicy> }

#[tauri::command]
pub fn add_app(input: NewApp, reg: State<'_, Registry>) -> Result<WrappedApp, AegisError> {
    let parsed = security::validate_https_url(&input.url)?;
    let network = input.network.unwrap_or_default();
    if !network.validate() { return Err(AegisError::InvalidPolicy); }
    let name: String = input.name.trim().chars().take(64).collect();
    let app = WrappedApp {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.is_empty() { parsed.host_str().unwrap_or("App").into() } else { name },
        host: parsed.host_str().unwrap_or_default().into(),
        url: parsed.to_string(),
        ephemeral: input.ephemeral,
        network,
    };
    let mut apps = reg.apps.lock().unwrap();
    apps.push(app.clone());
    reg.save(&apps)?;
    Ok(app)
}

#[tauri::command]
pub fn update_network(id: String, network: NetworkPolicy, handle: AppHandle, reg: State<'_, Registry>) -> Result<(), AegisError> {
    if !network.validate() { return Err(AegisError::InvalidPolicy); }
    {
        let mut apps = reg.apps.lock().unwrap();
        let app = apps.iter_mut().find(|a| a.id == id).ok_or(AegisError::NotFound)?;
        app.network = network;
        reg.save(&apps)?;
    }
    session::close(&handle, &id); // policy applies on next launch
    Ok(())
}

#[tauri::command]
pub fn remove_app(id: String, handle: AppHandle, reg: State<'_, Registry>) -> Result<(), AegisError> {
    session::wipe(&handle, &id)?;
    handle.state::<crate::workspace::Workspace>().forget(&id);
    let mut apps = reg.apps.lock().unwrap();
    apps.retain(|a| a.id != id);
    reg.save(&apps)
}

#[tauri::command]
pub fn launch_app(id: String, handle: AppHandle, reg: State<'_, Registry>) -> Result<(), AegisError> {
    let app = reg.get(&id)?;
    session::launch(&handle, &app)
}

#[tauri::command]
pub fn close_app(id: String, handle: AppHandle) {
    session::close(&handle, &id);
}

#[tauri::command]
pub fn wipe_session(id: String, handle: AppHandle, reg: State<'_, Registry>) -> Result<(), AegisError> {
    reg.get(&id)?;
    session::wipe(&handle, &id)
}

// ---- Intelligence Fabric ----
use crate::fabric::{Fabric, PermissionRequest, ThreatScore};

#[tauri::command]
pub fn score_host(host: String, app_host: String, fabric: State<'_, Fabric>) -> Result<ThreatScore, AegisError> {
    if host.len() > 253 || app_host.len() > 253 { return Err(AegisError::InvalidUrl); }
    Ok(fabric.score(&host, &app_host))
}

#[tauri::command]
pub fn advise_permission(request: PermissionRequest, fabric: State<'_, Fabric>) -> crate::fabric::permissions::Advice {
    fabric.advise(&request)
}

#[tauri::command]
pub fn record_permission(app_id: String, kind: String, allowed: bool, reg: State<'_, Registry>, fabric: State<'_, Fabric>) -> Result<(), AegisError> {
    reg.get(&app_id)?;
    fabric.record_decision(&app_id, &kind, allowed);
    Ok(())
}

#[tauri::command]
pub fn fabric_stats(fabric: State<'_, Fabric>) -> crate::fabric::worker::FabricStats {
    fabric.stats()
}

// ---- Workspace & Governor ----
use crate::{governor::Limits, runtime::{GovernorSnapshot, Runtime}, workspace::{self, Prediction, Workspace}};

#[tauri::command]
pub fn workspace_predictions(ws: State<'_, Workspace>) -> Vec<Prediction> {
    let b = workspace::bucket(workspace::now_secs() + 600, 0);
    ws.predict(b, &[]).into_iter().take(5).collect()
}

#[tauri::command]
pub fn governor_snapshot(rt: State<'_, Runtime>) -> GovernorSnapshot { rt.snapshot() }

#[tauri::command]
pub fn set_app_limits(app_id: String, limits: Option<Limits>, reg: State<'_, Registry>, rt: State<'_, Runtime>) -> Result<(), AegisError> {
    reg.get(&app_id)?;
    if let Some(l) = limits {
        if !(64.0..=4096.0).contains(&l.ram_mb) || !(1.0..=100.0).contains(&l.cpu_pct) { return Err(AegisError::InvalidPolicy); }
    }
    rt.set_limits(&app_id, limits); // None = back to automatic
    Ok(())
}

#[tauri::command]
pub fn set_governor(enabled: bool, battery_saver: bool, rt: State<'_, Runtime>) {
    let mut s = rt.0.lock().unwrap();
    s.enabled = enabled;
    s.battery_saver = battery_saver;
}
