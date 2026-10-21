//! Host-side AI adaptation.
//!
//! The portable AI rules live in `datazen_feature_core::ai`, the provider
//! contracts in `datazen_ai_api` and the provider implementations in
//! `datazen_ai_providers`. What stays here is the part that legitimately
//! needs host services: the schema-metadata source the AI feature reads.

pub(crate) mod metadata_source;
