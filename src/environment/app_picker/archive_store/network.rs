/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::{
    ArchiveFile, CatalogItem, CatalogPage, CatalogScope, CatalogSort, DownloadOutcome, ItemDetails,
};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const COLLECTION_ID: &str = "ipaarchive";
// Archive does not mark multi-file IPA bundles as collections. Ordinary
// one-app items usually contain five to seven files including metadata.
const COLLECTION_MIN_FILES: usize = 8;
const MAX_JSON_RESPONSE_SIZE: usize = 64 * 1024 * 1024;
const MAX_ARTWORK_RESPONSE_SIZE: usize = 8 * 1024 * 1024;

pub(super) fn fetch_catalog(
    scope: CatalogScope,
    query: &str,
    page: usize,
    rows: usize,
    sort: CatalogSort,
    fuzzy: bool,
) -> Result<CatalogPage, String> {
    let page = page.max(1);
    let rows = rows.clamp(1, 50);
    let json = fetch_json(&catalog_url(scope, query, page, rows, sort, fuzzy))?;
    parse_catalog(&json, page)
}

fn catalog_url(
    scope: CatalogScope,
    query: &str,
    page: usize,
    rows: usize,
    sort: CatalogSort,
    fuzzy: bool,
) -> String {
    let mut archive_query = format!("collection:{COLLECTION_ID}");
    if scope == CatalogScope::Collections {
        archive_query.push_str(&format!(" AND files_count:[{COLLECTION_MIN_FILES} TO *]"));
    }
    let terms: Vec<String> = query
        .split_whitespace()
        .take(8)
        .map(|term| term.chars().take(80).collect::<String>())
        .map(|term| escape_lucene_term(&term))
        .filter(|term| !term.is_empty())
        .collect();
    if fuzzy && !terms.is_empty() {
        let clauses: Vec<String> = terms
            .iter()
            .map(|term| {
                format!(
                    "(title:{term}~ OR identifier:{term}~ OR creator:{term}~ OR title:{term}* OR identifier:{term}*)"
                )
            })
            .collect();
        let mut combined = format!("({})", clauses.join(" AND "));
        if terms.len() >= 2 {
            let acronym: String = terms
                .iter()
                .filter_map(|term| term.chars().find(|character| character.is_alphanumeric()))
                .collect::<String>()
                .to_lowercase();
            if acronym.chars().count() >= 2 {
                combined = format!(
                    "({combined} OR identifier:{acronym}* OR title:{acronym}*)"
                );
            }
        }
        archive_query.push_str(&format!(" AND {combined}"));
    } else {
        for term in terms {
            archive_query.push_str(&format!(
                " AND (title:{term}* OR identifier:{term}* OR creator:{term}*)"
            ));
        }
    }
    let fields = [
        "identifier",
        "title",
        "date",
        "publicdate",
        "year",
        "creator",
        "version",
        "files_count",
    ];
    let mut url = format!(
        "https://archive.org/advancedsearch.php?q={}",
        percent_encode(&archive_query)
    );
    for field in fields {
        url.push_str("&fl%5B%5D=");
        url.push_str(field);
    }
    url.push_str(&format!(
        "&sort%5B%5D={}&rows={rows}&page={page}&output=json",
        percent_encode(sort.query_value())
    ));
    url
}

fn parse_catalog(json: &Value, requested_page: usize) -> Result<CatalogPage, String> {
    let response = json
        .get("response")
        .ok_or_else(|| "The catalog response has no response object.".to_string())?;
    let num_found = response
        .get("numFound")
        .and_then(json_u64)
        .ok_or_else(|| "The catalog response has no result count.".to_string())?;
    let documents = response
        .get("docs")
        .and_then(Value::as_array)
        .ok_or_else(|| "The catalog response has no document list.".to_string())?;
    let mut items = Vec::new();
    for document in documents {
        let Some(identifier) = document.get("identifier").and_then(json_text) else {
            continue;
        };
        if identifier.is_empty() {
            continue;
        }
        let title = document
            .get("title")
            .and_then(json_text)
            .unwrap_or_else(|| identifier.clone());
        let version = infer_version(
            &title,
            &identifier,
            document.get("version").and_then(json_text),
        );
        let date = document
            .get("date")
            .and_then(json_text)
            .or_else(|| document.get("publicdate").and_then(json_text))
            .or_else(|| document.get("year").and_then(json_text))
            .map(|date| short_date(&date));
        let creator = document.get("creator").and_then(json_text);
        let files_count = document.get("files_count").and_then(json_u64);
        items.push(CatalogItem {
            identifier,
            title,
            version,
            date,
            creator,
            files_count,
        });
    }
    Ok(CatalogPage {
        items,
        num_found,
        page: requested_page,
    })
}

