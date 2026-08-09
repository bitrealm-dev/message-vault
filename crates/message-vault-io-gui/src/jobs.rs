//! In-process exporter job adapters for the Slint GUI.

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

pub type LibraryJob =
    Box<dyn FnOnce(CancelFlag, mpsc::Sender<ProcessEvent>) -> Result<(), JobError> + Send>;

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

/// Run the exporter identified by `config.source` in-process (no subprocess).
///
/// Single dispatch point for every library runner: the seven exporter crates
/// plus `message-reexport` (Format tab). `config.source` is authoritative —
/// `Form::to_config` / `to_format_config` set it from the exporter chosen in
/// the GUI. Keeping the match here means adding an exporter touches one place
/// instead of every caller.
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

/// Wrap a runnable [`ExporterConfig`] as a background [`LibraryJob`].
///
/// Both the extract tabs and the Format tab build the same kind of job: wire
/// cancel/log sinks into the config, run it through [`run_exporter`], and
/// forward the result lines onto the job's event channel.
pub fn library_job_for_exporter(config: ExporterConfig) -> LibraryJob {
    Box::new(move |cancel, tx| {
        let config = prepare_config(config, cancel, &tx);
        run_and_log(run_exporter(&config), tx)
    })
}

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

pub trait HasMessages {
    fn into_messages(self) -> Vec<String>;
}

impl HasMessages for RunResult {
    fn into_messages(self) -> Vec<String> {
        self.messages
    }
}
