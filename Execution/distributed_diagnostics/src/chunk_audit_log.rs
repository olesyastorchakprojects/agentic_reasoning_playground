use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::shared_types::{
    IncidentEvidenceChunk, IncidentEvidenceRetrievalOutput, PromptIncidentEvidenceChunk,
};

#[derive(Serialize)]
struct AuditEntry<'a> {
    ts: String,
    run_id: &'a str,
    iteration_id: &'a str,
    query: &'a str,
    retrieved: RetrievedSection<'a>,
    selected: Vec<SelectedChunk>,
}

#[derive(Serialize)]
struct RetrievedSection<'a> {
    primary: Vec<RetrievedChunk<'a>>,
    alternatives: Vec<RetrievedChunk<'a>>,
}

#[derive(Serialize)]
struct RetrievedChunk<'a> {
    chunk_id: &'a str,
    case_id: &'a str,
    score: f32,
    tags: &'a Vec<String>,
    text: &'a str,
}

#[derive(Serialize)]
struct SelectedChunk {
    chunk_id: String,
    case_id: String,
    score: f32,
    role: String,
    tags: Vec<String>,
}

struct Inner {
    writer: Mutex<BufWriter<File>>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct ChunkAuditLog {
    inner: Arc<Inner>,
}

impl ChunkAuditLog {
    pub fn open(path: &str) -> std::io::Result<Self> {
        let file = OpenOptions::new().append(true).create(true).open(path)?;
        Ok(Self {
            inner: Arc::new(Inner {
                writer: Mutex::new(BufWriter::new(file)),
            }),
        })
    }

    pub fn append(
        &self,
        run_id: &str,
        iteration_id: &str,
        query: &str,
        retrieved: &IncidentEvidenceRetrievalOutput,
        selected: &[PromptIncidentEvidenceChunk],
    ) {
        let entry = AuditEntry {
            ts: chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            run_id,
            iteration_id,
            query,
            retrieved: RetrievedSection {
                primary: retrieved.primary_chunks.iter().map(to_retrieved).collect(),
                alternatives: retrieved.alternative_chunks.iter().map(to_retrieved).collect(),
            },
            selected: selected
                .iter()
                .map(|c| SelectedChunk {
                    chunk_id: c.chunk_id.clone(),
                    case_id: c.case_id.clone(),
                    score: c.score,
                    role: format!("{:?}", c.role),
                    tags: c.chunk_tags.iter().map(|t| t.to_string()).collect(),
                })
                .collect(),
        };

        if let Ok(line) = serde_json::to_string(&entry) {
            if let Ok(mut w) = self.inner.writer.lock() {
                let _ = writeln!(w, "{line}");
                let _ = w.flush();
            }
        } else {
            tracing::warn!("chunk_audit_log: failed to serialize audit entry");
        }
    }
}

fn to_retrieved(c: &IncidentEvidenceChunk) -> RetrievedChunk<'_> {
    RetrievedChunk {
        chunk_id: &c.chunk_id,
        case_id: &c.case_id,
        score: c.score,
        tags: &c.chunk_tags,
        text: &c.text,
    }
}
