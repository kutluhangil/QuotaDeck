//! Shared state between the read loop, the tray and the panel.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use chrono::{DateTime, Utc};
use quotadeck_core::atomic_write::atomic_write;
use quotadeck_core::breakdown::BreakdownPoint;
use quotadeck_core::error::{Error, Result};
use quotadeck_core::history::HistoryPoint;
use quotadeck_core::paths;
use quotadeck_core::provider::ProviderConfig;
use quotadeck_core::types::{ProviderId, ProviderSnapshot, QuotaWindow, UnavailableReason};
use serde::{Deserialize, Deserializer, Serialize};

use crate::i18n::Locale;
use crate::sandbox::{self, AccessState, ScopedAccess};

/// What the panel renders. Raw log lines never appear here — only folded snapshots.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckState {
    pub providers: Vec<ProviderSnapshot>,
    pub health: Vec<ProviderHealth>,
    pub updated_at: DateTime<Utc>,
    /// True until the first pass over every file has finished.
    pub scanning: bool,
    pub refreshing: bool,
    pub refresh_generation: u64,
    pub refresh_error: Option<String>,
    pub retention: RetentionState,
}

impl DeckState {
    pub(crate) fn empty() -> Self {
        DeckState {
            providers: Vec::new(),
            health: Vec::new(),
            updated_at: DateTime::UNIX_EPOCH,
            scanning: true,
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: RetentionState::default(),
        }
    }

    /// The single number the menu bar shows: the fullest window across every provider.
    pub fn peak_percent(&self) -> Option<f32> {
        self.headline().and_then(|(_, window)| window.used_percent)
    }

