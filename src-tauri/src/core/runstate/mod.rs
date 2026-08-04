//! Run State — decision logic for "how is the Core running, and what backs it".
//!
//! Ported from upstream in stages. This is the first: the pure decision
//! modules, which carry no state of their own and are covered by their own
//! tests. The store that owns Service Health, Running Mode, Pending Action and
//! the privileged-operation lock lands on top of these, together with
//! upstream's `env` module — the effects boundary, which needs the service
//! install-state machinery and PAC/TUN wiring that this fork has not ported.

mod health;
mod owner;
mod probe;

pub(crate) use owner::{OwnerRecoveryReason, OwnerSample, OwnerStep, OwnerWatch};
pub(crate) use probe::{ServiceVersionCheck, ServiceVersionReply, classify_service_version_reply};