pub(super) fn fetch_artwork(identifier: &str) -> Result<Vec<u8>, String> {
    let url = format!(
        "https://archive.org/services/img/{}",
        percent_encode(identifier)
    );
    let response = archive_agent()
        .get(&url)
        .set(
            "User-Agent",
            concat!("HyperHLE-Reborn/", env!("CARGO_PKG_VERSION")),
        )
        .call();
    let response = check_response(response)?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take((MAX_ARTWORK_RESPONSE_SIZE + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|error| format!("Could not read the archive artwork: {error}"))?;
    if body.is_empty() || body.len() > MAX_ARTWORK_RESPONSE_SIZE {
        return Err("The archive artwork is empty or too large.".to_string());
    }
    Ok(body)
}

pub(super) fn fetch_details(identifier: &str) -> Result<ItemDetails, String> {
    let url = format!(
        "https://archive.org/metadata/{}",
        percent_encode(identifier)
    );
    let json = fetch_json(&url)?;
    parse_details(&json, identifier)
}

fn parse_details(json: &Value, requested_identifier: &str) -> Result<ItemDetails, String> {
    let metadata = json.get("metadata");
    let identifier = metadata
        .and_then(|metadata| metadata.get("identifier"))
        .and_then(json_text)
        .unwrap_or_else(|| requested_identifier.to_string());
    let title = metadata
        .and_then(|metadata| metadata.get("title"))
        .and_then(json_text)
        .unwrap_or_else(|| identifier.clone());
    let item_version = metadata
        .and_then(|metadata| metadata.get("version"))
        .and_then(json_text)
        .or_else(|| infer_version(&title, &identifier, None));
    let creator = metadata
        .and_then(|metadata| metadata.get("creator"))
        .and_then(json_text);
    let date = metadata
        .and_then(|metadata| metadata.get("date"))
        .and_then(json_text)
        .or_else(|| {
            metadata
                .and_then(|metadata| metadata.get("publicdate"))
                .and_then(json_text)
        })
        .map(|date| short_date(&date));
    let description = metadata
        .and_then(|metadata| metadata.get("description"))
        .and_then(json_text)
        .map(|description| plain_text(&description))
        .filter(|description| !description.is_empty());

    let empty_files = Vec::new();
    let files = json
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&empty_files);
    let mut ipa_files = Vec::new();
    for file in files {
        let Some(name) = file.get("name").and_then(json_text) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".ipa") || json_is_private(file.get("private")) {
            continue;
        }
        let size = file.get("size").and_then(json_u64);
        let file_version = extract_version(&name).or_else(|| item_version.clone());
        let version = infer_version(&name, &identifier, file_version);
        ipa_files.push(ArchiveFile { name, size, version });
    }
    ipa_files.sort_by_key(|file| file.name.to_ascii_lowercase());
    Ok(ItemDetails {
        identifier,
        title,
        version: item_version,
        creator,
        date,
        description,
        files: ipa_files,
    })
}

