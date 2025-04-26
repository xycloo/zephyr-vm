//! Executor is responsible for executing the zephyrVM.
//!
//! We package executor as a standalone binary to execute the ZVM as a subprocess as opposed to a new
//! thread which grants more control over execution timing and isolation.
//!
//!

use anyhow::anyhow;
use base64::prelude::*;
use executor::db::mercury_db::MercuryDatabase;
use executor::ledger::LedgerReader;
use executor::requests::WebhookJob;
use multiuser_logging_service::{LogLevel, LoggingClient, MercuryLog};
use rs_zephyr_common::RelayedMessageRequest;
use serde::{Deserialize, Serialize};
use std::{env, rc::Rc, sync::Arc};
use tokio::{fs, io::AsyncWriteExt, runtime::Handle, sync::mpsc::UnboundedSender};
use zephyr_vm::{host::Host, vm::Vm};

#[derive(Deserialize, Serialize)]
pub enum BinaryType {
    Path(String),
    Code(Vec<u8>),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct UserLogPair {
    pub user: i64,
    pub log: MercuryLog,
}

#[derive(Deserialize, Serialize)]
pub struct FInput {
    associated_data: Vec<u8>,
    binary: BinaryType,
    user_id: u64,
    network_id: [u8; 32],
    fname: String,
}

impl FInput {
    pub async fn execute_with_channel(
        self,
        tx: UnboundedSender<Vec<u8>>,
        logger: Arc<UnboundedSender<UserLogPair>>,
    ) -> anyhow::Result<String> {
        let binary = match self.binary {
            BinaryType::Code(code) => code.clone(),
            BinaryType::Path(path) => fs::read(path).await?,
        };

        let blocking = Handle::current().spawn_blocking(move || -> anyhow::Result<String> {
            let mut host = Host::<MercuryDatabase, LedgerReader>::from_id(
                self.user_id as i64,
                self.network_id,
            )
            .unwrap();
            host.add_transmitter(tx);

            let start = std::time::Instant::now();
            let vm = Vm::new(&host, &binary);

            let vm = match vm {
                Ok(vm) => vm,
                Err(e) => {
                    let _ = logger.send(UserLogPair {
                        user: self.user_id as i64,
                        log: MercuryLog {
                            level: LogLevel::Debug,
                            message: format!("Zephyr program could not instantiate: {:?}", e),
                            data: None,
                        },
                    });

                    return Err(anyhow!("Failed to instantiate: {:?}", e).into());
                }
            };

            host.load_context(Rc::downgrade(&vm)).unwrap();
            host.add_ledger_close_meta(self.associated_data).unwrap();

            let res = vm
                .metered_function_call(&host, &self.fname)
                .unwrap_or("no response".into());

            let _ = logger.send(UserLogPair {
                user: self.user_id as i64,
                log: MercuryLog {
                    level: LogLevel::Debug,
                    message: format!(
                        "Zephyr program executed. Elapsed: {:?}, result: {}",
                        start.elapsed(),
                        res
                    ),
                    data: None,
                },
            });

            Ok(res)
        });
        let res = blocking.await??;
        Ok(res)
    }
}

#[tokio::main]
async fn main() {
    // we pass the input as a B64 encoded string.
    let vm_input: String = env::args().nth(1).unwrap_or_default();
    let decoded = BASE64_STANDARD
        .decode(vm_input)
        .expect("invalid input format");
    let function: FInput = bincode::deserialize(&decoded).unwrap();
    let user_id = function.user_id as i64;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let (logger, mut client_tx) = tokio::sync::mpsc::unbounded_channel::<UserLogPair>();

    tokio::spawn(async move {
        let client = LoggingClient::new();
        while let Some(i) = client_tx.recv().await {
            // nb: requires multiuser logging service to be running locally.
            let _ = client.send_log(i.user, i.log.level, i.log.message).await;
        }
    });

    let handle = Handle::current();
    let logger = Arc::new(logger);

    let spawn = handle.spawn(function.execute_with_channel(tx, logger.clone()));

    let _ = tokio::spawn(async move {
        let mut handles = Vec::new();
        while let Some(message) = rx.recv().await {
            let service_logger = logger.clone();
            let request: RelayedMessageRequest = bincode::deserialize(&message).unwrap();

            match request {
                RelayedMessageRequest::Http(request) => {
                    let handle = tokio::spawn(async move {
                        let logger = service_logger;

                        let builder = WebhookJob::build_request(request.clone());
                        let builder_cloned = builder.try_clone();

                        let response = builder.send().await;
                        let mut job = WebhookJob {
                            builder_cloned,
                            response,
                            request,
                        };

                        if !job.is_success() {
                            let _ = logger.send(UserLogPair {
                                user: user_id,
                                log: MercuryLog {
                                    level: LogLevel::Error,
                                    message: format!(
                                        "Zephyr program from cloud execution logged: {:?}",
                                        job.inspect().unwrap_or("Reqwest error".into())
                                    ),
                                    data: None,
                                },
                            });

                            let result = job.handle_retry(3).await;

                            if result.is_ok() {
                                let _ = logger.send(UserLogPair {
                                    user: user_id,
                                    log: MercuryLog {
                                        level: LogLevel::Warning,
                                        message: format!(
                                            "Zephyr program from cloud execution logged: Request succeeded after {} attempts",
                                            result.unwrap()
                                        ),
                                        data: None,
                                    },
                                });
                            } else {
                                let _ = logger.send(UserLogPair {
                                    user: user_id,
                                    log: MercuryLog {
                                        level: LogLevel::Error,
                                        message: format!(
                                            "Zephyr program from cloud execution logged: Exceeded max retries, request still failing: {}",
                                            result.err().unwrap()
                                        ),
                                        data: None,
                                    },
                                });
                            }
                        }
                    });

                    handles.push(handle)
                }

                RelayedMessageRequest::Log(log) => {
                    let _ = logger.send(UserLogPair {
                        user: user_id,
                        log: MercuryLog { level: LogLevel::Debug, message: format!("Zephyr program from cloud execution logged: {:?}", log), data: None }
                    });

                    println!("{:?}", log);
                }
            }
        }

        for handle in handles {
            let _result = handle.await;
        }
    })
    .await;
    
    let result = spawn.await;
    if let Ok(Ok(result)) = result {
        let _ = tokio::io::stdout().write_all(result.as_bytes()).await;
    } else {
        let _ = tokio::io::stdout()
            .write_all(b"finished execution without output")
            .await;
    }
}
