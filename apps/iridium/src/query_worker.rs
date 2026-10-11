//! Cancellable process isolation for interactive jq, including non-yielding filters.
use iridium_query::{MAX_FILTER_BYTES, MAX_SOURCE_BYTES, Query, QueryResult};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

pub const WORKER_FLAG: &str = "--internal-jq-worker";
const OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(3);

pub struct Snapshot {
    file: NamedTempFile,
}
impl Snapshot {
    pub fn new(source: &str) -> Result<Self, String> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err("Live jq currently accepts up to 32 MiB of source".into());
        }
        let mut file = NamedTempFile::new().map_err(|error| error.to_string())?;
        file.write_all(source.as_bytes())
            .map_err(|error| error.to_string())?;
        Ok(Self { file })
    }
}

pub struct Job {
    child: Child,
    output: NamedTempFile,
    deadline: Instant,
    reaped: bool,
}
impl Job {
    pub fn spawn(snapshot: &Snapshot, query: &Query) -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        Self::spawn_with(&executable, snapshot, query)
    }

    fn spawn_with(executable: &Path, snapshot: &Snapshot, query: &Query) -> Result<Self, String> {
        let mut input = NamedTempFile::new().map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut input, query).map_err(|error| error.to_string())?;
        input
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        let output = NamedTempFile::new().map_err(|error| error.to_string())?;
        let child = Command::new(executable)
            .arg(WORKER_FLAG)
            .arg(snapshot.file.path())
            .stdin(input.reopen().map_err(|error| error.to_string())?)
            .stdout(output.reopen().map_err(|error| error.to_string())?)
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            child,
            output,
            deadline: Instant::now() + DEADLINE,
            reaped: false,
        })
    }

    pub fn poll(&mut self) -> Option<QueryResult> {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                self.reaped = true;
                if !status.success() {
                    return Some(Err(format!("jq worker stopped: {status}")));
                }
                Some(self.read_result())
            },
            Ok(None) if Instant::now() >= self.deadline => Some(self.cancel().and_then(|()| {
                Err("jq exceeded the 3-second query deadline; narrow the filter".into())
            })),
            Ok(None) => None,
            Err(error) => Some(Err(format!("Cannot inspect jq worker: {error}"))),
        }
    }

    fn read_result(&self) -> QueryResult {
        let file = self.output.reopen().map_err(|error| error.to_string())?;
        if file.metadata().map_err(|error| error.to_string())?.len() > OUTPUT_LIMIT {
            return Err("jq worker output exceeded its size limit".into());
        }
        let mut bytes = Vec::new();
        file.take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > OUTPUT_LIMIT {
            return Err("jq worker output exceeded its size limit".into());
        }
        serde_json::from_slice::<QueryResult>(&bytes)
            .map_err(|error| format!("Invalid jq worker response: {error}"))?
    }

    pub fn cancel(&mut self) -> Result<(), String> {
        if self.reaped {
            return Ok(());
        }
        if self
            .child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_none()
        {
            self.child
                .kill()
                .map_err(|error| format!("Cannot cancel jq worker: {error}"))?;
        }
        self.child
            .wait()
            .map_err(|error| format!("Cannot reap jq worker: {error}"))?;
        self.reaped = true;
        Ok(())
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if let Err(error) = self.cancel() {
            tracing::error!(error, "jq worker cleanup failed");
        }
    }
}

pub fn run(path: &Path) -> crate::run::ExitStatus {
    let response = evaluate(path);
    match serde_json::to_writer(std::io::stdout().lock(), &response) {
        Ok(()) => crate::run::ExitStatus::Success,
        Err(error) => {
            if let Err(report) =
                writeln!(std::io::stderr().lock(), "jq worker output failed: {error}")
            {
                tracing::error!(error = %report, "jq worker diagnostic failed");
            }
            crate::run::ExitStatus::Failure
        },
    }
}

fn evaluate(path: &Path) -> QueryResult {
    let mut request = String::new();
    std::io::stdin()
        .take((MAX_FILTER_BYTES * 6 + 1024) as u64)
        .read_to_string(&mut request)
        .map_err(|error| error.to_string())?;
    let query: Query = serde_json::from_str(&request)
        .map_err(|error| format!("Invalid jq worker request: {error}"))?;
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > MAX_SOURCE_BYTES as u64 {
        return Err("JSON query source exceeds 32 MiB".into());
    }
    let mut source = String::new();
    file.take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_string(&mut source)
        .map_err(|error| error.to_string())?;
    iridium_query::select(&source, &query)
}
