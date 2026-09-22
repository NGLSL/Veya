//! Global search over clipboard text, source app, and target app.

use crate::model::ClipboardRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit<'a> {
    pub record: &'a ClipboardRecord,
    pub matched_on: MatchField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchField {
    Content,
    SourceApp,
    TargetApp,
}

/// Case-insensitive substring search across text, source app, and Used-in targets.
pub fn search<'a>(
    records: impl Iterator<Item = &'a ClipboardRecord>,
    query: &str,
) -> Vec<SearchHit<'a>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return records
            .map(|record| SearchHit {
                record,
                matched_on: MatchField::Content,
            })
            .collect();
    }

    let mut hits = Vec::new();
    for record in records {
        if record.content.to_lowercase().contains(&q) {
            hits.push(SearchHit {
                record,
                matched_on: MatchField::Content,
            });
            continue;
        }
        if record.source_app.to_lowercase().contains(&q) {
            hits.push(SearchHit {
                record,
                matched_on: MatchField::SourceApp,
            });
            continue;
        }
        if record
            .pastes
            .iter()
            .any(|p| p.target_app.to_lowercase().contains(&q))
        {
            hits.push(SearchHit {
                record,
                matched_on: MatchField::TargetApp,
            });
        }
    }
    hits
}
