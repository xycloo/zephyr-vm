use std::collections::BTreeMap;

use tokio::{sync::Mutex, task::JoinHandle};

pub struct JobsManager {
    jobs: Mutex<BTreeMap<u32, JoinHandle<String>>>,
    latest: Mutex<u32>,
}

impl JobsManager {
    pub fn new() -> Self {
        Self {
            jobs: BTreeMap::new().into(),
            latest: 0.into(),
        }
    }

    pub async fn add_job(&self, job: JoinHandle<String>) -> u32 {
        let mut jobs = self.jobs.lock().await;
        let current = self.latest.lock().await.checked_add(1).unwrap();
        jobs.insert(current, job);

        *self.latest.lock().await += 1;

        current
    }

    pub async fn read_job(&self, id: u32) -> Option<String> {
        let jobs = self.jobs.lock().await;
        if let Some(job) = jobs.get(&id) {
            if job.is_finished() {
                return Some("Catchup completed".into());
            }
        }

        None
    }
}