    /// The provider and window the menu bar is speaking for.
    ///
    /// The strip mode has to draw one provider's history against one window's duration, and
    /// the only defensible choice is the quota closest to running out — the same reading the
    /// glyph and the percentage already show.
    pub fn headline(&self) -> Option<(&ProviderSnapshot, &QuotaWindow)> {
        self.providers
            .iter()
            .flat_map(|snapshot| {
                snapshot
                    .windows
                    .iter()
                    .filter(|window| window.used_percent.is_some())
                    .map(move |window| (snapshot, window))
            })
            .fold(
                None::<(&ProviderSnapshot, &QuotaWindow)>,
                |peak, candidate| match peak {
                    Some((_, best)) if best.used_percent >= candidate.1.used_percent => peak,
                    _ => Some(candidate),
                },
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionState {
    pub requested_days: u16,
    pub effective_days: u16,
    pub rebuilding: bool,
    pub error: Option<String>,
}

impl Default for RetentionState {
    fn default() -> Self {
        RetentionState {
            requested_days: 32,
            effective_days: 32,
            rebuilding: false,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthState {
    Healthy,
    Rebuilding,
    Stale,
    Error,
    Disabled,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub provider: ProviderId,
    pub state: HealthState,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub next_retry_at: Option<DateTime<Utc>>,
}

impl ProviderHealth {
    pub fn new(provider: ProviderId) -> Self {
        ProviderHealth {
            provider,
            state: HealthState::Unavailable,
            last_attempt_at: None,
            last_success_at: None,
            consecutive_failures: 0,
            last_error: None,
            next_retry_at: None,
        }
    }

    pub fn retry_due(&self, now: DateTime<Utc>, manual: bool) -> bool {
        manual || self.next_retry_at.is_none_or(|retry| retry <= now)
    }

    pub fn record_success(&mut self, at: DateTime<Utc>) {
        self.state = HealthState::Healthy;
        self.last_attempt_at = Some(at);
        self.last_success_at = Some(at);
        self.consecutive_failures = 0;
        self.last_error = None;
        self.next_retry_at = None;
    }

    pub fn record_rebuilding(&mut self, at: DateTime<Utc>) {
        self.state = HealthState::Rebuilding;
        self.last_attempt_at = Some(at);
        self.last_success_at = None;
        self.consecutive_failures = 0;
        self.last_error = None;
        self.next_retry_at = None;
    }

    pub fn record_failure(&mut self, at: DateTime<Utc>, error: String, had_success: bool) {
        self.state = if had_success {
            HealthState::Stale
        } else {
            HealthState::Error
        };
        self.last_attempt_at = Some(at);
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_error = Some(error);
        let shift = self.consecutive_failures.saturating_sub(1).min(6);
        let seconds = (5_i64 << shift).min(300);
        self.next_retry_at = Some(at + chrono::Duration::seconds(seconds));
    }

    pub fn record_disabled(&mut self) {
        self.state = HealthState::Disabled;
        self.consecutive_failures = 0;
        self.last_error = None;
        self.next_retry_at = None;
    }

    pub fn record_unavailable(&mut self, at: DateTime<Utc>, error: String) {
        self.state = HealthState::Unavailable;
        self.last_attempt_at = Some(at);
        self.consecutive_failures = 0;
        self.last_error = Some(error);
        self.next_retry_at = None;
    }
}

pub(crate) fn provider_snapshot_after_failure(
    previous: Option<&ProviderSnapshot>,
    provider: ProviderId,
) -> ProviderSnapshot {
    previous
        .cloned()
        .unwrap_or_else(|| ProviderSnapshot::unavailable(provider, UnavailableReason::ReadError))
}

/// One provider's retained usage, folded to the hour.
///
/// Pulled by the dashboard rather than pushed with every tick: the panel never renders it, and
/// a month of history on every refresh would put the cost of a surface nobody has open through
/// the channel the panel depends on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHistory {
    pub id: ProviderId,
    /// Hours carrying usage, oldest first. Empty hours are omitted; the dashboard lays out
    /// the calendar itself, in the viewer's own zone.
    pub hours: Vec<HistoryPoint>,
    /// The same hours split by the model that produced the usage. A point whose `label` is
    /// `null` is usage the provider reported no model for — Codex names none in any record —
    /// and the dashboard says so rather than dropping it.
    pub models: Vec<BreakdownPoint>,
    /// Records refused because the machine reported more distinct models than the breakdown
    /// holds. Surfaced rather than folded into an "other" row, which would under-report a real
    /// model without saying so.
    pub models_dropped: u64,
    /// The same hours split by the directory the work was done in, as the tool recorded it.
    /// A point whose `label` is `null` is usage no tool named a directory for.
    pub projects: Vec<BreakdownPoint>,
    /// Records refused for carrying more distinct directories than the breakdown holds.
    pub projects_dropped: u64,
    /// The same hours split by which thread of work produced them — the main conversation, a
    /// subagent, or an agent inside a workflow run. Three labels at most.
    pub agents: Vec<BreakdownPoint>,
    /// Kept for symmetry with the other two dimensions; three fixed labels cannot overflow.
    pub agents_dropped: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrayMode {
    /// A bar. No numbers, no colour until the quota is genuinely at risk.
    Glyph,
    /// The highest reported usage, as a percentage.
    Compact,
    /// A miniature of the panel's timeline.
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub enum RetentionDays {
    #[default]
    Days32,
    Days90,
    Days365,
}

impl RetentionDays {
    pub const fn days(self) -> i64 {
        match self {
            RetentionDays::Days32 => 32,
            RetentionDays::Days90 => 90,
            RetentionDays::Days365 => 365,
        }
    }

    pub fn duration(self) -> chrono::Duration {
        chrono::Duration::days(self.days())
    }
}

impl TryFrom<u16> for RetentionDays {
    type Error = String;

    fn try_from(value: u16) -> std::result::Result<Self, Self::Error> {
        match value {
            32 => Ok(RetentionDays::Days32),
            90 => Ok(RetentionDays::Days90),
            365 => Ok(RetentionDays::Days365),
            _ => Err(format!(
                "settings.retentionDays must be one of 32, 90, or 365; received {value}"
            )),
        }
    }
}

impl From<RetentionDays> for u16 {
    fn from(value: RetentionDays) -> Self {
        value.days() as u16
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub tray_mode: TrayMode,
    pub theme: Theme,
    /// Which catalogue the copy comes from, here and in the panel.
    ///
    /// Stored rather than left to the frontend because the notifications and the tray menu are
    /// written from this process, and the panel is usually closed when they are.
    pub locale: Locale,
    /// Chosen subscription tier per provider, keyed by [`ProviderId::key`].
    ///
    /// A provider missing from this map has no tier picked, which means no estimated window is
    /// produced for it at all. There is deliberately no default tier: an unpicked plan must
    /// read as "tell me your plan", never as a percentage the user never asked for.
    pub plans: BTreeMap<String, String>,
    /// Usage percentages that raise a notification, per provider key.
    ///
    /// Unlike `plans`, an absent provider takes [`DEFAULT_THRESHOLDS`] rather than nothing: a
    /// quota tracker that never warns you is not doing its job, and the operating system asks
    /// for its own consent before the first notification is ever shown. An empty list is how
    /// the user turns one provider off.
    pub alerts: BTreeMap<String, Vec<u8>>,
    /// Nothing is raised before this instant. Set from the panel, in the user's own zone.
    pub muted_until: Option<DateTime<Utc>>,
    /// Show the sample deck instead of this machine's readings.
    ///
    /// Required by the store listing: someone has to be able to see what the app looks like
    /// before they buy it, and on a machine with no supported tool the real answer is an empty
    /// panel. Off by default — a sample shown without being asked for is a lie.
    pub demo: bool,
    /// Provider keys that stay out of every backend pass until explicitly re-enabled.
    pub disabled_providers: BTreeSet<String>,
    /// Provider keys in the user's preferred presentation and processing order.
    pub provider_order: Vec<String>,
    pub retention_days: RetentionDays,
}

/// Thresholds a provider raises at unless the user changed them (blueprint §9, Phase 7).
pub const DEFAULT_THRESHOLDS: [u8; 3] = [70, 85, 95];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            tray_mode: TrayMode::Glyph,
            theme: Theme::System,
            locale: Locale::System,
            plans: BTreeMap::new(),
            alerts: BTreeMap::new(),
            muted_until: None,
            demo: false,
            disabled_providers: BTreeSet::new(),
            provider_order: quotadeck_providers::ids()
                .into_iter()
                .map(|id| id.key().to_string())
                .collect(),
            retention_days: RetentionDays::Days32,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SettingsDocument {
    tray_mode: TrayMode,
    theme: Theme,
    locale: Locale,
    plans: BTreeMap<String, String>,
    alerts: BTreeMap<String, Vec<u8>>,
    muted_until: Option<DateTime<Utc>>,
    demo: bool,
    disabled_providers: BTreeSet<String>,
    provider_order: Vec<String>,
    retention_days: RetentionDays,
}

impl Default for SettingsDocument {
    fn default() -> Self {
        let settings = Settings::default();
        SettingsDocument {
            tray_mode: settings.tray_mode,
            theme: settings.theme,
            locale: settings.locale,
            plans: settings.plans,
            alerts: settings.alerts,
            muted_until: settings.muted_until,
            demo: settings.demo,
            disabled_providers: settings.disabled_providers,
            provider_order: settings.provider_order,
            retention_days: settings.retention_days,
        }
    }
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error as _;

        let document = SettingsDocument::deserialize(deserializer)?;
        let mut settings = Settings {
            tray_mode: document.tray_mode,
            theme: document.theme,
            locale: document.locale,
            plans: document.plans,
            alerts: document.alerts,
            muted_until: document.muted_until,
            demo: document.demo,
            disabled_providers: document.disabled_providers,
            provider_order: document.provider_order,
            retention_days: document.retention_days,
        };
        let ordered = settings
            .ordered_provider_ids(&quotadeck_providers::ids())
            .map_err(D::Error::custom)?;
        settings.provider_order = ordered.into_iter().map(|id| id.key().to_string()).collect();
        Ok(settings)
    }
}

#[derive(Debug)]
pub(crate) struct RetentionChangeRequest {
    pub retention: RetentionDays,
    pub complete: mpsc::Sender<Result<()>>,
}

impl Settings {
    pub fn is_provider_enabled(&self, id: ProviderId) -> bool {
        !self.disabled_providers.contains(id.key())
    }

    /// Resolve the saved policy against this build's registry.
    ///
    /// A saved partial order is valid: providers compiled by a later release are appended in
    /// registry order. Unknown and duplicate keys are rejected with the persisted JSON path.
    pub fn ordered_provider_ids(&self, registry: &[ProviderId]) -> Result<Vec<ProviderId>> {
        let known: BTreeMap<&str, ProviderId> = registry.iter().map(|id| (id.key(), *id)).collect();

        for key in &self.disabled_providers {
            if !known.contains_key(key.as_str()) {
                return Err(Error::Invalid(format!(
                    "settings.disabledProviders contains unknown provider key {key:?}"
                )));
            }
        }

        let mut seen = BTreeSet::new();
        let mut ordered = Vec::with_capacity(registry.len());
        for key in &self.provider_order {
            let Some(id) = known.get(key.as_str()).copied() else {
                return Err(Error::Invalid(format!(
                    "settings.providerOrder contains unknown provider key {key:?}"
                )));
            };
            if !seen.insert(key.as_str()) {
                return Err(Error::Invalid(format!(
                    "settings.providerOrder contains duplicate provider key {key:?}"
                )));
            }
            ordered.push(id);
        }
        for id in registry {
            if seen.insert(id.key()) {
                ordered.push(*id);
            }
        }
        Ok(ordered)
    }

    pub fn config_for(&self, id: ProviderId) -> ProviderConfig {
        ProviderConfig {
            plan_id: self.plans.get(id.key()).cloned(),
        }
    }

    /// Percentages this provider raises a notification at.
    pub fn thresholds_for(&self, id: ProviderId) -> Vec<u8> {
        self.alerts
            .get(id.key())
            .cloned()
            .unwrap_or_else(|| DEFAULT_THRESHOLDS.to_vec())
    }

    pub fn is_muted(&self, now: DateTime<Utc>) -> bool {
        self.muted_until.is_some_and(|until| until > now)
    }

    fn path() -> Option<PathBuf> {
        paths::data_dir().map(|dir| dir.join("settings.json"))
    }

    /// Load the stored settings. A first launch has no file and starts from defaults; a file
    /// that exists but cannot be read or parsed is an actionable startup error rather than a
    /// silent preference reset.
    pub fn load() -> Result<Self> {
        let path = Self::path()
            .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))?;
        Self::load_from(path)
    }

    pub fn load_from(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|error| {
                Error::Invalid(format!(
                    "invalid settings JSON in {}: {error}",
                    path.display()
                ))
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(error) => Err(Error::io(path, error)),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()
            .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))?;
        self.save_to(path)
    }

    /// Persist settings at an explicit path.
    ///
    /// The explicit path is also the test seam: tests never mutate process-wide home-directory
    /// variables, so they remain safe when the workspace test runner executes in parallel.
    pub fn save_to(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        self.ordered_provider_ids(&quotadeck_providers::ids())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        atomic_write(path, text.as_bytes())
    }
}

/// The user's grant over the home directory, and what went wrong if there is none.
///
/// One lock over both: the error only ever describes the grant beside it, and splitting them
/// makes it possible to report a stale reason against a fresh grant.
#[derive(Default)]
struct Access {
    held: Option<ScopedAccess>,
    error: Option<String>,
}

pub(crate) struct ProviderPolicySyncRequest {
    pub revision: u64,
    pub complete: mpsc::Sender<Result<()>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPolicyOutcome {
    pub settings: Settings,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshReceipt {
    pub request_id: u64,
}

#[cfg(test)]
pub(crate) fn refresh_request_completed(completed_generation: u64, request_id: u64) -> bool {
    completed_generation >= request_id
}

fn publish_refresh_generation(
    requested: &AtomicU64,
    request_id: u64,
    wake: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let previous = requested.swap(request_id, Ordering::Release);
    if let Err(error) = wake() {
        // The caller serializes request publication with the refresh-control lock. A failed
        // send means there is no receiver that could have consumed this wake, so restoring the
        // last successfully queued generation cannot race a live read loop or a newer request.
        requested.store(previous, Ordering::Release);
        return Err(error);
    }
    Ok(())
}

/// Handle shared by the read loop, the tray and the command handlers.
#[derive(Clone)]
pub struct Deck {
    state: Arc<Mutex<DeckState>>,
    history: Arc<Mutex<Vec<ProviderHistory>>>,
    view_commit: Arc<Mutex<()>>,
    settings: Arc<Mutex<Settings>>,
    access: Arc<Mutex<Access>>,
    panel_open: Arc<AtomicBool>,
    /// True while a modal is on screen. The popover dismisses itself on blur, and an open
    /// panel steals focus — without this, asking for the folder closes the window that asked.
    modal_open: Arc<AtomicBool>,
    provider_policy_revision: Arc<AtomicU64>,
    provider_policy_commit: Arc<Mutex<()>>,
    provider_policy_sync: Arc<Mutex<Option<mpsc::Sender<ProviderPolicySyncRequest>>>>,
    refresh_control: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    next_refresh_request: Arc<AtomicU64>,
    requested_refresh_generation: Arc<AtomicU64>,
    retention_commit: Arc<Mutex<()>>,
    retention_control: Arc<Mutex<Option<mpsc::Sender<RetentionChangeRequest>>>>,
}

impl Deck {
    pub fn new() -> Result<Self> {
        Ok(Deck {
            state: Arc::new(Mutex::new(DeckState::empty())),
            history: Arc::new(Mutex::new(Vec::new())),
            view_commit: Arc::new(Mutex::new(())),
            settings: Arc::new(Mutex::new(Settings::load()?)),
            access: Arc::new(Mutex::new(Access::default())),
            panel_open: Arc::new(AtomicBool::new(false)),
            modal_open: Arc::new(AtomicBool::new(false)),
            provider_policy_revision: Arc::new(AtomicU64::new(0)),
            provider_policy_commit: Arc::new(Mutex::new(())),
            provider_policy_sync: Arc::new(Mutex::new(None)),
            refresh_control: Arc::new(Mutex::new(None)),
            next_refresh_request: Arc::new(AtomicU64::new(0)),
            requested_refresh_generation: Arc::new(AtomicU64::new(0)),
            retention_commit: Arc::new(Mutex::new(())),
            retention_control: Arc::new(Mutex::new(None)),
        })
    }

    pub(crate) fn register_refresh_control(&self, control: mpsc::Sender<()>) {
        match self.refresh_control.lock() {
            Ok(mut slot) => *slot = Some(control),
            Err(poisoned) => *poisoned.into_inner() = Some(control),
        }
    }

    pub fn queue_refresh(&self) -> Result<RefreshReceipt> {
        let control = match self.refresh_control.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        let control = control
            .as_ref()
            .ok_or_else(|| Error::Invalid("read loop refresh control is not registered".into()))?;
        let request_id = self.next_refresh_request.fetch_add(1, Ordering::AcqRel) + 1;
        publish_refresh_generation(&self.requested_refresh_generation, request_id, || {
            control.send(()).map_err(|error| {
                Error::Invalid(format!(
                    "read loop refresh request {request_id} could not be queued: {error}"
                ))
            })
        })?;
        Ok(RefreshReceipt { request_id })
    }

    pub(crate) fn requested_refresh_generation(&self) -> u64 {
        self.requested_refresh_generation.load(Ordering::Acquire)
    }

    pub(crate) fn register_retention_control(&self, control: mpsc::Sender<RetentionChangeRequest>) {
        match self.retention_control.lock() {
            Ok(mut slot) => *slot = Some(control),
            Err(poisoned) => *poisoned.into_inner() = Some(control),
        }
    }

    /// Take up the grant made on an earlier launch. Called once, before the first read pass.
    ///
    /// A missing grant is the state a new install is in and says nothing; a grant that will
    /// not resolve is reported, because the folder moving is something the user can fix.
    pub fn restore_access(&self) {
        let restored = sandbox::restore();
        self.with_access(|access| match restored {
            Ok(held) => {
                access.held = held;
                access.error = None;
            }
            Err(e) => {
                access.held = None;
                access.error = Some(e.to_string());
            }
        });
    }

    /// Replace the grant. The previous one is released as it drops.
    pub fn set_access(&self, held: Option<ScopedAccess>, error: Option<String>) {
        self.with_access(|access| {
            access.held = held;
            access.error = error;
        });
    }

    pub fn access_state(&self) -> AccessState {
        self.with_access(|access| sandbox::state(access.held.as_ref(), access.error.clone()))
    }

    /// Whether the read loop can see anything at all. False means every provider would report
    /// a permission problem, and the panel should be asking for the folder instead.
    pub fn has_access(&self) -> bool {
        self.access_state().granted
    }

    fn with_access<T>(&self, apply: impl FnOnce(&mut Access) -> T) -> T {
        match self.access.lock() {
            Ok(mut guard) => apply(&mut guard),
            Err(poisoned) => apply(&mut poisoned.into_inner()),
        }
    }

    pub fn history(&self) -> Vec<ProviderHistory> {
        match self.history.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn set_history(&self, next: Vec<ProviderHistory>) {
        let _view = match self.view_commit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        self.set_history_raw(next);
    }

    fn set_history_raw(&self, next: Vec<ProviderHistory>) {
        match self.history.lock() {
            Ok(mut guard) => *guard = next,
            Err(poisoned) => *poisoned.into_inner() = next,
        }
    }

    /// A poisoned lock means another thread panicked while holding it. The data behind it
    /// is a snapshot that is rebuilt every tick, so recovering is correct here.
    pub fn state(&self) -> DeckState {
        match self.state.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn set_state(&self, next: DeckState) {
        let _view = match self.view_commit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        self.set_state_raw(next);
    }

    fn set_state_raw(&self, next: DeckState) {
        match self.state.lock() {
            Ok(mut guard) => *guard = next,
            Err(poisoned) => *poisoned.into_inner() = next,
        }
    }

    pub fn set_published_view(&self, history: Vec<ProviderHistory>, state: DeckState) {
        let _view = match self.view_commit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        self.set_history_raw(history);
        self.set_state_raw(state);
    }

    pub fn export_snapshot(&self) -> (DeckState, Vec<ProviderHistory>) {
        let _view = match self.view_commit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        (self.state(), self.history())
    }

    pub fn settings(&self) -> Settings {
        match self.settings.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn set_tray_mode(&self, mode: TrayMode) -> Result<Settings> {
        self.update_settings(|settings| settings.tray_mode = mode)
    }

    pub fn set_theme(&self, theme: Theme) -> Result<Settings> {
        self.update_settings(|settings| settings.theme = theme)
    }

    pub fn set_locale(&self, locale: Locale) -> Result<Settings> {
        self.update_settings(|settings| settings.locale = locale)
    }

    pub fn set_demo(&self, demo: bool) -> Result<Settings> {
        self.update_settings(|settings| settings.demo = demo)
    }

    pub fn set_retention_days(&self, retention: RetentionDays) -> Result<Settings> {
        self.set_retention_days_with(retention, Settings::save)
    }

    fn set_retention_days_with(
        &self,
        retention: RetentionDays,
        mut save: impl FnMut(&Settings) -> Result<()>,
    ) -> Result<Settings> {
        let _commit = match self.retention_commit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let current_retention = self.state().retention;
        if current_retention.rebuilding {
            return Err(Error::Invalid(format!(
                "retention is rebuilding from {} to {} days; wait for it to finish before choosing another retention",
                current_retention.effective_days, current_retention.requested_days
            )));
        }
        let previous = self.settings();
        if previous.retention_days == retention {
            return Ok(previous);
        }
        let mut next = previous.clone();
        next.retention_days = retention;
        save(&next)?;
        match self.settings.lock() {
            Ok(mut guard) => *guard = next.clone(),
            Err(poisoned) => *poisoned.into_inner() = next.clone(),
        }

        let control = match self.retention_control.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let (complete, completed) = mpsc::channel();
        let send_result = control
            .ok_or_else(|| Error::Invalid("read loop retention control is not registered".into()))
            .and_then(|control| {
                control.send(RetentionChangeRequest { retention, complete }).map_err(|error| {
                    Error::Invalid(format!(
                        "retention change to {} days could not be delivered to the read loop: {error}",
                        retention.days()
                    ))
                })
            })
            .and_then(|()| {
                completed
                    .recv()
                    .map_err(|error| {
                        Error::Invalid(format!(
                            "retention change to {} days was delivered, but the read loop acknowledgement was lost: {error}",
                            retention.days()
                        ))
                    })?
            });
        if let Err(send_error) = send_result {
            let rollback_result = save(&previous);
            match self.settings.lock() {
                Ok(mut guard) => *guard = previous,
                Err(poisoned) => *poisoned.into_inner() = previous,
            }
            return match rollback_result {
                Ok(()) => Err(send_error),
                Err(rollback_error) => Err(Error::Invalid(format!(
                    "{send_error}; settings rollback also failed: {rollback_error}"
                ))),
            };
        }
        Ok(next)
    }

    pub fn set_provider_policy(
        &self,
        disabled_providers: BTreeSet<String>,
        provider_order: Vec<String>,
    ) -> Result<ProviderPolicyOutcome> {
        self.set_provider_policy_and_sync_with(disabled_providers, provider_order, Settings::save)
    }

    fn set_provider_policy_and_sync_with(
        &self,
        disabled_providers: BTreeSet<String>,
        provider_order: Vec<String>,
        save: impl FnOnce(&Settings) -> Result<()>,
    ) -> Result<ProviderPolicyOutcome> {
        let revision = {
            let _commit = match self.provider_policy_commit.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let next = self.set_provider_policy_with(disabled_providers, provider_order, save)?;
            self.apply_provider_policy_to_cached_data(&next)?;
            self.provider_policy_revision
                .fetch_add(1, Ordering::Release)
                + 1
        };

        let warning = self
            .wait_for_provider_policy_sync(revision)
            .err()
            .map(|error| {
                let warning = error.to_string();
                eprintln!("quotadeck: {warning}");
                warning
            });
        Ok(ProviderPolicyOutcome {
            settings: self.settings(),
            warning,
        })
    }

    fn wait_for_provider_policy_sync(&self, revision: u64) -> Result<()> {
        let sync = match self.provider_policy_sync.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let Some(sync) = sync else {
            return Ok(());
        };

        let (complete, completed) = mpsc::channel();
        sync.send(ProviderPolicySyncRequest { revision, complete })
            .map_err(|error| {
                Error::Invalid(format!(
                    "provider policy revision {revision} was saved, but the filesystem watcher sync request could not be delivered: {error}"
                ))
            })?;
        completed.recv().map_err(|error| {
            Error::Invalid(format!(
                "provider policy revision {revision} was saved, but the filesystem watcher sync did not complete: {error}"
            ))
        })?
    }

    pub(crate) fn register_provider_policy_sync(
        &self,
        sync: mpsc::Sender<ProviderPolicySyncRequest>,
    ) {
        match self.provider_policy_sync.lock() {
            Ok(mut guard) => *guard = Some(sync),
            Err(poisoned) => *poisoned.into_inner() = Some(sync),
        }
    }

    fn apply_provider_policy_to_cached_data(&self, settings: &Settings) -> Result<()> {
        let order = settings.ordered_provider_ids(&quotadeck_providers::ids())?;
        let positions: BTreeMap<ProviderId, usize> = order
            .into_iter()
            .enumerate()
            .map(|(position, id)| (id, position))
            .collect();

        let mut state = self.state();
        state
            .providers
            .retain(|snapshot| settings.is_provider_enabled(snapshot.id));
        state
            .providers
            .sort_by_key(|snapshot| positions.get(&snapshot.id).copied().unwrap_or(usize::MAX));
        let mut history = self.history();

        history.retain(|entry| settings.is_provider_enabled(entry.id));
        history.sort_by_key(|entry| positions.get(&entry.id).copied().unwrap_or(usize::MAX));
        self.set_published_view(history, state);
        Ok(())
    }

    fn set_provider_policy_with(
        &self,
        disabled_providers: BTreeSet<String>,
        provider_order: Vec<String>,
        save: impl FnOnce(&Settings) -> Result<()>,
    ) -> Result<Settings> {
        let registry = quotadeck_providers::ids();
        self.update_settings_result_with(
            move |settings| {
                settings.disabled_providers = disabled_providers;
                settings.provider_order = provider_order;
                let ordered = settings.ordered_provider_ids(&registry)?;
                settings.provider_order =
                    ordered.into_iter().map(|id| id.key().to_string()).collect();
                Ok(())
            },
            save,
        )
    }

    /// Record the tier the user picked for one provider, or clear it.
    pub fn set_plan(&self, provider: ProviderId, plan_id: Option<String>) -> Result<Settings> {
        self.update_settings(|settings| match plan_id {
            Some(id) => {
                settings.plans.insert(provider.key().to_string(), id);
            }
            None => {
                settings.plans.remove(provider.key());
            }
        })
    }

    /// Record which percentages one provider warns at. An empty list turns it off.
    pub fn set_alert_thresholds(
        &self,
        provider: ProviderId,
        thresholds: Vec<u8>,
    ) -> Result<Settings> {
        self.update_settings(|settings| {
            settings
                .alerts
                .insert(provider.key().to_string(), thresholds);
        })
    }

    /// Silence every notification until `until`, or lift the silence with `None`.
    pub fn set_muted_until(&self, until: Option<DateTime<Utc>>) -> Result<Settings> {
        self.update_settings(|settings| settings.muted_until = until)
    }

    /// Persist a complete new snapshot before making it authoritative in memory.
    fn update_settings(&self, apply: impl FnOnce(&mut Settings)) -> Result<Settings> {
        self.update_settings_with(apply, Settings::save)
    }

    fn update_settings_with(
        &self,
        apply: impl FnOnce(&mut Settings),
        save: impl FnOnce(&Settings) -> Result<()>,
    ) -> Result<Settings> {
        let mut guard = match self.settings.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut next = guard.clone();
        apply(&mut next);
        save(&next)?;
        *guard = next.clone();
        Ok(next)
    }

    fn update_settings_result_with(
        &self,
        apply: impl FnOnce(&mut Settings) -> Result<()>,
        save: impl FnOnce(&Settings) -> Result<()>,
    ) -> Result<Settings> {
        let mut guard = match self.settings.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut next = guard.clone();
        apply(&mut next)?;
        save(&next)?;
        *guard = next.clone();
        Ok(next)
    }

    pub fn panel_open(&self) -> bool {
        self.panel_open.load(Ordering::Relaxed)
    }

    pub fn set_panel_open(&self, open: bool) {
        self.panel_open.store(open, Ordering::Relaxed);
    }

    pub fn modal_open(&self) -> bool {
        self.modal_open.load(Ordering::Relaxed)
    }

    pub fn provider_policy_revision(&self) -> u64 {
        self.provider_policy_revision.load(Ordering::Acquire)
    }

    /// A settings snapshot and revision taken under the same commit gate.
    pub fn provider_policy_snapshot(&self) -> (Settings, u64) {
        let _commit = match self.provider_policy_commit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        (self.settings(), self.provider_policy_revision())
    }

    /// Run a complete consumer-side policy update while policy setters are excluded.
    pub fn with_provider_policy<T>(
        &self,
        apply: impl FnOnce(&Settings, u64) -> Result<T>,
    ) -> Result<T> {
        let _policy = match self.provider_policy_commit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let settings = self.settings();
        apply(&settings, self.provider_policy_revision())
    }

    /// Commit pass results only while the provider policy that produced them is still current.
    pub fn with_current_provider_policy<T>(
        &self,
        started_revision: u64,
        commit: impl FnOnce() -> Result<T>,
    ) -> Result<Option<T>> {
        let _policy = match self.provider_policy_commit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if self.provider_policy_revision() != started_revision {
            return Ok(None);
        }
        commit().map(Some)
    }

    pub fn set_modal_open(&self, open: bool) {
        self.modal_open.store(open, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quotadeck_core::types::{
        Confidence, ProviderId, QuotaWindow, TokenRollup, UnavailableReason, WindowKind,
    };

    fn scratch(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "quotadeck-deck-{}-{unique}-{name}",
            std::process::id()
        ))
    }

    fn snapshot(percents: &[f32]) -> ProviderSnapshot {
        ProviderSnapshot {
            id: ProviderId::Codex,
            installed: true,
            windows: percents
                .iter()
                .enumerate()
                .map(|(i, percent)| QuotaWindow {
                    limit_id: "codex".into(),
                    kind: WindowKind::Weekly,
                    window_minutes: 10_080 + i as u32,
                    used_percent: Some(*percent),
                    resets_at: None,
                    confidence: Confidence::Measured {
                        reported_at: Utc::now(),
                    },
                })
                .collect(),
            today: TokenRollup::default(),
            today_cost: Default::default(),
            series: Vec::new(),
            pace: Vec::new(),
            last_activity: None,
            unavailable: None,
            read_error: None,
            burst: None,
        }
    }

    #[test]
    fn the_tray_reports_the_fullest_window_anywhere() {
        let state = DeckState {
            providers: vec![snapshot(&[12.0, 80.0]), snapshot(&[95.0])],
            updated_at: Utc::now(),
            scanning: false,
            health: Vec::new(),
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: Default::default(),
        };
        assert_eq!(state.peak_percent(), Some(95.0));
    }

    #[test]
    fn the_strip_draws_the_provider_holding_the_worst_reading() {
        let mut busy = snapshot(&[95.0]);
        busy.id = ProviderId::ClaudeCode;
        let state = DeckState {
            providers: vec![snapshot(&[12.0, 80.0]), busy],
            updated_at: Utc::now(),
            scanning: false,
            health: Vec::new(),
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: Default::default(),
        };

        let (provider, window) = state.headline().expect("a headline reading");
        assert_eq!(provider.id, ProviderId::ClaudeCode);
        assert_eq!(window.used_percent, Some(95.0));
    }

    #[test]
    fn a_window_with_no_reading_never_becomes_the_headline() {
        let mut unreported = snapshot(&[]);
        unreported.windows.push(QuotaWindow {
            limit_id: "codex".into(),
            kind: WindowKind::Weekly,
            window_minutes: 10_080,
            used_percent: None,
            resets_at: None,
            confidence: Confidence::Measured {
                reported_at: Utc::now(),
            },
        });
        let state = DeckState {
            providers: vec![unreported, snapshot(&[3.0])],
            updated_at: Utc::now(),
            scanning: false,
            health: Vec::new(),
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: Default::default(),
        };

        assert_eq!(state.peak_percent(), Some(3.0));
    }

    #[test]
    fn a_deck_with_no_reading_reports_nothing_rather_than_zero() {
        let state = DeckState {
            providers: vec![snapshot(&[])],
            updated_at: Utc::now(),
            scanning: false,
            health: Vec::new(),
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: Default::default(),
        };
        assert_eq!(state.peak_percent(), None);
    }

    #[test]
    fn settings_start_on_the_quiet_tray_mode_and_with_no_plan_picked() {
        let settings = Settings::default();
        assert_eq!(settings.tray_mode, TrayMode::Glyph);
        assert_eq!(settings.theme, Theme::System);
        assert!(
            settings.plans.is_empty(),
            "a default tier would put an unrequested percentage in front of the user"
        );
        assert!(settings
            .config_for(ProviderId::ClaudeCode)
            .plan_id
            .is_none());
        assert!(settings.disabled_providers.is_empty());
        assert_eq!(
            settings.provider_order,
            vec!["claude-code", "codex", "copilot-cli"]
        );
        assert_eq!(settings.retention_days, RetentionDays::Days32);
    }

    #[test]
    fn retention_days_use_numeric_serde_and_reject_every_unsupported_value() {
        for (days, expected) in [
            (32, RetentionDays::Days32),
            (90, RetentionDays::Days90),
            (365, RetentionDays::Days365),
        ] {
            let decoded: RetentionDays =
                serde_json::from_str(&days.to_string()).expect("supported retention");
            assert_eq!(decoded, expected);
            assert_eq!(
                serde_json::to_string(&decoded).expect("encode retention"),
                days.to_string()
            );
            assert_eq!(decoded.days(), i64::from(days));
            assert_eq!(decoded.duration(), chrono::Duration::days(i64::from(days)));
        }

        for invalid in [0, 31, 366, 999] {
            let error = serde_json::from_str::<RetentionDays>(&invalid.to_string())
                .expect_err("unsupported retention must fail");
            assert!(error.to_string().contains(&invalid.to_string()));
        }
    }

    #[test]
    fn legacy_settings_enable_every_compiled_provider_in_registry_order() {
        let stored: Settings = serde_json::from_str(r#"{"trayMode":"strip","theme":"light"}"#)
            .expect("an older settings file");

        assert!(stored.disabled_providers.is_empty());
        assert_eq!(
            stored
                .ordered_provider_ids(&quotadeck_providers::ids())
                .expect("legacy provider order"),
            quotadeck_providers::ids()
        );
        assert_eq!(stored.retention_days, RetentionDays::Days32);
    }

    #[test]
    fn a_retention_change_rolls_back_settings_when_the_read_loop_is_disconnected() {
        let deck = Deck::new().expect("create deck");
        let (control, receiver) = mpsc::channel();
        deck.register_retention_control(control);
        drop(receiver);
        let saved = Arc::new(Mutex::new(Vec::new()));
        let recorded = saved.clone();

        let error = deck
            .set_retention_days_with(RetentionDays::Days90, move |settings| {
                recorded
                    .lock()
                    .expect("saved settings")
                    .push(settings.retention_days);
                Ok(())
            })
            .expect_err("a disconnected read loop must roll back");

        assert!(error.to_string().contains("90"));
        assert_eq!(deck.settings().retention_days, RetentionDays::Days32);
        assert_eq!(
            *saved.lock().expect("saved settings"),
            vec![RetentionDays::Days90, RetentionDays::Days32]
        );
    }

    #[test]
    fn a_retention_change_is_saved_before_the_read_loop_observes_it() {
        let deck = Deck::new().expect("create deck");
        let (control, receiver) = mpsc::channel();
        deck.register_retention_control(control);
        let saved = Arc::new(AtomicBool::new(false));
        let marker = saved.clone();
        let setter = deck.clone();
        let (finished, result) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let outcome = setter.set_retention_days_with(RetentionDays::Days90, move |_| {
                marker.store(true, Ordering::Release);
                Ok(())
            });
            finished.send(outcome).expect("return setter result");
        });
        let request = receiver.recv().expect("receive retention change");
        assert!(saved.load(Ordering::Acquire));
        assert_eq!(request.retention, RetentionDays::Days90);
        assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));

        let mut state = deck.state();
        state.retention.requested_days = 90;
        state.retention.rebuilding = true;
        deck.set_state(state);
        request
            .complete
            .send(Ok(()))
            .expect("acknowledge owner state");
        let settings = result
            .recv()
            .expect("setter completion")
            .expect("accepted change");
        thread.join().expect("setter thread");
        assert_eq!(settings.retention_days, RetentionDays::Days90);
    }

    #[test]
    fn a_second_retention_change_is_rejected_before_saving_while_rebuilding() {
        let deck = Deck::new().expect("create deck");
        let mut state = deck.state();
        state.retention.rebuilding = true;
        state.retention.requested_days = 90;
        deck.set_state(state);
        let saves = Arc::new(AtomicU64::new(0));
        let recorded = saves.clone();

        let error = deck
            .set_retention_days_with(RetentionDays::Days365, move |_| {
                recorded.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .expect_err("active rebuild rejects a second change");

        assert!(error.to_string().contains("90"));
        assert!(error.to_string().contains("rebuilding"));
        assert_eq!(saves.load(Ordering::Relaxed), 0);
        assert_eq!(deck.settings().retention_days, RetentionDays::Days32);
    }

    #[test]
    fn partial_provider_order_appends_new_compiled_providers_in_registry_order() {
        let stored: Settings = serde_json::from_str(
            r#"{"providerOrder":["codex"],"disabledProviders":["claude-code"]}"#,
        )
        .expect("partial settings");

        assert!(!stored.is_provider_enabled(ProviderId::ClaudeCode));
        assert_eq!(
            stored
                .ordered_provider_ids(&quotadeck_providers::ids())
                .expect("partial provider order"),
            vec![
                ProviderId::Codex,
                ProviderId::ClaudeCode,
                ProviderId::CopilotCli
            ]
        );
    }

    #[test]
    fn duplicate_provider_order_is_rejected_with_the_setting_path_and_key() {
        let error = serde_json::from_str::<Settings>(r#"{"providerOrder":["codex","codex"]}"#)
            .expect_err("duplicate provider order must fail");
        let message = error.to_string();
        assert!(message.contains("providerOrder"));
        assert!(message.contains("codex"));
    }

    #[test]
    fn unknown_provider_policy_keys_are_rejected_with_the_setting_path_and_key() {
        let order_error =
            serde_json::from_str::<Settings>(r#"{"providerOrder":["planned-provider"]}"#)
                .expect_err("unknown ordered provider must fail");
        assert!(order_error.to_string().contains("providerOrder"));
        assert!(order_error.to_string().contains("planned-provider"));

        let disabled_error =
            serde_json::from_str::<Settings>(r#"{"disabledProviders":["planned-provider"]}"#)
                .expect_err("unknown disabled provider must fail");
        assert!(disabled_error.to_string().contains("disabledProviders"));
        assert!(disabled_error.to_string().contains("planned-provider"));
    }

    #[test]
    fn failed_provider_policy_save_rolls_back_the_in_memory_snapshot() {
        let deck = Deck::new().expect("create deck");
        let before = deck.settings();

        let result = deck.set_provider_policy_and_sync_with(
            ["codex".to_string()].into_iter().collect(),
            vec!["copilot-cli".into(), "claude-code".into(), "codex".into()],
            |_| {
                Err(Error::Invalid(
                    "simulated provider policy write failure".into(),
                ))
            },
        );

        assert!(result.is_err());
        assert_eq!(deck.settings(), before);
    }

    #[test]
    fn cached_state_and_history_drop_disabled_providers_and_follow_saved_order() {
        let deck = Deck::new().expect("create deck");
        let mut claude = snapshot(&[70.0]);
        claude.id = ProviderId::ClaudeCode;
        let codex = snapshot(&[40.0]);
        deck.set_state(DeckState {
            providers: vec![claude, codex],
            updated_at: Utc::now(),
            scanning: false,
            health: Vec::new(),
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: Default::default(),
        });
        deck.set_history(vec![
            ProviderHistory {
                id: ProviderId::ClaudeCode,
                hours: Vec::new(),
                models: Vec::new(),
                models_dropped: 0,
                projects: Vec::new(),
                projects_dropped: 0,
                agents: Vec::new(),
                agents_dropped: 0,
            },
            ProviderHistory {
                id: ProviderId::Codex,
                hours: Vec::new(),
                models: Vec::new(),
                models_dropped: 0,
                projects: Vec::new(),
                projects_dropped: 0,
                agents: Vec::new(),
                agents_dropped: 0,
            },
        ]);
        let settings = Settings {
            disabled_providers: ["claude-code".to_string()].into_iter().collect(),
            provider_order: vec!["codex".into(), "claude-code".into(), "copilot-cli".into()],
            ..Settings::default()
        };

        deck.apply_provider_policy_to_cached_data(&settings)
            .expect("apply provider policy");

        assert_eq!(
            deck.state()
                .providers
                .iter()
                .map(|provider| provider.id)
                .collect::<Vec<_>>(),
            vec![ProviderId::Codex]
        );
        assert_eq!(
            deck.history()
                .iter()
                .map(|provider| provider.id)
                .collect::<Vec<_>>(),
            vec![ProviderId::Codex]
        );
        assert_eq!(
            deck.state().headline().map(|(provider, _)| provider.id),
            Some(ProviderId::Codex)
        );
    }

    #[test]
    fn provider_policy_commit_gate_rejects_a_stale_pass() {
        let deck = Deck::new().expect("create deck");
        let started_revision = deck.provider_policy_revision();
        let committed = std::cell::Cell::new(false);

        deck.provider_policy_revision
            .fetch_add(1, Ordering::Release);
        let outcome = deck
            .with_current_provider_policy(started_revision, || {
                committed.set(true);
                Ok(())
            })
            .expect("check provider policy revision");

        assert!(outcome.is_none());
        assert!(!committed.get());
    }

    #[test]
    fn provider_policy_setter_waits_for_watcher_reconciliation_without_deadlock() {
        let deck = Deck::new().expect("create deck");
        let (requests, requested) = std::sync::mpsc::channel();
        deck.register_provider_policy_sync(requests);

        let watched = Arc::new(Mutex::new(BTreeSet::from([
            "claude-code".to_string(),
            "codex".to_string(),
        ])));
        let synced_watched = watched.clone();
        let sync_deck = deck.clone();
        let sync_thread = std::thread::spawn(move || {
            let request = requested.recv().expect("provider policy sync request");
            sync_deck
                .with_provider_policy(|settings, revision| {
                    assert!(revision >= request.revision);
                    let mut watched = synced_watched.lock().expect("watched roots lock");
                    watched.retain(|provider| !settings.disabled_providers.contains(provider));
                    Ok(())
                })
                .expect("reconcile watched roots");
            request
                .complete
                .send(Ok(()))
                .expect("acknowledge provider policy sync");
        });

        let setter_deck = deck.clone();
        let (completed, completion) = std::sync::mpsc::channel();
        let setter_thread = std::thread::spawn(move || {
            let result = setter_deck.set_provider_policy_and_sync_with(
                ["claude-code".to_string()].into_iter().collect(),
                vec!["codex".into(), "claude-code".into(), "copilot-cli".into()],
                |_| Ok(()),
            );
            completed.send(result).expect("report setter completion");
        });

        let outcome = completion
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("policy setter must not deadlock behind watcher sync")
            .expect("save provider policy");
        assert!(!outcome.settings.is_provider_enabled(ProviderId::ClaudeCode));
        assert!(outcome.warning.is_none());
        assert_eq!(
            *watched.lock().expect("watched roots lock"),
            BTreeSet::from(["codex".to_string()]),
            "the setter must not return while a disabled provider remains watched"
        );

        setter_thread.join().expect("setter thread");
        sync_thread.join().expect("sync thread");
    }

    #[test]
    fn provider_policy_change_is_blocked_while_watcher_sync_holds_the_policy_gate() {
        let deck = Deck::new().expect("create deck");
        let sync_deck = deck.clone();
        let (entered, sync_started) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let sync_thread = std::thread::spawn(move || {
            sync_deck
                .with_provider_policy(|_settings, _revision| {
                    entered.send(()).expect("report sync start");
                    released.recv().expect("release watcher sync");
                    Ok(())
                })
                .expect("watcher sync");
        });
        sync_started.recv().expect("watcher sync start");

        let setter_deck = deck.clone();
        let (completed, completion) = std::sync::mpsc::channel();
        let setter_thread = std::thread::spawn(move || {
            let result = setter_deck.set_provider_policy_and_sync_with(
                ["claude-code".to_string()].into_iter().collect(),
                vec!["codex".into(), "claude-code".into(), "copilot-cli".into()],
                |_| Ok(()),
            );
            completed.send(result).expect("report setter completion");
        });

        assert!(matches!(
            completion.recv_timeout(std::time::Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        release.send(()).expect("release watcher sync");
        completion
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("policy setter must complete after watcher sync releases the gate")
            .expect("save provider policy");

        setter_thread.join().expect("setter thread");
        sync_thread.join().expect("sync thread");
    }

    #[test]
    fn watcher_sync_delivery_failure_returns_the_saved_policy_with_a_warning() {
        let deck = Deck::new().expect("create deck");
        let (requests, requested) = std::sync::mpsc::channel();
        deck.register_provider_policy_sync(requests);
        drop(requested);

        let outcome = deck
            .set_provider_policy_and_sync_with(
                ["claude-code".to_string()].into_iter().collect(),
                vec!["codex".into(), "claude-code".into(), "copilot-cli".into()],
                |_| Ok(()),
            )
            .expect("a post-save watcher failure must be a warning");

        assert!(!outcome.settings.is_provider_enabled(ProviderId::ClaudeCode));
        assert_eq!(outcome.settings, deck.settings());
        assert!(
            outcome
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("could not be delivered")),
            "the warning must explain why watcher reconciliation failed"
        );
    }

    #[test]
    fn provider_health_preserves_success_then_backs_off_and_resets() {
        let first = DateTime::from_timestamp(1_785_715_200, 0).expect("valid instant");
        let failed = first + chrono::Duration::seconds(30);
        let recovered = failed + chrono::Duration::seconds(20);
        let mut health = ProviderHealth::new(ProviderId::Codex);

        health.record_success(first);
        health.record_failure(failed, "could not read codex log".into(), true);
        assert_eq!(health.state, HealthState::Stale);
        assert_eq!(health.last_success_at, Some(first));
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(
            health.next_retry_at,
            Some(failed + chrono::Duration::seconds(5))
        );
        assert!(!health.retry_due(failed + chrono::Duration::seconds(4), false));
        assert!(
            health.retry_due(failed, true),
            "manual refresh bypasses backoff"
        );

        health.record_failure(
            failed + chrono::Duration::seconds(5),
            "still unreadable".into(),
            true,
        );
        assert_eq!(
            health.next_retry_at,
            Some(failed + chrono::Duration::seconds(15)),
            "the second failure backs off for ten seconds"
        );

        health.record_success(recovered);
        assert_eq!(health.state, HealthState::Healthy);
        assert_eq!(health.last_attempt_at, Some(recovered));
        assert_eq!(health.last_success_at, Some(recovered));
        assert_eq!(health.consecutive_failures, 0);
        assert!(health.last_error.is_none());
        assert!(health.next_retry_at.is_none());
    }

    #[test]
    fn provider_health_distinguishes_disabled_unavailable_and_first_error() {
        let at = DateTime::from_timestamp(1_785_715_200, 0).expect("valid instant");
        let mut disabled = ProviderHealth::new(ProviderId::ClaudeCode);
        disabled.record_disabled();
        assert_eq!(disabled.state, HealthState::Disabled);
        assert!(disabled.last_attempt_at.is_none());

        let mut unavailable = ProviderHealth::new(ProviderId::Codex);
        unavailable.record_unavailable(at, "provider root is not readable".into());
        assert_eq!(unavailable.state, HealthState::Unavailable);
        assert_eq!(unavailable.last_attempt_at, Some(at));

        let mut error = ProviderHealth::new(ProviderId::CopilotCli);
        error.record_failure(at, "provider parser failed".into(), false);
        assert_eq!(error.state, HealthState::Error);
    }

    #[test]
    fn rebuilding_is_explicit_until_a_full_success_or_failure() {
        let at = Utc::now();
        let mut health = ProviderHealth::new(ProviderId::ClaudeCode);
        health.record_rebuilding(at);
        assert_eq!(health.state, HealthState::Rebuilding);
        assert_eq!(health.last_attempt_at, Some(at));
        assert!(health.last_success_at.is_none());
        assert!(health.last_error.is_none());

        health.record_success(at + chrono::Duration::seconds(1));
        assert_eq!(health.state, HealthState::Healthy);

        health.record_rebuilding(at);
        health.record_failure(
            at + chrono::Duration::seconds(1),
            "could not rebuild Claude logs".into(),
            false,
        );
        assert_eq!(health.state, HealthState::Error);
    }

    #[test]
    fn a_failed_provider_attempt_keeps_the_last_successful_snapshot() {
        let previous = snapshot(&[72.0]);
        let preserved = provider_snapshot_after_failure(Some(&previous), ProviderId::Codex);
        assert_eq!(preserved.id, previous.id);
        assert_eq!(preserved.windows.len(), previous.windows.len());
        assert_eq!(preserved.windows[0].used_percent, Some(72.0));

        let first_failure = provider_snapshot_after_failure(None, ProviderId::ClaudeCode);
        assert_eq!(first_failure.id, ProviderId::ClaudeCode);
        assert_eq!(
            first_failure.unavailable,
            Some(UnavailableReason::ReadError)
        );
    }

    #[test]
    fn global_update_time_can_advance_without_changing_provider_success_time() {
        let success = DateTime::from_timestamp(1_785_715_200, 0).expect("valid instant");
        let attempted = success + chrono::Duration::minutes(1);
        let mut health = ProviderHealth::new(ProviderId::Codex);
        health.record_success(success);
        health.record_failure(attempted, "temporary failure".into(), true);
        let state = DeckState {
            providers: Vec::new(),
            health: vec![health],
            updated_at: attempted,
            scanning: false,
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: Default::default(),
        };

        assert_eq!(state.updated_at, attempted);
        assert_eq!(state.health[0].last_success_at, Some(success));
    }

    #[test]
    fn refresh_requests_are_monotonic_and_coalesce_to_the_latest_generation() {
        let deck = Deck::new().expect("create deck");
        let (wake, woken) = mpsc::channel();
        deck.register_refresh_control(wake);

        let first = deck.queue_refresh().expect("queue first refresh");
        let second = deck.queue_refresh().expect("queue second refresh");

        assert_eq!(first.request_id + 1, second.request_id);
        assert_eq!(deck.requested_refresh_generation(), second.request_id);
        assert!(woken.recv().is_ok());
        assert!(woken.recv().is_ok());
    }

    #[test]
    fn refresh_wake_observes_the_published_receipt_generation() {
        let requested = Arc::new(AtomicU64::new(0));
        let observed = requested.clone();
        let (wake, woken) = mpsc::sync_channel(0);
        let consumer = std::thread::spawn(move || {
            woken.recv().expect("receive wake");
            observed.load(Ordering::Acquire)
        });

        publish_refresh_generation(&requested, 7, || {
            wake.send(())
                .map_err(|error| Error::Invalid(error.to_string()))
        })
        .expect("publish refresh generation");

        assert_eq!(consumer.join().expect("consumer completes"), 7);
    }

    #[test]
    fn a_dropped_refresh_control_channel_is_an_actionable_error() {
        let deck = Deck::new().expect("create deck");
        let (wake, woken) = mpsc::channel();
        deck.register_refresh_control(wake);
        drop(woken);

        let error = deck
            .queue_refresh()
            .expect_err("queue must fail")
            .to_string();
        assert!(error.contains("read loop"), "{error}");
        assert!(error.contains("refresh"), "{error}");
        assert_eq!(
            deck.requested_refresh_generation(),
            0,
            "a failed wake must not leave a request pending for a future loop"
        );
    }

    #[test]
    fn a_refresh_generation_completes_every_older_request() {
        assert!(refresh_request_completed(7, 7));
        assert!(refresh_request_completed(8, 7));
        assert!(!refresh_request_completed(6, 7));
    }

    #[test]
    fn settings_serialise_in_the_shape_the_panel_expects() {
        let mut settings = Settings {
            tray_mode: TrayMode::Compact,
            theme: Theme::Dark,
            ..Settings::default()
        };
        settings
            .plans
            .insert("claude-code".into(), "max-20x".into());
        settings.alerts.insert("codex".into(), vec![85, 95]);

        let json = serde_json::to_string(&settings).expect("serialise settings");
        assert_eq!(
            json,
            r#"{"trayMode":"compact","theme":"dark","locale":"system","plans":{"claude-code":"max-20x"},"alerts":{"codex":[85,95]},"mutedUntil":null,"demo":false,"disabledProviders":[],"providerOrder":["claude-code","codex","copilot-cli"],"retentionDays":32}"#
        );
    }

    #[test]
    fn a_settings_file_written_before_plans_existed_still_loads() {
        let stored: Settings = serde_json::from_str(r#"{"trayMode":"strip","theme":"light"}"#)
            .expect("an older settings file");
        assert_eq!(stored.tray_mode, TrayMode::Strip);
        assert!(stored.plans.is_empty());
        // A file written before the language could be picked keeps following the system.
        assert_eq!(stored.locale, Locale::System);
    }

    #[test]
    fn a_chosen_plan_reaches_the_provider_that_declared_it() {
        let mut settings = Settings::default();
        settings.plans.insert("claude-code".into(), "pro".into());

        assert_eq!(
            settings
                .config_for(ProviderId::ClaudeCode)
                .plan_id
                .as_deref(),
            Some("pro")
        );
        // One provider's tier must never leak into another's estimate.
        assert!(settings.config_for(ProviderId::Codex).plan_id.is_none());
    }

    #[test]
    fn a_failed_settings_save_does_not_change_the_in_memory_snapshot() {
        let deck = Deck::new().expect("create deck");
        let before = deck.settings();
        let next_theme = match before.theme {
            Theme::Dark => Theme::Light,
            Theme::System | Theme::Light => Theme::Dark,
        };

        let result = deck.update_settings_with(
            |settings| settings.theme = next_theme,
            |_| Err(Error::Invalid("simulated settings write failure".into())),
        );

        assert!(result.is_err());
        assert_eq!(deck.settings(), before);
    }

    #[test]
    fn theme_is_persisted_in_the_settings_file() {
        let dir = scratch("theme");
        let path = dir.join("settings.json");
        let settings = Settings {
            theme: Theme::Dark,
            ..Settings::default()
        };

        settings.save_to(&path).expect("persist theme");

        let stored: Settings =
            serde_json::from_slice(&std::fs::read(&path).expect("read persisted settings"))
                .expect("parse persisted settings");
        assert_eq!(stored.theme, Theme::Dark);
        std::fs::remove_dir_all(dir).expect("remove scratch directory");
    }

    #[test]
    fn a_missing_settings_file_is_the_only_defaulting_case() {
        let dir = scratch("missing-settings");
        let settings = Settings::load_from(dir.join("settings.json")).expect("first launch");
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn malformed_settings_are_reported_with_the_path() {
        let dir = scratch("malformed-settings");
        std::fs::create_dir_all(&dir).expect("create scratch directory");
        let path = dir.join("settings.json");
        std::fs::write(&path, "{broken").expect("write malformed settings");

        let error = Settings::load_from(&path).expect_err("malformed settings must fail");
        let message = error.to_string();
        assert!(message.contains("invalid settings JSON"));
        assert!(message.contains(path.to_string_lossy().as_ref()));
        std::fs::remove_dir_all(dir).expect("remove scratch directory");
    }
}
