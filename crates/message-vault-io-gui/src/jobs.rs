//! Run exporter libraries on a background thread for the Slint GUI.
//!
//! Each exporter crate exposes a `run` function that takes an `ExporterConfig`.
//! This module picks the matching `run` from `config.source` and wraps it as a
//! [`LibraryJob`] that the GUI can spawn.

use std::sync::mpsc;

use go_sms_pro_exporter::run as run_go_sms_pro;
use imazing_exporter::run as run_imazing;
use imessage_ir_exporter::run as run_imessage;
use message_reexport::run as run_format;
use message_vault_io_core::{
    CancelFlag, ExporterConfig, JobError, LogSink, ProcessEvent, RunResult, SourceConfig,
};
use openextract_exporter::run as run_openextract;
use sms_backup_plus_exporter::run as run_sms_plus;
use sms_backup_restore_exporter::run as run_sms_restore;
use whatsapp_exporter::run as run_whatsapp;

/// Background work item: cancel flag plus a channel for log and finish events.
pub type LibraryJob =
    Box<dyn FnOnce(CancelFlag, mpsc::Sender<ProcessEvent>) -> Result<(), JobError> + Send>;

/// Attach the cancel flag and a log callback that forwards lines onto `tx`.
fn prepare_config(
    mut config: ExporterConfig,
    cancel: CancelFlag,
    tx: &mpsc::Sender<ProcessEvent>,
) -> ExporterConfig {
    config.cancel = Some(cancel);
    let tx = tx.clone();
    config.log = Some(LogSink::new(move |line| {
        let _ = tx.send(ProcessEvent::Log(line.to_string()));
    }));
    config
}

/// Run the exporter named by `config.source` in this process (not as a subprocess).
///
/// `config.source` is set by `Form::to_config` and `Form::to_format_config` from
/// the exporter chosen in the GUI. Adding an exporter means adding one match arm here.
///
/// # Errors
///
/// Returns the exporter's error if conversion or writing fails.
pub fn run_exporter(config: &ExporterConfig) -> anyhow::Result<RunResult> {
    match &config.source {
        SourceConfig::GoSmsPro(_) => run_go_sms_pro(config),
        SourceConfig::SmsBackupRestore(_) => run_sms_restore(config),
        SourceConfig::SmsBackupPlus(_) => run_sms_plus(config),
        SourceConfig::OpenExtract(_) => run_openextract(config),
        SourceConfig::Imazing(_) => run_imazing(config),
        SourceConfig::Apple(_) => run_imessage(config),
        SourceConfig::Whatsapp(_) => run_whatsapp(config),
        SourceConfig::Format(_) => run_format(config),
    }
}

/// Wrap an [`ExporterConfig`] as a background [`LibraryJob`].
///
/// Extract and Format both use this: attach cancel and log sinks, run
/// [`run_exporter`], and forward result lines onto the job's event channel.
pub fn library_job_for_exporter(config: ExporterConfig) -> LibraryJob {
    Box::new(move |cancel, tx| {
        let config = prepare_config(config, cancel, &tx);
        run_and_log(run_exporter(&config), tx)
    })
}

/// Send each success message as a log event, or turn a failure into a [`JobError`].
///
/// # Errors
///
/// Returns [`JobError::detail`] with the Display form of `error` (`{error:#}`).
pub fn run_and_log<R, E: std::fmt::Display>(
    result: Result<R, E>,
    tx: mpsc::Sender<ProcessEvent>,
) -> Result<(), JobError>
where
    R: HasMessages,
{
    match result {
        Ok(run) => {
            for line in run.into_messages() {
                let _ = tx.send(ProcessEvent::Log(line));
            }
            Ok(())
        }
        Err(error) => Err(JobError::detail(format!("{error:#}"))),
    }
}

/// Types that can be turned into log lines after a successful library run.
pub trait HasMessages {
    /// Consume the value and return the lines to show in the session log.
    fn into_messages(self) -> Vec<String>;
}

impl HasMessages for RunResult {
    fn into_messages(self) -> Vec<String> {
        self.messages
    }
}
