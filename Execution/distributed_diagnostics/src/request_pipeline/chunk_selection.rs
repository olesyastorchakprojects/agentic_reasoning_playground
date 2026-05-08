use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::config::ChunkRolePackingSettings;
use crate::shared_types::{
    IncidentCard, IncidentChunkTag, IncidentEvidenceChunk, PromptEvidenceRole,
    PromptIncidentEvidenceChunk, PromptTheoryEvidenceChunk, TheoryEvidenceChunk,
};

// ─── Ranked chunk helper ──────────────────────────────────────────────────────

pub(crate) struct RankedChunk<'a> {
    pub tag_bucket_index: usize,
    pub score: f32,
    pub source_index: usize,
    pub chunk: &'a IncidentEvidenceChunk,
}

// ─── Core helpers ─────────────────────────────────────────────────────────────

pub(crate) fn compute_eligible_incident_chunks<'a>(
    chunks: &'a [IncidentEvidenceChunk],
    role_settings: &ChunkRolePackingSettings,
) -> Vec<RankedChunk<'a>> {
    let fallback_bucket = role_settings.tag_priority.len();
    let mut ranked: Vec<RankedChunk<'a>> = Vec::new();

    for (source_index, chunk) in chunks.iter().enumerate() {
        let mut best_tag_index: Option<usize> = None;
        for raw_tag in &chunk.chunk_tags {
            if let Ok(tag) = IncidentChunkTag::from_str(raw_tag) {
                if let Some(idx) = role_settings.tag_priority.iter().position(|t| *t == tag) {
                    best_tag_index = Some(match best_tag_index {
                        Some(prev) => prev.min(idx),
                        None => idx,
                    });
                }
            }
        }

        let tag_bucket_index = match best_tag_index {
            Some(idx) => idx,
            None if role_settings.fallback_to_any_chunk => fallback_bucket,
            None => continue,
        };

        ranked.push(RankedChunk {
            tag_bucket_index,
            score: chunk.score,
            source_index,
            chunk,
        });
    }

    ranked.sort_by(|a, b| {
        a.tag_bucket_index
            .cmp(&b.tag_bucket_index)
            .then_with(|| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.source_index.cmp(&b.source_index))
    });

    ranked
}

pub(crate) fn make_prompt_incident_chunk(
    source: &IncidentEvidenceChunk,
    role: PromptEvidenceRole,
) -> PromptIncidentEvidenceChunk {
    let chunk_tags = source
        .chunk_tags
        .iter()
        .filter_map(|t| IncidentChunkTag::from_str(t).ok())
        .collect();
    PromptIncidentEvidenceChunk {
        role,
        chunk_id: source.chunk_id.clone(),
        case_id: source.case_id.clone(),
        score: source.score,
        chunk_tags,
        text: source.text.clone(),
    }
}

pub(crate) fn select_alternative_context_chunks(
    alt_chunks: &[IncidentEvidenceChunk],
    role_settings: &ChunkRolePackingSettings,
    alt_cards: &[IncidentCard],
    already_selected: &HashSet<String>,
) -> Vec<PromptIncidentEvidenceChunk> {
    if role_settings.limit == 0 {
        return vec![];
    }

    let per_case_limit = role_settings.per_case_limit.unwrap_or(usize::MAX);
    let fallback_bucket = role_settings.tag_priority.len();

    let case_order: Vec<String> = if alt_cards.is_empty() {
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        for chunk in alt_chunks {
            if seen.insert(chunk.case_id.clone()) {
                order.push(chunk.case_id.clone());
            }
        }
        order
    } else {
        alt_cards.iter().map(|c| c.case_id.clone()).collect()
    };

    let mut case_pools: HashMap<String, Vec<(usize, f32, usize, &IncidentEvidenceChunk)>> =
        HashMap::new();

    for (src_idx, chunk) in alt_chunks.iter().enumerate() {
        if already_selected.contains(&chunk.chunk_id) {
            continue;
        }

        let mut best_tag_index: Option<usize> = None;
        for raw_tag in &chunk.chunk_tags {
            if let Ok(tag) = IncidentChunkTag::from_str(raw_tag) {
                if let Some(idx) = role_settings.tag_priority.iter().position(|t| *t == tag) {
                    best_tag_index = Some(match best_tag_index {
                        Some(prev) => prev.min(idx),
                        None => idx,
                    });
                }
            }
        }

        let tag_bucket = match best_tag_index {
            Some(idx) => idx,
            None if role_settings.fallback_to_any_chunk => fallback_bucket,
            None => continue,
        };

        case_pools
            .entry(chunk.case_id.clone())
            .or_default()
            .push((tag_bucket, chunk.score, src_idx, chunk));
    }

    for pool in case_pools.values_mut() {
        pool.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.2.cmp(&b.2))
        });
    }

    let mut selected: Vec<PromptIncidentEvidenceChunk> = Vec::new();
    let mut cursors: HashMap<String, usize> = HashMap::new();
    let mut counts: HashMap<String, usize> = HashMap::new();

    loop {
        let prev_len = selected.len();

        for case_id in &case_order {
            if selected.len() >= role_settings.limit {
                break;
            }
            let pool = match case_pools.get(case_id) {
                Some(p) => p,
                None => continue,
            };
            let count = *counts.get(case_id).unwrap_or(&0);
            if count >= per_case_limit {
                continue;
            }
            let cursor = cursors.entry(case_id.clone()).or_insert(0);
            if *cursor >= pool.len() {
                continue;
            }
            let (_, _, _, chunk) = pool[*cursor];
            *cursor += 1;
            *counts.entry(case_id.clone()).or_insert(0) += 1;
            selected.push(make_prompt_incident_chunk(chunk, PromptEvidenceRole::AlternativeContext));
        }

        if selected.len() == prev_len || selected.len() >= role_settings.limit {
            break;
        }
    }

    selected
}

pub(crate) fn select_theory_chunks(
    chunks: &[TheoryEvidenceChunk],
    limit: usize,
) -> Vec<PromptTheoryEvidenceChunk> {
    chunks
        .iter()
        .take(limit)
        .map(|c| PromptTheoryEvidenceChunk {
            role: PromptEvidenceRole::MechanismExplanation,
            chunk_id: c.chunk_id.clone(),
            score: c.score,
            text: c.text.clone(),
        })
        .collect()
}