fn plain_text(value: &str) -> String {
    let mut without_tags = String::with_capacity(value.len());
    let mut inside_tag = false;
    for character in value.chars() {
        match character {
            '<' => {
                inside_tag = true;
                without_tags.push(' ');
            }
            '>' => {
                inside_tag = false;
                without_tags.push(' ');
            }
            _ if !inside_tag => without_tags.push(character),
            _ => (),
        }
    }
    without_tags = without_tags
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    without_tags.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn download_ipa(
    apps_dir: &Path,
    identifier: &str,
    file: &ArchiveFile,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<DownloadOutcome, String> {
    std::fs::create_dir_all(apps_dir)
        .map_err(|error| format!("Could not create the apps directory: {error}"))?;
    let (destination, already_present) =
        choose_destination(apps_dir, identifier, &file.name, file.size);
    if already_present {
        return Ok(DownloadOutcome {
            path: destination,
            already_present: true,
        });
    }

    let partial = destination.with_extension("ipa.part");
    if partial.exists() {
        std::fs::remove_file(&partial)
            .map_err(|error| format!("Could not remove an old partial download: {error}"))?;
    }

    let result = (|| -> Result<(), String> {
        if cancel.load(Ordering::Acquire) {
            return Err("Download cancelled.".to_string());
        }
        let url = download_url(identifier, &file.name);
        let response = archive_agent()
            .get(&url)
            .set(
                "User-Agent",
                concat!("HyperHLE-Reborn/", env!("CARGO_PKG_VERSION")),
            )
            .call();
        let response = check_response(response)?;
        let total = response
            .header("Content-Length")
            .and_then(|length| length.parse::<u64>().ok())
            .or(file.size);
        let mut reader = response.into_reader();
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|error| format!("Could not create {}: {error}", partial.display()))?;
        let mut buffer = [0u8; 64 * 1024];
        let mut downloaded = 0u64;
        let mut last_report = std::time::Instant::now();
        let mut last_reported_bytes = 0u64;
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err("Download cancelled.".to_string());
            }
            let count = reader
                .read(&mut buffer)
                .map_err(|error| format!("The download was interrupted: {error}"))?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| format!("Could not save the IPA: {error}"))?;
            downloaded += count as u64;
            if downloaded - last_reported_bytes >= 1024 * 1024
                || last_report.elapsed() >= Duration::from_millis(150)
            {
                progress(downloaded, total);
                last_report = std::time::Instant::now();
                last_reported_bytes = downloaded;
            }
        }
        output
            .sync_all()
            .map_err(|error| format!("Could not finish saving the IPA: {error}"))?;
        drop(output);
        if let Some(expected) = file.size.filter(|size| *size > 0) {
            if downloaded != expected {
                return Err(format!(
                    "The download is incomplete: expected {expected} bytes, received {downloaded}."
                ));
            }
        }
        let package = std::fs::File::open(&partial)
            .map_err(|error| format!("Could not verify the downloaded IPA: {error}"))?;
        drop(zip::ZipArchive::new(package).map_err(|error| {
            format!("The downloaded file is not a valid IPA/ZIP archive: {error}")
        })?);
        progress(downloaded, Some(downloaded));
        if cancel.load(Ordering::Acquire) {
            return Err("Download cancelled.".to_string());
        }
        std::fs::rename(&partial, &destination).map_err(|error| {
            format!(
                "Could not move the completed IPA into {}: {error}",
                apps_dir.display()
            )
        })?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    Ok(DownloadOutcome {
        path: destination,
        already_present: false,
    })
}

fn choose_destination(
    apps_dir: &Path,
    identifier: &str,
    archive_name: &str,
    expected_size: Option<u64>,
) -> (PathBuf, bool) {
    let original_name = safe_file_component(display_file_name(archive_name));
    let original = apps_dir.join(&original_name);
    if existing_file_matches(&original, expected_size) {
        return (original, true);
    }
    if !original.exists() {
        return (original, false);
    }

    let stem = original_name
        .get(..original_name.len().saturating_sub(4))
        .unwrap_or("app");
    let suffix = safe_file_component(identifier);
    for counter in 1..1000usize {
        let suffix = if counter == 1 {
            suffix.clone()
        } else {
            format!("{suffix}-{counter}")
        };
        let name: String = format!(
            "{} [{}].ipa",
            stem.chars().take(100).collect::<String>(),
            suffix.chars().take(48).collect::<String>()
        );
        let candidate = apps_dir.join(name);
        if existing_file_matches(&candidate, expected_size) {
            return (candidate, true);
        }
        if !candidate.exists() {
            return (candidate, false);
        }
    }
    (original, false)
}

fn existing_file_matches(path: &Path, expected_size: Option<u64>) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    metadata.is_file()
        && expected_size
            .filter(|size| *size > 0)
            .is_none_or(|size| metadata.len() == size)
}

fn safe_file_component(name: &str) -> String {
    let mut result: String = name
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                '_'
            } else {
                character
            }
        })
        .take(160)
        .collect();
    result = result.trim().trim_matches('.').to_string();
    if result.is_empty() || result == "." || result == ".." {
        "app.ipa".to_string()
    } else {
        result
    }
}

