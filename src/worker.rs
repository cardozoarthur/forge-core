use chrono::Utc;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

const WORKER_POOL_SCHEMA_VERSION: &str = "forge.worker_pool.v1";

#[derive(Debug, Clone, Serialize)]
pub struct WorkerPoolReport {
    pub schema_version: String,
    pub status: String,
    pub total_jobs: usize,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub cancelled: bool,
    pub max_concurrency: usize,
    pub wave_count: usize,
    pub duration_ms: i64,
    pub backpressure_active: bool,
}

pub struct WorkerPool {
    max_concurrency: usize,
    cancelled: Arc<AtomicBool>,
}

type Job = Box<dyn FnOnce() -> Result<(), String> + Send>;

impl WorkerPool {
    pub fn new(max_concurrency: usize) -> Self {
        Self {
            max_concurrency: max_concurrency.max(1),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn execute(&self, jobs: Vec<Job>) -> WorkerPoolReport {
        let start = Utc::now();
        let total_jobs = jobs.len();
        let completed = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));

        if total_jobs == 0 {
            return WorkerPoolReport {
                schema_version: WORKER_POOL_SCHEMA_VERSION.to_string(),
                status: "completed".to_string(),
                total_jobs: 0,
                completed_jobs: 0,
                failed_jobs: 0,
                cancelled: false,
                max_concurrency: self.max_concurrency,
                wave_count: 0,
                duration_ms: 0,
                backpressure_active: false,
            };
        }

        let mut wave_count = 0usize;
        let mut remaining = jobs;
        while !remaining.is_empty() {
            wave_count += 1;
            let chunk_size = self.max_concurrency.min(remaining.len());
            let chunk: Vec<Job> = remaining.drain(..chunk_size).collect();
            let mut handles = Vec::new();

            for job in chunk {
                if self.is_cancelled() {
                    break;
                }
                let completed = Arc::clone(&completed);
                let failed = Arc::clone(&failed);

                let handle = std::thread::spawn(move || match job() {
                    Ok(()) => {
                        completed.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                if handle.join().is_err() {
                    self.cancelled.store(true, Ordering::SeqCst);
                    failed.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        let duration_ms = (Utc::now() - start).num_milliseconds();
        let completed_jobs = completed.load(Ordering::SeqCst);
        let failed_jobs = failed.load(Ordering::SeqCst);
        let cancelled = self.is_cancelled();
        let backpressure_active = total_jobs > self.max_concurrency;

        WorkerPoolReport {
            schema_version: WORKER_POOL_SCHEMA_VERSION.to_string(),
            status: if cancelled {
                "cancelled"
            } else if failed_jobs > 0 {
                "completed_with_failures"
            } else {
                "completed"
            }
            .to_string(),
            total_jobs,
            completed_jobs,
            failed_jobs,
            cancelled,
            max_concurrency: self.max_concurrency,
            wave_count,
            duration_ms,
            backpressure_active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_job() -> Job {
        Box::new(|| Ok(()))
    }

    #[test]
    fn worker_pool_executes_all_jobs_when_within_concurrency() {
        let pool = WorkerPool::new(4);
        let jobs: Vec<Job> = (0..4).map(|_| ok_job()).collect();
        let report = pool.execute(jobs);
        assert_eq!(report.status, "completed");
        assert_eq!(report.total_jobs, 4);
        assert_eq!(report.completed_jobs, 4);
        assert_eq!(report.failed_jobs, 0);
        assert!(!report.backpressure_active);
    }

    #[test]
    fn worker_pool_reports_backpressure_when_jobs_exceed_concurrency() {
        let pool = WorkerPool::new(2);
        let jobs: Vec<Job> = (0..5).map(|_| ok_job()).collect();
        let report = pool.execute(jobs);
        assert!(report.backpressure_active);
        assert_eq!(report.max_concurrency, 2);
        assert!(report.wave_count > 1);
    }

    #[test]
    fn worker_pool_handles_empty_job_list() {
        let pool = WorkerPool::new(4);
        let jobs: Vec<Job> = Vec::new();
        let report = pool.execute(jobs);
        assert_eq!(report.status, "completed");
        assert_eq!(report.total_jobs, 0);
        assert_eq!(report.completed_jobs, 0);
    }

    #[test]
    fn worker_pool_reports_failures() {
        let pool = WorkerPool::new(4);
        let jobs: Vec<Job> = (0..3)
            .map(|i| {
                if i == 1 {
                    Box::new(|| Err("intentional failure".to_string())) as Job
                } else {
                    ok_job()
                }
            })
            .collect();
        let report = pool.execute(jobs);
        assert_eq!(report.status, "completed_with_failures");
        assert_eq!(report.completed_jobs, 2);
        assert_eq!(report.failed_jobs, 1);
    }

    #[test]
    fn worker_pool_cancellation_stops_execution() {
        let pool = WorkerPool::new(2);
        pool.cancel();
        let jobs: Vec<Job> = (0..10).map(|_| ok_job()).collect();
        let report = pool.execute(jobs);
        assert!(report.cancelled);
    }

    #[test]
    fn worker_pool_respects_max_concurrency() {
        let pool = WorkerPool::new(3);
        assert_eq!(pool.max_concurrency(), 3);
        let pool2 = WorkerPool::new(0);
        assert!(pool2.max_concurrency() >= 1);
    }

    #[test]
    fn worker_pool_wave_count_matches_chunks() {
        let pool = WorkerPool::new(2);
        let jobs: Vec<Job> = (0..6).map(|_| ok_job()).collect();
        let report = pool.execute(jobs);
        assert_eq!(report.wave_count, 3);
        assert_eq!(report.total_jobs, 6);
        assert_eq!(report.completed_jobs, 6);
    }

    #[test]
    fn worker_pool_schema_version_is_fixed() {
        let pool = WorkerPool::new(1);
        let jobs: Vec<Job> = vec![ok_job()];
        let report = pool.execute(jobs);
        assert_eq!(report.schema_version, "forge.worker_pool.v1");
    }

    #[test]
    fn worker_pool_single_job_completes() {
        let pool = WorkerPool::new(1);
        let jobs: Vec<Job> = vec![ok_job()];
        let report = pool.execute(jobs);
        assert_eq!(report.status, "completed");
        assert_eq!(report.total_jobs, 1);
        assert_eq!(report.completed_jobs, 1);
        assert_eq!(report.wave_count, 1);
    }
}
