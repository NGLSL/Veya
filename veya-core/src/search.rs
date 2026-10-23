//! Global search over clipboard text, source app, and target app.

use crate::model::ClipboardRecord;

const SEARCH_CHUNK_BYTES: usize = 32 * 1024;
const SHORT_QUERY_BYTES: usize = 1024;

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

/// A normalized query and substring matcher shared across all records in a search.
pub struct PreparedSearch {
    query: String,
    prefix: Vec<usize>,
}

impl PreparedSearch {
    pub fn new(query: &str) -> Self {
        let query = query.trim().to_lowercase();
        let bytes = query.as_bytes();
        // Short queries use std's optimized substring search over bounded
        // chunks. Only long queries need a KMP prefix table.
        let mut prefix = if bytes.len() > SHORT_QUERY_BYTES {
            vec![0; bytes.len()]
        } else {
            Vec::new()
        };
        let mut matched = 0;
        for index in 1..prefix.len() {
            while matched > 0 && bytes[index] != bytes[matched] {
                matched = prefix[matched - 1];
            }
            if bytes[index] == bytes[matched] {
                matched += 1;
            }
            prefix[index] = matched;
        }
        Self { query, prefix }
    }

    fn contains(&self, content: &str) -> bool {
        if self.query.is_empty() {
            return true;
        }
        // ASCII casing preserves UTF-8 byte lengths. Leave other characters
        // untouched only when their lowercase mapping is already identical.
        // Otherwise use str's full Unicode rules, including contextual final
        // sigma and mappings that expand to multiple characters.
        let ascii_casing_only = content.is_ascii()
            || content.chars().all(|ch| {
                if ch.is_ascii() {
                    return true;
                }
                let mut lower = ch.to_lowercase();
                lower.next() == Some(ch) && lower.next().is_none()
            });
        if !ascii_casing_only {
            return content.to_lowercase().contains(&self.query);
        }

        if self.query.len() <= SHORT_QUERY_BYTES {
            return self.contains_in_chunks(content);
        }

        // KMP scans even repetitive large text in linear time without a
        // lowercase copy. The prefix table depends only on the query length.
        let query = self.query.as_bytes();
        let mut matched = 0;
        for byte in content.bytes().map(|byte| byte.to_ascii_lowercase()) {
            while matched > 0 && byte != query[matched] {
                matched = self.prefix[matched - 1];
            }
            if byte == query[matched] {
                matched += 1;
                if matched == query.len() {
                    return true;
                }
            }
        }
        false
    }

    fn contains_in_chunks(&self, content: &str) -> bool {
        // Reuse one fixed-size buffer instead of copying the complete text.
        // A short query limits overlap to about 3% of each full chunk.
        let mut buffer = [0u8; SEARCH_CHUNK_BYTES];
        let mut start = 0;
        while start < content.len() {
            let mut end = (start + SEARCH_CHUNK_BYTES).min(content.len());
            while !content.is_char_boundary(end) {
                end -= 1;
            }
            let chunk = &mut buffer[..end - start];
            chunk.copy_from_slice(&content.as_bytes()[start..end]);
            chunk.make_ascii_lowercase();
            // Both boundaries came from the original UTF-8 string; ASCII
            // lowercase cannot invalidate its encoding.
            if std::str::from_utf8(chunk)
                .expect("ASCII lowercase preserves UTF-8")
                .contains(&self.query)
            {
                return true;
            }
            if end == content.len() {
                break;
            }
            // Keep every possible cross-boundary match, rounding outward
            // when a query-sized byte overlap lands inside a UTF-8 character.
            start = end - (self.query.len() - 1);
            while !content.is_char_boundary(start) {
                start -= 1;
            }
        }
        false
    }

    /// Return the first matching field, preserving content/source/target priority.
    pub fn match_field<'a>(
        &self,
        content: &str,
        source_app: &str,
        target_apps: impl Iterator<Item = &'a str>,
    ) -> Option<MatchField> {
        if self.contains(content) {
            return Some(MatchField::Content);
        }
        if self.contains(source_app) {
            return Some(MatchField::SourceApp);
        }
        if target_apps.into_iter().any(|target| self.contains(target)) {
            return Some(MatchField::TargetApp);
        }
        None
    }
}

/// Which field matched a query. UI and `search` share this.
pub fn match_field<'a>(
    content: &str,
    source_app: &str,
    target_apps: impl Iterator<Item = &'a str>,
    query: &str,
) -> Option<MatchField> {
    PreparedSearch::new(query).match_field(content, source_app, target_apps)
}

/// Case-insensitive substring search across text, source app, and Used-in targets.
pub fn search<'a>(
    records: impl Iterator<Item = &'a ClipboardRecord>,
    query: &str,
) -> Vec<SearchHit<'a>> {
    let query = PreparedSearch::new(query);
    records
        .filter_map(|record| {
            let matched_on = query.match_field(
                &record.content,
                &record.source_app,
                record.pastes.iter().map(|p| p.target_app.as_str()),
            )?;
            Some(SearchHit { record, matched_on })
        })
        .collect()
}