fn download_url(identifier: &str, file_name: &str) -> String {
    let encoded_path = file_name
        .split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "https://archive.org/download/{}/{}",
        percent_encode(identifier),
        encoded_path
    )
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn escape_lucene_term(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if "+-=&|><!(){}[]^\"~*?:\\/".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn archive_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .timeout_write(Duration::from_secs(60))
        .redirects(10)
        .build()
}

fn check_response(result: Result<ureq::Response, ureq::Error>) -> Result<ureq::Response, String> {
    match result {
        Ok(response) => Ok(response),
        Err(ureq::Error::Status(status, response)) => Err(format!(
            "Internet Archive returned HTTP {} {}.",
            status,
            response.status_text()
        )),
        Err(ureq::Error::Transport(error)) => Err(format!(
            "Could not connect to Internet Archive: {error}. Check the network connection and try again."
        )),
    }
}

fn fetch_json(url: &str) -> Result<Value, String> {
    let response = archive_agent()
        .get(url)
        .set("Accept", "application/json")
        .set(
            "User-Agent",
            concat!("HyperHLE-Reborn/", env!("CARGO_PKG_VERSION")),
        )
        .call();
    let response = check_response(response)?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take((MAX_JSON_RESPONSE_SIZE + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|error| format!("Could not read Internet Archive's response: {error}"))?;
    if body.len() > MAX_JSON_RESPONSE_SIZE {
        return Err("Internet Archive's metadata response is too large.".to_string());
    }
    serde_json::from_slice(&body)
        .map_err(|error| format!("Internet Archive returned invalid JSON: {error}"))
}

fn json_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let text = text.trim();
            (!text.is_empty()).then_some(text.to_string())
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Array(values) => {
            let text = values
                .iter()
                .filter_map(json_text)
                .collect::<Vec<_>>()
                .join(", ");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn json_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn json_is_private(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => value.eq_ignore_ascii_case("true") || value == "1",
        Some(Value::Number(value)) => value.as_u64() == Some(1),
        _ => false,
    }
}

fn short_date(value: &str) -> String {
    value.chars().take(10).collect()
}

pub(super) fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= GIB {
        format!("{:.2} GB", bytes_f / GIB)
    } else if bytes_f >= MIB {
        format!("{:.1} MB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.0} KB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

pub(super) fn download_key(identifier: &str, file_name: &str) -> String {
    format!("{identifier}\n{file_name}")
}

pub(super) fn display_file_name(name: &str) -> &str {
    name.rsplit(|character| character == '/' || character == '\\')
        .next()
        .unwrap_or(name)
}

fn infer_version(title: &str, identifier: &str, explicit: Option<String>) -> Option<String> {
    explicit
        .and_then(|version| {
            let version = version.trim().trim_start_matches(['v', 'V']).to_string();
            (!version.is_empty()).then_some(version)
        })
        .or_else(|| extract_version(title))
        .or_else(|| extract_version(identifier))
}

fn extract_version(text: &str) -> Option<String> {
    let text = text
        .get(text.len().saturating_sub(4)..)
        .filter(|extension| extension.eq_ignore_ascii_case(".ipa"))
        .map(|_| &text[..text.len() - 4])
        .unwrap_or(text);
    for token in text.split(|character: char| {
        character.is_whitespace() || "(),[]{}<>".contains(character)
    }) {
        let token = token.trim_matches(|character: char| {
            matches!(character, ':' | ';' | '!' | '?' | '"' | '\'')
        });
        let explicit_v = token.as_bytes().get(..2).is_some_and(|prefix| {
            matches!(prefix[0], b'v' | b'V') && prefix[1].is_ascii_digit()
        });
        for (start, character) in token.char_indices() {
            if !character.is_ascii_digit() {
                continue;
            }
            let mut end = start + character.len_utf8();
            for (index, next) in token.char_indices().skip_while(|(index, _)| *index <= start) {
                if next.is_ascii_digit() || next == '.' {
                    end = index + next.len_utf8();
                } else {
                    break;
                }
            }
            let candidate = token[start..end].trim_end_matches('.');
            let dotted = candidate.contains('.');
            if !dotted && !(explicit_v && start == 1) {
                continue;
            }
            if candidate.len() <= 40
                && candidate
                    .split('.')
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn catalog_reads_versions_dates_and_creators() {
        let json = json!({"response": {"numFound": 2, "docs": [
            {"identifier": "example-game", "title": "Example Game 1.2.3", "creator": "Developer", "publicdate": "2024-03-02T10:00:00Z"},
            {"identifier": "other", "title": "Other", "version": "v4.5"}
        ]}});
        let page = parse_catalog(&json, 3).unwrap();
        assert_eq!(page.num_found, 2);
        assert_eq!(page.page, 3);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].version.as_deref(), Some("1.2.3"));
        assert_eq!(page.items[0].creator.as_deref(), Some("Developer"));
        assert_eq!(page.items[0].date.as_deref(), Some("2024-03-02"));
        assert_eq!(page.items[1].version.as_deref(), Some("4.5"));
    }

    #[test]
    fn details_expose_public_ipa_files_and_clean_metadata() {
        let json = json!({
            "metadata": {
                "identifier": "item",
                "title": "Game 2.0",
                "creator": "Studio",
                "description": "<p>A &amp; B<br>game</p>"
            },
            "files": [
                {"name": "Game 2.0.ipa", "size": "1024"},
                {"name": "Other.IPA", "size": 2048},
                {"name": "notes.txt"},
                {"name": "secret.ipa", "private": "true"}
            ]
        });
        let details = parse_details(&json, "item").unwrap();
        assert_eq!(details.files.len(), 2);
        assert_eq!(details.files[0].size, Some(1024));
        assert_eq!(details.files[1].size, Some(2048));
        assert_eq!(details.files[0].version.as_deref(), Some("2.0"));
        assert_eq!(details.creator.as_deref(), Some("Studio"));
        assert_eq!(details.description.as_deref(), Some("A & B game"));
    }

    #[test]
    fn version_inference_handles_common_archive_titles() {
        assert_eq!(extract_version("Video Star 11.2.8 1675756235"), Some("11.2.8".to_string()));
        assert_eq!(extract_version("[ Temple Run ][ 1.6.1 ]"), Some("1.6.1".to_string()));
        assert_eq!(extract_version("Talking Tom v2.7 IPA"), Some("2.7".to_string()));
        assert_eq!(extract_version("Game-1.2.3.ipa"), Some("1.2.3".to_string()));
        assert_eq!(extract_version("Minecraft Pocket Edition"), None);
        assert_eq!(extract_version("é1"), None);
    }

    #[test]
    fn catalog_urls_support_scopes_fuzzy_search_and_sorting() {
        let url = catalog_url(
            CatalogScope::All,
            "need:for speed*",
            4,
            25,
            CatalogSort::Newest,
            false,
        );
        assert!(url.contains("page=4"));
        assert!(url.contains("rows=25"));
        assert!(url.contains("need%5C%3Afor"));
        assert!(url.contains("speed%5C%2A"));
        assert!(url.contains("creator%3A"));
        assert!(url.contains("publicdate%20desc"));
        let url = catalog_url(
            CatalogScope::All,
            "",
            1,
            20,
            CatalogSort::Title,
            false,
        );
        assert!(url.contains("collection%3Aipaarchive"));
        assert!(url.contains("title%20asc"));
        assert!(!url.contains("mediatype"));

        let url = catalog_url(
            CatalogScope::Collections,
            "Gangstar Vegas",
            1,
            20,
            CatalogSort::Popular,
            true,
        );
        assert!(url.contains("files_count%3A%5B8%20TO%20%2A%5D"));
        assert!(url.contains("title%3AGangstar~"));
        assert!(url.contains("identifier%3Agv%2A"));
        assert!(url.contains("week%20desc"));
    }

    #[test]
    fn archive_urls_and_file_names_are_safe() {
        assert_eq!(
            download_url("item id", "folder/Game & More.ipa"),
            "https://archive.org/download/item%20id/folder/Game%20%26%20More.ipa"
        );
        assert_eq!(safe_file_component(display_file_name("../Game.ipa")), "Game.ipa");
        assert_eq!(safe_file_component(".."), "app.ipa");
        assert_eq!(display_file_name("folder/subfolder/Game.ipa"), "Game.ipa");
    }
}
