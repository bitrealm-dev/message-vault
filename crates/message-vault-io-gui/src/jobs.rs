//! In-process exporter job adapters for the Slint GUI.

use std::sync::mpsc;

use go_sms_pro_exporter::run as run_go_sms_pro;
use imazing_exporter::run as run_imazing;
use imessage_ir_exporter::run as run_imessage;
use message_vault_io_core::{
    CancelFlag, Exporter, ExporterConfig, JobError, LogSink, ProcessEvent, RunResult,
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

pub fn library_job_for_exporter(exporter: Exporter, config: ExporterConfig) -> LibraryJob {
    match exporter {
        Exporter::GoSmsPro => Box::new(move |cancel, tx| {
            let config = prepare_config(config, cancel, &tx);
            run_and_log(run_go_sms_pro(&config), tx)
        }),
        Exporter::SmsBackupRestore => Box::new(move |cancel, tx| {
            let config = prepare_config(config, cancel, &tx);
            run_and_log(run_sms_restore(&config), tx)
        }),
        Exporter::SmsBackupPlus => Box::new(move |cancel, tx| {
            let config = prepare_config(config, cancel, &tx);
            run_and_log(run_sms_plus(&config), tx)
        }),
        Exporter::OpenExtract => Box::new(move |cancel, tx| {
            let config = prepare_config(config, cancel, &tx);
            run_and_log(run_openextract(&config), tx)
        }),
        Exporter::Imazing => Box::new(move |cancel, tx| {
            let config = prepare_config(config, cancel, &tx);
            run_and_log(run_imazing(&config), tx)
        }),
        Exporter::Whatsapp => Box::new(move |cancel, tx| {
            let config = prepare_config(config, cancel, &tx);
            run_and_log(run_whatsapp(&config), tx)
        }),
        Exporter::Imessage => Box::new(move |cancel, tx| {
            let config = prepare_config(config, cancel, &tx);
            run_and_log(run_imessage(&config), tx)
        }),
    }
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

pub fn prepare_library_config(
    config: ExporterConfig,
    cancel: CancelFlag,
    tx: &mpsc::Sender<ProcessEvent>,
) -> ExporterConfig {
    prepare_config(config, cancel, tx)
}

pub trait HasMessages {
    fn into_messages(self) -> Vec<String>;
}

impl HasMessages for RunResult {
    fn into_messages(self) -> Vec<String> {
        self.messages
    }
}
