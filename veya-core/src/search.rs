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

/// Which field matched a non-empty query. UI and `search` share this.
pub fn match_field<'a>(
    content: &str,
    source_app: &str,
    target_apps: impl Iterator<Item = &'a str>,
    query: &str,
) -> Option<MatchField> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(MatchField::Content);
    }
    if content.to_lowercase().contains(&q) {
        return Some(MatchField::Content);
    }
    if source_app.to_lowercase().contains(&q) {
        return Some(MatchField::SourceApp);
    }
    if target_apps
        .into_iter()
        .any(|t| t.to_lowercase().contains(&q))
    {
        return Some(MatchField::TargetApp);
    }
    None
}

/// Case-insensitive substring search across text, source app, and Used-in targets.
pub fn search<'a>(
    records: impl Iterator<Item = &'a ClipboardRecord>,
    query: &str,
) -> Vec<SearchHit<'a>> {
    records
        .filter_map(|record| {
            let matched_on = match_field(
                &record.content,
                &record.source_app,
                record.pastes.iter().map(|p| p.target_app.as_str()),
                query,
            )?;
            Some(SearchHit { record, matched_on })
        })
        .collect()
}
