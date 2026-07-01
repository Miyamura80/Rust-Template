//! Application context – holds capability trait objects and config.
//!
//! Two layers, mirroring the transport design:
//! - [`AppContext`] holds the shared OS capabilities (fs/net/clipboard). It is
//!   built once at process/transport start and shared (e.g. behind an `Arc` in
//!   the HTTP server's state).
//! - [`Ctx`] is constructed **per request/invocation**, borrowing the shared
//!   capabilities and carrying request-scoped data (`request_id`, `deadline`).
//!   Commands receive `&Ctx`. This is the seam where auth/identity would later
//!   attach — no identity field exists yet, by design.

use crate::platform::{HeadlessClipboard, ReqwestNetwork, StdFilesystem, SystemClipboard};
use crate::traits::*;
use crate::types::{detect_headless, new_run_id};
use std::time::Instant;

/// Central context passed to all engine operations.
///
/// Holds trait-object capabilities so callers (CLI / HTTP API / tests) can swap
/// implementations (e.g. headless stubs vs real platform capabilities).
pub struct AppContext {
    fs: Box<dyn FilesystemOps>,
    network: Box<dyn NetworkOps>,
    clipboard: Box<dyn ClipboardOps>,
    /// Target host for network probe (configurable).
    pub network_probe_host: String,
}

impl AppContext {
    pub fn new(
        fs: Box<dyn FilesystemOps>,
        network: Box<dyn NetworkOps>,
        clipboard: Box<dyn ClipboardOps>,
    ) -> Self {
        Self {
            fs,
            network,
            clipboard,
            network_probe_host: "https://httpbin.org/get".to_string(),
        }
    }

    /// Create a context with real platform implementations, choosing the
    /// appropriate clipboard based on headless detection.
    pub fn default_platform() -> Self {
        let clipboard: Box<dyn ClipboardOps> = if detect_headless() {
            Box::new(HeadlessClipboard)
        } else {
            Box::new(SystemClipboard)
        };
        Self {
            fs: Box::new(StdFilesystem),
            network: Box::new(ReqwestNetwork),
            clipboard,
            network_probe_host: "https://httpbin.org/get".to_string(),
        }
    }

    /// Create a context suitable for headless / CI environments.
    pub fn default_headless() -> Self {
        Self {
            fs: Box::new(StdFilesystem),
            network: Box::new(ReqwestNetwork),
            clipboard: Box::new(HeadlessClipboard),
            network_probe_host: "https://httpbin.org/get".to_string(),
        }
    }

    pub fn fs(&self) -> &dyn FilesystemOps {
        self.fs.as_ref()
    }

    pub fn network(&self) -> &dyn NetworkOps {
        self.network.as_ref()
    }

    pub fn clipboard(&self) -> &dyn ClipboardOps {
        self.clipboard.as_ref()
    }
}

/// Per-request context passed to every [`Command`](crate::commands::Command).
///
/// Borrows the shared [`AppContext`] capabilities and adds request-scoped
/// state. Built fresh for each invocation so each request gets its own
/// `request_id` (and, when set, `deadline`).
pub struct Ctx<'a> {
    /// Unique id for this invocation (surfaced as `run_id` in the CLI envelope
    /// and the `x-run-id` HTTP header).
    pub request_id: String,
    /// Optional wall-clock deadline for this invocation. Enforcement lives in
    /// the transport (scenario runner / tower timeout); commands may consult it.
    pub deadline: Option<Instant>,
    caps: &'a AppContext,
}

impl<'a> Ctx<'a> {
    /// Build a per-request context over shared capabilities with a fresh id.
    pub fn new(caps: &'a AppContext) -> Self {
        Self {
            request_id: new_run_id(),
            deadline: None,
            caps,
        }
    }

    /// Build a context with a caller-supplied request id (e.g. an incoming
    /// `x-run-id`/trace header).
    pub fn with_request_id(caps: &'a AppContext, request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            deadline: None,
            caps,
        }
    }

    /// Attach a deadline (builder-style).
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// The shared capability bundle.
    pub fn caps(&self) -> &AppContext {
        self.caps
    }

    pub fn fs(&self) -> &dyn FilesystemOps {
        self.caps.fs()
    }

    pub fn network(&self) -> &dyn NetworkOps {
        self.caps.network()
    }

    pub fn clipboard(&self) -> &dyn ClipboardOps {
        self.caps.clipboard()
    }
}
