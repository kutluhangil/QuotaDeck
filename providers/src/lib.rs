//! Provider implementations and the registry that owns them.
//!
//! One tool is one module here, registered in [`all`]. Nothing outside this crate should
//! ever match on a specific provider.

pub mod claude_code;
pub mod codex;

use quotadeck_core::provider::Provider;
use quotadeck_core::types::ProviderId;

/// Every provider this build knows how to read.
pub fn all() -> Vec<Box<dyn Provider>> {
    vec![Box::new(claude_code::ClaudeCode), Box::new(codex::Codex)]
}

/// Providers whose root directories exist on this machine.
pub fn installed() -> Vec<Box<dyn Provider>> {
    all()
        .into_iter()
        .filter(|provider| !provider.discover_roots().is_empty())
        .collect()
}

pub fn by_id(id: ProviderId) -> Option<Box<dyn Provider>> {
    all().into_iter().find(|provider| provider.id() == id)
}

/// Look a provider up by its stable key, as used on the command line.
pub fn by_key(key: &str) -> Option<Box<dyn Provider>> {
    all()
        .into_iter()
        .find(|provider| provider.id().key() == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registered_providers_have_distinct_ids() {
        let ids: HashSet<ProviderId> = all().iter().map(|provider| provider.id()).collect();
        assert_eq!(ids.len(), all().len());
    }

    #[test]
    fn every_registered_provider_is_reachable_by_its_key() {
        for provider in all() {
            let key = provider.id().key();
            assert!(by_key(key).is_some(), "{key} is not reachable by key");
        }
    }

    #[test]
    fn every_registered_provider_declares_at_least_one_glob() {
        for provider in all() {
            assert!(
                !provider.watch_globs().is_empty(),
                "{} declares no watch globs",
                provider.display_name()
            );
        }
    }
}
