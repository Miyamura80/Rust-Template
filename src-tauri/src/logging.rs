use crate::global_config::{get_config, AppConfig};
use regex::Regex;
use std::io;
use std::sync::{Arc, OnceLock};
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Layer};

static SESSION_ID: OnceLock<String> = OnceLock::new();

fn get_session_id() -> &'static str {
    SESSION_ID.get_or_init(|| {
        use rand::distributions::Alphanumeric;
        use rand::{thread_rng, Rng};
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect()
    })
}

struct RedactingWriter<W> {
    inner: W,
    patterns: Arc<Vec<(Regex, String)>>,
    session_id: Option<Arc<String>>,
}

impl<W: io::Write> io::Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s = String::from_utf8_lossy(buf);
        let mut redacted = s;

        // Prepend session ID if enabled
        if let Some(ref id) = self.session_id {
            // Only prepend to lines that aren't just whitespace/newlines
            if !redacted.trim().is_empty() {
                redacted = std::borrow::Cow::Owned(format!("[{}] {}", id, redacted));
            }
        }

        for (re, replacement) in self.patterns.iter() {
            if let std::borrow::Cow::Owned(s) = re.replace_all(&redacted, replacement) {
                redacted = std::borrow::Cow::Owned(s);
            }
        }
        self.inner.write_all(redacted.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

struct RedactingMakeWriter {
    patterns: Arc<Vec<(Regex, String)>>,
    session_id: Option<Arc<String>>,
}

impl<'a> fmt::MakeWriter<'a> for RedactingMakeWriter {
    type Writer = RedactingWriter<io::Stdout>;

    fn make_writer(&self) -> Self::Writer {
        RedactingWriter {
            inner: io::stdout(),
            patterns: self.patterns.clone(),
            session_id: self.session_id.clone(),
        }
    }
}

fn determine_log_level(config: &AppConfig) -> &'static str {
    // Determine the log level from config - pick the most verbose one enabled.
    // In a hierarchical system like tracing, the most verbose level (e.g., debug)
    // naturally includes all less verbose levels (e.g., info, warn, error).
    // We select the "widest" enabled threshold to ensure the user's request for
    // verbosity is honored even if multiple levels are checked.
    if config.logging.verbose || config.logging.levels.debug {
        "debug"
    } else if config.logging.levels.info {
        "info"
    } else if config.logging.levels.warning {
        "warn"
    } else if config.logging.levels.error || config.logging.levels.critical {
        "error"
    } else {
        "off"
    }
}

