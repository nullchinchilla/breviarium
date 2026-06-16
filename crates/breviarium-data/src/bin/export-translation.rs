use std::env;
use std::fs;
use std::path::PathBuf;

use breviarium_data::{Breviarium, ContentNode, TextRole};
use serde::Serialize;

fn main() {
    if let Err(error) = run() {
        eprintln!("export-translation: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/to_translate.json"));
    let sidecar = env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/to_translate.sidecar.json"));

    let engine = Breviarium::embedded().map_err(|error| error.to_string())?;
    let mut array = Vec::<String>::new();
    let mut entries = Vec::<SidecarEntry>::new();

    for text in engine.catalog().corpus_texts() {
        let Some(nodes) = text.content.get("la") else {
            continue;
        };
        let (block, segments) = translation_block(nodes);
        if block.trim().is_empty() {
            continue;
        }
        let index = array.len();
        array.push(block.clone());
        entries.push(SidecarEntry {
            index,
            corpus_id: text.id,
            role: role_name(&text.role),
            available_languages: text.content.keys().cloned().collect(),
            source_line_count: block.lines().count(),
            segments,
        });
    }

    write_json(&output, &array)?;
    write_json(
        &sidecar,
        &Sidecar {
            schema: "breviarium_translation_sidecar_v3",
            source_language: "la",
            target_language: "en2",
            array_path: output.to_string_lossy().as_ref(),
            entries,
        },
    )?;

    println!("translation strings: {}", array.len());
    println!("array: {}", output.display());
    println!("sidecar: {}", sidecar.display());
    Ok(())
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize {}: {error}", path.display()))?;
    fs::write(path, json).map_err(|error| format!("{}: {error}", path.display()))
}

fn translation_block(nodes: &[ContentNode]) -> (String, Vec<Segment>) {
    let mut lines = Vec::new();
    let mut segments = Vec::new();
    let mut next_line = 1usize;
    for (node_index, node) in nodes.iter().enumerate() {
        let Some((field, text)) = translatable_text(node) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        let line_count = text.lines().count().max(1);
        let start_line = next_line;
        let end_line = start_line + line_count - 1;
        lines.push(text.to_string());
        segments.push(Segment {
            node_index,
            field,
            start_line,
            end_line,
        });
        next_line = end_line + 1;
    }
    (lines.join("\n"), segments)
}

fn translatable_text(node: &ContentNode) -> Option<(&'static str, &str)> {
    match node {
        ContentNode::Text { text } => Some(("text", text)),
        ContentNode::Rubric { text } => Some(("text", text)),
        ContentNode::Marker { text } => Some(("text", text)),
        ContentNode::Heading { text } => Some(("text", text)),
        ContentNode::Citation { text } => Some(("text", text)),
        ContentNode::Versicle { text } => Some(("text", text)),
        ContentNode::Response { text } => Some(("text", text)),
        ContentNode::ShortResponse { text } => Some(("text", text)),
        ContentNode::Prayer { text } => Some(("text", text)),
        ContentNode::Blessing { text } => Some(("text", text)),
        ContentNode::Antiphon { text } => Some(("text", text)),
        ContentNode::Psalmody { antiphon, .. } => Some(("antiphon", antiphon)),
        ContentNode::TableRow { text, .. } => text.as_deref().map(|text| ("text", text)),
        ContentNode::PsalmRef { .. } | ContentNode::Rank { .. } | ContentNode::Rule { .. } => None,
        _ => None,
    }
}

fn role_name(role: &TextRole) -> String {
    let debug = format!("{role:?}");
    let mut output = String::new();
    for (index, ch) in debug.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
        } else {
            output.push(ch);
        }
    }
    output
}

#[derive(Serialize)]
struct Sidecar<'a> {
    schema: &'a str,
    source_language: &'a str,
    target_language: &'a str,
    array_path: &'a str,
    entries: Vec<SidecarEntry>,
}

#[derive(Serialize)]
struct SidecarEntry {
    index: usize,
    corpus_id: String,
    role: String,
    available_languages: Vec<String>,
    source_line_count: usize,
    segments: Vec<Segment>,
}

#[derive(Serialize)]
struct Segment {
    node_index: usize,
    field: &'static str,
    start_line: usize,
    end_line: usize,
}