pub fn init_logging() {
    let config = get_config();
    let level = determine_log_level(config);

    // Use the level from config as the base filter.
    // Note: try_from_default_env() is skipped to ensure config is the source of truth.
    let filter = EnvFilter::new(level);

    // Base formatter configuration
    let location = &config.logging.format.location;
    let show_file = location.show_file;
    let show_line = location.show_line;
    let show_target = location.show_function; // Map show_function to tracing's target display

    // TODO: Implement per-level location display control (show_for_info, show_for_debug, etc.)
    // in Phase 4. Currently, location settings are applied globally if enabled.
    // This requires separate layers for each level using with_filter().

    // Setup redaction patterns
    let mut patterns = Vec::new();
    if config.logging.redaction.enabled {
        for p in &config.logging.redaction.patterns {
            match Regex::new(&p.regex) {
                Ok(re) => patterns.push((re, p.placeholder.clone())),
                Err(e) => eprintln!(
                    "Warning: Failed to compile redaction regex '{}': {}",
                    p.name, e
                ),
            }
        }
    }

    let patterns = Arc::new(patterns);

    let session_id = if config.logging.format.show_session_id {
        Some(Arc::new(get_session_id().to_string()))
    } else {
        None
    };

    let make_writer = RedactingMakeWriter {
        patterns,
        session_id,
    };

    // Use Layer::boxed() to unify the types of the if/else branches
    let fmt_layer = if !config.logging.format.show_time {
        fmt::layer()
            .with_writer(make_writer)
            .with_target(show_target)
            .with_file(show_file)
            .with_line_number(show_line)
            .with_thread_ids(false)
            .without_time()
            .boxed()
    } else {
        fmt::layer()
            .with_writer(make_writer)
            .with_target(show_target)
            .with_file(show_file)
            .with_line_number(show_line)
            .with_thread_ids(false)
            .boxed()
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Arc;

    /// Helper: create a RedactingWriter that writes to a Vec<u8> buffer.
    fn make_writer<'a>(
        buffer: &'a mut Vec<u8>,
        patterns: Vec<(Regex, String)>,
        session_id: Option<&'a str>,
    ) -> RedactingWriter<&'a mut Vec<u8>> {
        RedactingWriter {
            inner: buffer,
            patterns: Arc::new(patterns),
            session_id: session_id.map(|s| Arc::new(s.to_string())),
        }
    }

    // ── Session ID ──────────────────────────────────────────────────────

    #[test]
    fn test_session_id_generation() {
        let id1 = get_session_id();
        let id2 = get_session_id();
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 8);
        assert!(id1.chars().all(|c| c.is_alphanumeric()));
    }

    // ── RedactingWriter: session ID behaviour ───────────────────────────

    #[test]
    fn test_session_id_prepended_to_output() {
        let mut buf = Vec::new();
        let mut w = make_writer(&mut buf, vec![], Some("SESS01"));
        w.write_all(b"hello world").unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(out, "[SESS01] hello world");
    }

    #[test]
    fn test_no_session_id_when_disabled() {
        let mut buf = Vec::new();
        let mut w = make_writer(&mut buf, vec![], None);
        w.write_all(b"hello world").unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(out, "hello world");
        assert!(!out.contains('['));
    }

    #[test]
    fn test_session_id_skipped_for_whitespace_only_input() {
        let mut buf = Vec::new();
        let mut w = make_writer(&mut buf, vec![], Some("SESS01"));
        w.write_all(b"   \n  ").unwrap();
        let out = String::from_utf8(buf).unwrap();
        // Whitespace-only lines are passed through without the prefix
        assert!(!out.contains("[SESS01]"));
        assert_eq!(out, "   \n  ");
    }

    #[test]
    fn test_session_id_skipped_for_empty_input() {
        let mut buf = Vec::new();
        let mut w = make_writer(&mut buf, vec![], Some("SESS01"));
        w.write_all(b"").unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(!out.contains("[SESS01]"));
    }

    // ── RedactingWriter: single-pattern redaction ───────────────────────

    #[test]
    fn test_single_pattern_redacts_match() {
        let mut buf = Vec::new();
        let patterns = vec![(Regex::new(r"password=\w+").unwrap(), "password=***".into())];
        let mut w = make_writer(&mut buf, patterns, None);
        w.write_all(b"login password=secret123").unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("password=***"));
        assert!(!out.contains("secret123"));
    }

    #[test]
    fn test_no_match_passes_through_unchanged() {
        let mut buf = Vec::new();
        let patterns = vec![(Regex::new(r"password=\w+").unwrap(), "password=***".into())];
        let mut w = make_writer(&mut buf, patterns, None);
        w.write_all(b"just a normal log line").unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(out, "just a normal log line");
    }

    #[test]
    fn test_multiple_occurrences_of_same_pattern_all_redacted() {
        let mut buf = Vec::new();
        let patterns = vec![(Regex::new(r"tok_\w+").unwrap(), "[REDACTED]".into())];
        let mut w = make_writer(&mut buf, patterns, None);
        w.write_all(b"auth tok_abc then tok_xyz end").unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(out, "auth [REDACTED] then [REDACTED] end");
    }

    // ── RedactingWriter: multiple patterns ──────────────────────────────

    #[test]
    fn test_multiple_patterns_applied_sequentially() {
        let mut buf = Vec::new();
        let patterns = vec![
            (Regex::new(r"secret=\w+").unwrap(), "secret=***".into()),
            (Regex::new(r"\d{3}-\d{2}-\d{4}").unwrap(), "[SSN]".into()),
        ];
        let mut w = make_writer(&mut buf, patterns, None);
        w.write_all(b"secret=hunter2 ssn=123-45-6789").unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("secret=***"));
        assert!(out.contains("[SSN]"));
        assert!(!out.contains("hunter2"));
        assert!(!out.contains("123-45-6789"));
    }

    #[test]
    fn test_empty_patterns_list_passes_through() {
        let mut buf = Vec::new();
        let mut w = make_writer(&mut buf, vec![], None);
        w.write_all(b"secret=hunter2 password=abc").unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(out, "secret=hunter2 password=abc");
    }

    // ── RedactingWriter: combined session ID + redaction ─────────────────

    #[test]
    fn test_session_id_and_redaction_together() {
        let mut buf = Vec::new();
        let patterns = vec![(Regex::new(r"password=\w+").unwrap(), "password=***".into())];
        let mut w = make_writer(&mut buf, patterns, Some("TESTID"));
        w.write_all(b"login password=secret").unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("[TESTID]"));
        assert!(out.contains("password=***"));
        assert!(!out.contains("secret"));
    }

    #[test]
    fn test_redaction_applies_after_session_id_prepend() {
        // If a pattern matches something in the session ID prefix itself,
        // it should still be redacted (defense in depth).
        let mut buf = Vec::new();
        let patterns = vec![(Regex::new(r"AB12").unwrap(), "[GONE]".into())];
        let mut w = make_writer(&mut buf, patterns, Some("AB12"));
        w.write_all(b"hello").unwrap();
        let out = String::from_utf8(buf).unwrap();
        // The session ID "AB12" is prepended, then the pattern replaces it
        assert!(out.contains("[GONE]"));
        assert!(!out.contains("AB12"));
    }

    // ── RedactingWriter: unicode ────────────────────────────────────────

    #[test]
    fn test_unicode_content_preserved() {
        let mut buf = Vec::new();
        let mut w = make_writer(&mut buf, vec![], None);
        w.write_all("日本語テスト 🔒".as_bytes()).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(out, "日本語テスト 🔒");
    }

    #[test]
    fn test_redaction_pattern_with_unicode() {
        let mut buf = Vec::new();
        let patterns = vec![(Regex::new(r"キー=\w+").unwrap(), "キー=***".into())];
        let mut w = make_writer(&mut buf, patterns, None);
        w.write_all("ログ キー=secret".as_bytes()).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("キー=***"));
        assert!(!out.contains("secret"));
    }

    // ── RedactingWriter: write returns original byte count ──────────────

    #[test]
    fn test_write_returns_original_buf_len() {
        let mut buf = Vec::new();
        let patterns = vec![(Regex::new(r"x").unwrap(), "EXPANDED".into())];
        let mut w = make_writer(&mut buf, patterns, Some("ID"));
        // "x" is 1 byte, but after session ID + expansion the output is much longer
        let n = w.write(b"x").unwrap();
        assert_eq!(n, 1, "write() must return the original buffer length");
    }

    // ── RedactingWriter: flush ──────────────────────────────────────────

    #[test]
    fn test_flush_propagates() {
        let mut buf = Vec::new();
        let mut w = make_writer(&mut buf, vec![], None);
        w.write_all(b"data").unwrap();
        // flush should not panic or error
        w.flush().unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "data");
    }

    // ── determine_log_level ─────────────────────────────────────────────

    fn create_test_config() -> AppConfig {
        use crate::global_config::*;
        use std::collections::HashMap;
        AppConfig {
            model_name: "test".into(),
            dot_global_config_health_check: true,
            dev_env: "test".into(),
            example_parent: ExampleParent {
                example_child: "val".into(),
            },
            default_llm: DefaultLlm {
                default_model: "test".into(),
                fallback_model: None,
                default_temperature: 0.5,
                default_max_tokens: 100,
            },
            llm_config: LlmConfig {
                cache_enabled: false,
                retry: RetryConfig {
                    max_attempts: 1,
                    min_wait_seconds: 1,
                    max_wait_seconds: 1,
                },
            },
            logging: LoggingConfig {
                verbose: false,
                format: LoggingFormatConfig {
                    show_time: false,
                    show_session_id: false,
                    location: LoggingLocationConfig {
                        enabled: false,
                        show_file: false,
                        show_function: false,
                        show_line: false,
                        show_for_info: false,
                        show_for_debug: false,
                        show_for_warning: false,
                        show_for_error: false,
                    },
                },
                levels: LoggingLevelsConfig {
                    debug: false,
                    info: false,
                    warning: false,
                    error: false,
                    critical: false,
                },
                redaction: RedactionConfig::default(),
            },
            features: HashMap::new(),
            openai_api_key: None,
            anthropic_api_key: None,
            groq_api_key: None,
            perplexity_api_key: None,
            gemini_api_key: None,
        }
    }

    #[test]
    fn test_determine_log_level_verbose_overrides_everything() {
        let mut config = create_test_config();
        config.logging.verbose = true;
        config.logging.levels.debug = false;
        assert_eq!(determine_log_level(&config), "debug");
    }

    #[test]
    fn test_determine_log_level_debug() {
        let mut config = create_test_config();
        config.logging.levels.debug = true;
        assert_eq!(determine_log_level(&config), "debug");
    }

    #[test]
    fn test_determine_log_level_info() {
        let mut config = create_test_config();
        config.logging.levels.info = true;
        assert_eq!(determine_log_level(&config), "info");
    }

    #[test]
    fn test_determine_log_level_warning() {
        let mut config = create_test_config();
        config.logging.levels.warning = true;
        assert_eq!(determine_log_level(&config), "warn");
    }

    #[test]
    fn test_determine_log_level_error() {
        let mut config = create_test_config();
        config.logging.levels.error = true;
        assert_eq!(determine_log_level(&config), "error");
    }

    #[test]
    fn test_determine_log_level_critical_maps_to_error() {
        let mut config = create_test_config();
        config.logging.levels.critical = true;
        assert_eq!(determine_log_level(&config), "error");
    }

    #[test]
    fn test_determine_log_level_off_when_nothing_enabled() {
        let config = create_test_config();
        assert_eq!(determine_log_level(&config), "off");
    }

    #[test]
    fn test_determine_log_level_most_verbose_wins() {
        // When multiple levels are enabled, the most verbose one wins
        let mut config = create_test_config();
        config.logging.levels.info = true;
        config.logging.levels.warning = true;
        config.logging.levels.error = true;
        // info is more verbose than warning/error, so it should win
        assert_eq!(determine_log_level(&config), "info");
    }

    #[test]
    fn test_determine_log_level_verbose_plus_all_levels() {
        let mut config = create_test_config();
        config.logging.verbose = true;
        config.logging.levels.debug = true;
        config.logging.levels.info = true;
        config.logging.levels.warning = true;
        config.logging.levels.error = true;
        config.logging.levels.critical = true;
        assert_eq!(determine_log_level(&config), "debug");
    }

    #[test]
    fn test_determine_log_level_error_and_critical_both_map_to_error() {
        let mut config = create_test_config();
        config.logging.levels.error = true;
        config.logging.levels.critical = true;
        assert_eq!(determine_log_level(&config), "error");
    }
}
