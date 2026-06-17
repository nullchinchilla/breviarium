use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

fn main() {
    let root = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/divinum-officium-master"));

    if let Err(error) = run(&root) {
        eprintln!("import-divinum: {error}");
        std::process::exit(1);
    }
}

fn run(root: &Path) -> Result<(), String> {
    let www = root.join("web/www");
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    let output_roots = [
        "corpus",
        "sources",
        "books",
        "lexicon",
        "texts",
        "office",
        "mass",
        "martyrology",
        "necrology",
        "appendix",
        "tables",
    ]
    .map(|service| data_dir.join(service));

    for output_root in &output_roots {
        if output_root.exists() {
            fs::remove_dir_all(output_root)
                .map_err(|error| format!("{}: {error}", output_root.display()))?;
        }
    }
    fs::create_dir_all(data_dir.join("books"))
        .map_err(|error| format!("{}: {error}", data_dir.join("books").display()))?;
    fs::create_dir_all(data_dir.join("lexicon"))
        .map_err(|error| format!("{}: {error}", data_dir.join("lexicon").display()))?;

    let mut corpus = SourceCorpus::default();
    let mut stats = ImportStats::default();
    for service_root in ["horas"] {
        let path = www.join(service_root);
        collect_tree(&www, &path, service_root, &mut corpus, &mut stats)?;
    }
    corpus.build_section_index();
    let sanctoral_replacements = roman_1960_sanctoral_replacements(&corpus);

    let mut bundles = BTreeMap::<BundleKey, BTreeMap<String, PrioritizedTextRecord>>::new();
    for source_path in corpus.files.keys().cloned().collect::<Vec<_>>() {
        let Some(file) = corpus.files.get(&source_path).cloned() else {
            continue;
        };
        if !matches!(file.language.as_str(), "la" | "en") {
            continue;
        }
        let Some(source) = canonical_source(&file, &corpus, &sanctoral_replacements) else {
            continue;
        };
        let key = BundleKey {
            language: file.language.clone(),
            category: source.bundle.clone(),
        };
        let target = bundles.entry(key).or_default();

        for section in selected_sections(&file) {
            let Some(slot) = canonical_slot(&source.key, &file.source_path, &section.name) else {
                continue;
            };
            let mut stack = Vec::new();
            let mut content = canonicalize_content(
                &file.language,
                corpus.expand_section(
                    &file.language,
                    &file.source_path,
                    &section.name,
                    Some(section),
                    &mut stack,
                ),
            );
            normalize_record_content(&source.key, &slot.key, &mut content);
            let record = TextRecord {
                id: format!("{}.{}", source.key.replace('/', "."), slot.key),
                role: slot.role,
                content,
            };
            let priority = source.priority * 1000 + slot.priority;
            let entry = target
                .entry(record.id.clone())
                .or_insert(PrioritizedTextRecord {
                    priority: i32::MIN,
                    record: record.clone(),
                });
            if priority >= entry.priority {
                *entry = PrioritizedTextRecord { priority, record };
            }
        }
    }

    let structural_aliases =
        structural_aliases_from_primary_language(&corpus, &sanctoral_replacements);
    fill_common_inheritance(&mut bundles);
    fill_structural_aliases(&mut bundles, &structural_aliases);

    let normalized = normalize_bundles(bundles)?;
    stats.generated_corpus_texts = normalized.corpus.values().map(BTreeMap::len).sum();
    stats.generated_source_sections = normalized
        .sources
        .values()
        .flat_map(BTreeMap::values)
        .map(|source| source.sections.len())
        .sum();
    stats.generated_bundles += emit_books_lexicon(&normalized, &data_dir)?;

    println!("Divinum Officium source: {}", root.display());
    println!("Imported files: {}", stats.imported_files);
    println!(
        "Lossy-decoded non-UTF-8 files: {}",
        stats.lossy_decoded_files
    );
    println!("Skipped binary assets: {}", stats.skipped_binary_assets);
    println!("Generated corpus texts: {}", stats.generated_corpus_texts);
    println!(
        "Generated source sections: {}",
        stats.generated_source_sections
    );
    println!("Generated YAML bundles: {}", stats.generated_bundles);
    println!(
        "Output: {}",
        data_dir
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(&data_dir)
            .display()
    );

    Ok(())
}

fn collect_tree(
    www: &Path,
    path: &Path,
    service_root: &str,
    corpus: &mut SourceCorpus,
    stats: &mut ImportStats,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))? {
        let entry = entry.map_err(|error| format!("{}: {error}", path.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if file_type.is_dir() {
            collect_tree(www, &path, service_root, corpus, stats)?;
        } else if file_type.is_file() {
            collect_file(www, &path, service_root, corpus, stats)?;
        }
    }

    Ok(())
}

fn collect_file(
    www: &Path,
    path: &Path,
    service_root: &str,
    corpus: &mut SourceCorpus,
    stats: &mut ImportStats,
) -> Result<(), String> {
    if is_binary_asset(path) {
        stats.skipped_binary_assets += 1;
        return Ok(());
    }

    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            stats.lossy_decoded_files += 1;
            String::from_utf8_lossy(error.as_bytes()).into_owned()
        }
    };
    text = text.replace("\r\n", "\n").replace('\r', "\n");
    if let Some(stripped) = text.strip_prefix('\u{feff}') {
        text = stripped.to_string();
    }
    text = sanitize_yaml_text(&text);

    let relative = relative_to(www, path)?;
    let source_path = format!("web/www/{relative}");
    let parts = relative.split('/').collect::<Vec<_>>();
    let service = semantic_service(service_root, &parts).to_string();
    let language = language_id(service_root, &parts).to_string();
    let category = category(service_root, &parts);
    let sections = if service == "martyrology" {
        vec![Section {
            name: "raw".to_string(),
            qualifier: None,
            body: text.trim().to_string(),
        }]
    } else {
        parse_sections(&text)
    };

    corpus.files.insert(
        source_path.clone(),
        SourceFile {
            service,
            language,
            category,
            source_path,
            sections,
        },
    );
    stats.imported_files += 1;
    Ok(())
}

#[derive(Default)]
struct SourceCorpus {
    files: BTreeMap<String, SourceFile>,
    section_index: BTreeMap<(String, String), Vec<SectionLocation>>,
}

impl SourceCorpus {
    fn build_section_index(&mut self) {
        self.section_index.clear();
        for file in self.files.values() {
            for section in &file.sections {
                self.section_index
                    .entry((file.language.clone(), normalize_section_name(&section.name)))
                    .or_default()
                    .push(SectionLocation {
                        source_path: file.source_path.clone(),
                        section_name: section.name.clone(),
                    });
            }
        }
    }

    fn expand_section(
        &self,
        language: &str,
        source_path: &str,
        section_name: &str,
        known_section: Option<&Section>,
        stack: &mut Vec<(String, String, String)>,
    ) -> Vec<ContentItem> {
        let key = (
            language.to_string(),
            source_path.to_string(),
            section_name.to_string(),
        );
        if stack.contains(&key) {
            return vec![ContentItem::Rubric {
                text: "cyclic migrated reference".to_string(),
            }];
        }
        stack.push(key);

        let section = known_section
            .cloned()
            .or_else(|| self.find_section(source_path, section_name).cloned());
        let items = section.map_or_else(
            || {
                vec![ContentItem::Rubric {
                    text: "missing migrated reference".to_string(),
                }]
            },
            |section| self.expand_body(language, source_path, &section, stack),
        );

        stack.pop();
        items
    }

    fn expand_body(
        &self,
        language: &str,
        source_path: &str,
        section: &Section,
        stack: &mut Vec<(String, String, String)>,
    ) -> Vec<ContentItem> {
        let section_kind = SectionKind::from_name(&section.name);
        match section_kind {
            SectionKind::Rank => return vec![parse_rank(&section.body)],
            SectionKind::Rule => return vec![parse_rule(&section.body)],
            SectionKind::Psalmody | SectionKind::Table | SectionKind::Text => {}
        }

        let lines = section
            .body
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if is_minor_psalm_table(source_path, &section.name) {
            if let Some(items) = parse_alternating_psalm_table(&lines) {
                return items;
            }
        }
        let mut items = self.expand_lines(language, source_path, &section.name, &lines, stack);
        if items.is_empty() {
            items.push(ContentItem::Text {
                text: String::new(),
            });
        }
        items
    }

    fn expand_include(
        &self,
        language: &str,
        current_source_path: &str,
        current_section_name: &str,
        input: &str,
        stack: &mut Vec<(String, String, String)>,
    ) -> Vec<ContentItem> {
        let include = parse_include(input);
        let Some(source_path) = include_source_path(
            language,
            current_source_path,
            include.target.file.as_deref(),
        ) else {
            return vec![ContentItem::Rubric {
                text: format!("unresolvable migrated include {input}"),
            }];
        };
        let section_name = include
            .target
            .section
            .as_deref()
            .unwrap_or(current_section_name);

        let mut items = self.expand_section(language, &source_path, section_name, None, stack);
        if include.selection.is_none() && include.transforms.is_empty() {
            return items;
        }

        let mut lines = canonical_lines(&items);
        if let Some(selection) = include.selection {
            lines = select_lines(&lines, selection);
        }
        if !include.transforms.is_empty() {
            let mut text = lines.join("\n");
            for transform in include.transforms {
                text = apply_substitute(&text, &transform);
            }
            lines = text.lines().map(ToOwned::to_owned).collect();
        }
        items = self.expand_lines(language, &source_path, section_name, &lines, stack);
        items
    }

    fn expand_lines(
        &self,
        language: &str,
        source_path: &str,
        section_name: &str,
        lines: &[String],
        stack: &mut Vec<(String, String, String)>,
    ) -> Vec<ContentItem> {
        let mut items = Vec::new();
        let mut literal = Vec::new();
        for line in lines {
            let mut trimmed = line.trim();
            let mut literal_line = None;
            if let Some((condition, rest)) = split_conditional_prefix(trimmed) {
                if !rubrical_condition_matches_roman_1960(condition) {
                    continue;
                }
                trimmed = rest.trim();
                if trimmed.is_empty() {
                    continue;
                }
                literal_line = Some(trimmed.to_string());
            }
            if let Some((condition, rest)) = split_legacy_parenthetical_prefix(trimmed) {
                if looks_like_legacy_condition(condition)
                    && matches!(rest.trim().chars().next(), Some('$' | '@' | '&'))
                {
                    continue;
                }
            }

            if let Some(include) = trimmed.strip_prefix('@') {
                flush_literal(&mut items, &mut literal);
                items.extend(self.expand_include(
                    language,
                    source_path,
                    section_name,
                    include,
                    stack,
                ));
            } else if let Some(call) = trimmed.strip_prefix('$') {
                flush_literal(&mut items, &mut literal);
                items.extend(self.expand_macro(language, source_path, call, stack));
            } else if let Some(call) = trimmed.strip_prefix('&') {
                flush_literal(&mut items, &mut literal);
                items.extend(self.expand_command(language, source_path, call, stack));
            } else if let Some((text, call)) = split_inline_macro(trimmed) {
                if !text.trim().is_empty() {
                    literal.push(text.trim_end().to_string());
                }
                flush_literal(&mut items, &mut literal);
                items.extend(self.expand_macro(language, source_path, call, stack));
            } else if let Some(marker) = trimmed.strip_prefix('!') {
                flush_literal(&mut items, &mut literal);
                items.push(ContentItem::Marker {
                    text: marker.trim().to_string(),
                });
            } else if let Some(rubric) = parse_rubric_line(trimmed) {
                if is_roman_1960_replacement_marker(&rubric) {
                    remove_previous_literal_line(&mut items, &mut literal);
                } else {
                    flush_literal(&mut items, &mut literal);
                }
                if !rubric.starts_with("sed rubrica ") {
                    items.push(ContentItem::Rubric { text: rubric });
                }
            } else if trimmed == "_" || trimmed == "*" {
                literal.push(String::new());
            } else if let Some(item) = parse_structured_line(section_name, trimmed) {
                flush_literal(&mut items, &mut literal);
                items.push(item);
            } else {
                literal.push(literal_line.unwrap_or_else(|| line.to_string()));
            }
        }
        flush_literal(&mut items, &mut literal);
        items
    }

    fn expand_macro(
        &self,
        language: &str,
        current_source_path: &str,
        input: &str,
        stack: &mut Vec<(String, String, String)>,
    ) -> Vec<ContentItem> {
        let (name, args) = parse_macro_call(input);
        if name.eq_ignore_ascii_case("rubrica") {
            return args
                .first()
                .map(|text| vec![ContentItem::Rubric { text: text.clone() }])
                .unwrap_or_default();
        }
        if name.eq_ignore_ascii_case("ant") {
            return vec![ContentItem::Marker {
                text: "repeat full invitatory antiphon".to_string(),
            }];
        }
        if name.eq_ignore_ascii_case("ant2") {
            return vec![ContentItem::Marker {
                text: "repeat second half of invitatory antiphon".to_string(),
            }];
        }
        self.expand_named_common(language, current_source_path, &name, &args, stack)
    }

    fn expand_command(
        &self,
        language: &str,
        current_source_path: &str,
        input: &str,
        stack: &mut Vec<(String, String, String)>,
    ) -> Vec<ContentItem> {
        let (name, args) = parse_macro_call(input);
        if name.eq_ignore_ascii_case("psalm") {
            if let Some(number) = args.first() {
                return vec![ContentItem::PsalmRef {
                    number: number.clone(),
                    start: args.get(1).cloned(),
                    end: args.get(2).cloned(),
                    optional: false,
                }];
            }
        }
        if let Some(section) = script_command_section(&name) {
            return self.expand_named_common(language, current_source_path, section, &args, stack);
        }
        self.expand_named_common(language, current_source_path, &name, &args, stack)
    }

    fn expand_named_common(
        &self,
        language: &str,
        current_source_path: &str,
        name: &str,
        args: &[String],
        stack: &mut Vec<(String, String, String)>,
    ) -> Vec<ContentItem> {
        if !args.is_empty() {
            return vec![ContentItem::Rubric {
                text: format!(
                    "unexpanded parameterized migrated command {name}({})",
                    args.join(", ")
                ),
            }];
        }

        let name = normalized_command_name(name);
        let candidates = common_section_candidates(language, current_source_path, &name);
        for (source_path, section_name) in candidates {
            if self.find_section(&source_path, &section_name).is_some() {
                return self.expand_section(language, &source_path, &section_name, None, stack);
            }
        }

        if let Some(location) = self.find_section_by_name(language, &name) {
            return self.expand_section(
                language,
                &location.source_path,
                &location.section_name,
                None,
                stack,
            );
        }

        vec![ContentItem::Rubric {
            text: format!("unexpanded migrated command {name}"),
        }]
    }

    fn find_section(&self, source_path: &str, section_name: &str) -> Option<&Section> {
        let file = self.files.get(source_path)?;
        if let Some(section) = preferred_section(
            file.sections
                .iter()
                .filter(|section| section.name == section_name),
        ) {
            return Some(section);
        }
        let alternate = format!("{section_name}_");
        if alternate != section_name {
            if let Some(section) = preferred_section(
                file.sections
                    .iter()
                    .filter(|section| section.name == alternate),
            ) {
                return Some(section);
            }
        }
        let normalized = normalize_section_name(section_name);
        preferred_section(file.sections.iter().filter(|section| {
            section.name != section_name
                && section.name != alternate
                && normalize_section_name(&section.name) == normalized
        }))
    }

    fn find_section_by_name(&self, language: &str, name: &str) -> Option<SectionLocation> {
        let normalized = normalize_section_name(name);
        self.section_index
            .get(&(language.to_string(), normalized))
            .and_then(|locations| locations.first())
            .cloned()
    }
}

fn preferred_section<'a>(sections: impl IntoIterator<Item = &'a Section>) -> Option<&'a Section> {
    let mut sections = sections.into_iter().collect::<Vec<_>>();
    sections.sort_by_key(|section| {
        if section
            .qualifier
            .as_deref()
            .is_some_and(|qualifier| qualifier.contains("196"))
        {
            2
        } else if section.qualifier.is_none() {
            1
        } else {
            0
        }
    });
    sections.pop()
}

fn selected_sections(file: &SourceFile) -> Vec<&Section> {
    let names = file
        .sections
        .iter()
        .map(|section| section.name.as_str())
        .collect::<BTreeSet<_>>();
    names
        .into_iter()
        .filter_map(|name| {
            preferred_section(file.sections.iter().filter(|section| section.name == name))
        })
        .collect()
}

fn parse_sections(text: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_qualifier: Option<String> = None;
    let mut current_body = Vec::new();

    for line in text.lines() {
        if let Some((name, qualifier)) = section_heading(line) {
            if let Some(name) = current_name.replace(name) {
                sections.push(Section {
                    name,
                    qualifier: current_qualifier.take(),
                    body: current_body.join("\n").trim().to_string(),
                });
                current_body.clear();
            }
            current_qualifier = qualifier;
        } else {
            current_body.push(line.to_string());
        }
    }

    if let Some(name) = current_name {
        sections.push(Section {
            name,
            qualifier: current_qualifier,
            body: current_body.join("\n").trim().to_string(),
        });
    } else if !text.trim().is_empty() {
        sections.push(Section {
            name: "raw".to_string(),
            qualifier: None,
            body: text.trim().to_string(),
        });
    }

    sections
}

fn section_heading(line: &str) -> Option<(String, Option<String>)> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix('[')?;
    let (name, suffix) = rest.split_once(']')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let qualifier = suffix
        .trim()
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Some((name.to_string(), qualifier))
}

fn parse_structured_line(section_name: &str, line: &str) -> Option<ContentItem> {
    if line.is_empty()
        || line.starts_with('(')
        || line.starts_with('/')
        || line.starts_with('@')
        || line.starts_with('$')
        || line.starts_with('&')
    {
        return None;
    }
    if let Some((label, tail)) = line.split_once('=') {
        if tail.contains(";;") || looks_like_psalm_list(tail) {
            let (text, psalms) = split_text_psalms(tail);
            return Some(ContentItem::TableRow {
                label: normalize_space(label),
                text: (!text.is_empty()).then_some(text),
                psalms,
            });
        }
    }
    if line.contains(";;") {
        let (text, psalms) = split_text_psalms(line);
        if !psalms.is_empty() {
            return Some(ContentItem::Psalmody {
                antiphon: text,
                psalms,
            });
        }
    }
    if is_antiphon_section(section_name) {
        return Some(ContentItem::Antiphon {
            text: normalize_antiphon(line),
        });
    }
    None
}

fn is_antiphon_section(section_name: &str) -> bool {
    let lower = section_name.to_ascii_lowercase();
    lower == "invit" || lower.starts_with("ant") || lower.contains("antiphon")
}

fn split_text_psalms(input: &str) -> (String, Vec<PsalmReference>) {
    let (text, psalms) = input
        .split_once(";;")
        .map(|(text, psalms)| (text.trim(), psalms.trim()))
        .unwrap_or(("", input.trim()));
    let mut antiphon = normalize_antiphon(text);
    let mut references = Vec::new();
    for spec in split_psalm_specs(psalms) {
        if let Some(suffix) = psalm_spec_antiphon_suffix(&spec) {
            if !antiphon.contains(&suffix) {
                antiphon = format!("{antiphon} {suffix}");
            }
        }
        if let Some(reference) = parse_psalm_reference(&spec) {
            references.push(reference);
        }
    }
    (antiphon, references)
}

fn split_psalm_specs(input: &str) -> Vec<String> {
    let mut specs = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in input.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' | ';' if depth == 0 => {
                let spec = current.trim();
                if !spec.is_empty() {
                    specs.push(spec.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let spec = current.trim();
    if !spec.is_empty() {
        specs.push(spec.to_string());
    }
    specs
}

fn parse_psalm_reference(input: &str) -> Option<PsalmReference> {
    let trimmed = input.trim();
    let optional = trimmed.starts_with('[') && trimmed.ends_with(']');
    let normalized = trimmed
        .trim_matches('\'')
        .trim_start_matches('-')
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split_whitespace()
        .collect::<String>();
    if normalized.is_empty() {
        return None;
    }
    if let Some((number, range)) = normalized.split_once('(') {
        let range = range.strip_suffix(')').unwrap_or(range);
        if !range
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit() || ch == '\'')
        {
            return Some(PsalmReference {
                number: number.to_string(),
                start: None,
                end: None,
                optional,
            });
        }
        let (start, end) = range
            .split_once('-')
            .map(|(start, end)| (Some(clean_verse_label(start)), Some(clean_verse_label(end))))
            .unwrap_or_else(|| (Some(clean_verse_label(range)), None));
        Some(PsalmReference {
            number: number.to_string(),
            start,
            end,
            optional,
        })
    } else {
        Some(PsalmReference {
            number: normalized,
            start: None,
            end: None,
            optional,
        })
    }
}

fn psalm_spec_antiphon_suffix(input: &str) -> Option<String> {
    let trimmed = input
        .trim()
        .trim_matches('\'')
        .trim_start_matches('-')
        .trim_start_matches('[')
        .trim_end_matches(']');
    let (_, range) = trimmed.split_once('(')?;
    let range = range.strip_suffix(')').unwrap_or(range).trim();
    if range
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit() || ch == '\'')
    {
        None
    } else {
        Some(format!("({range})"))
    }
}

fn clean_verse_label(input: &str) -> String {
    input.trim().trim_matches('\'').to_string()
}

fn is_minor_psalm_table(source_path: &str, section_name: &str) -> bool {
    source_path.ends_with("/Psalterium/Psalmi/Psalmi minor.txt")
        && matches!(
            section_name,
            "Prima" | "Tertia" | "Sexta" | "Nona" | "Completorium"
        )
}

fn parse_alternating_psalm_table(lines: &[String]) -> Option<Vec<ContentItem>> {
    let lines = lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() % 2 != 0 {
        return None;
    }

    let mut items = Vec::new();
    for pair in lines.chunks_exact(2) {
        let (label, antiphon) = pair[0].split_once('=')?;
        if pair[1].contains('=') {
            return None;
        }
        let psalms = split_psalm_specs(pair[1])
            .into_iter()
            .filter_map(|spec| parse_psalm_reference(&spec))
            .collect::<Vec<_>>();
        if psalms.is_empty() {
            return None;
        }
        items.push(ContentItem::TableRow {
            label: normalize_space(label),
            text: Some(normalize_antiphon(antiphon)),
            psalms,
        });
    }
    (!items.is_empty()).then_some(items)
}

fn parse_rank(body: &str) -> ContentItem {
    let mut label = None;
    let mut value = None;
    let mut common = None;
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let parts = line.split(";;").collect::<Vec<_>>();
        if let Some(part) = parts
            .get(1)
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
        {
            label = Some((*part).to_string());
        }
        if let Some(part) = parts
            .get(2)
            .and_then(|part| part.trim().parse::<f32>().ok())
        {
            value = Some(part);
        }
        if let Some(part) = parts
            .iter()
            .find_map(|part| extract_source_ref(part.trim()))
        {
            common = Some(part);
        }
        if label.is_some() || value.is_some() || common.is_some() {
            break;
        }
    }
    ContentItem::Rank {
        label,
        value,
        common,
    }
}

fn parse_rule(body: &str) -> ContentItem {
    let mut tokens = Vec::new();
    let mut seen = BTreeSet::new();
    for raw in body
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let token = if let Some(target) = extract_source_ref(raw) {
            RuleToken::SourceRef {
                relation: if raw.trim_start().starts_with('@') {
                    "ref".to_string()
                } else {
                    raw.split_whitespace()
                        .next()
                        .unwrap_or("ref")
                        .to_ascii_lowercase()
                },
                target,
            }
        } else if let Some((key, value)) = raw.split_once('=') {
            RuleToken::Value {
                key: slug(key),
                value: value.trim().to_string(),
                label: raw.to_string(),
            }
        } else if let Some((head, tail)) = split_trailing_number(raw) {
            RuleToken::Value {
                key: slug(head),
                value: tail.to_string(),
                label: raw.to_string(),
            }
        } else {
            RuleToken::Flag {
                id: slug(raw),
                label: raw.to_string(),
            }
        };
        let key = token.key();
        if seen.insert(key) {
            tokens.push(token);
        }
    }
    ContentItem::Rule { tokens }
}

fn extract_source_ref(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_end_matches(';');
    if let Some(reference) = trimmed.strip_prefix('@') {
        return Some(
            reference
                .trim()
                .trim_start_matches(':')
                .strip_suffix(".txt")
                .unwrap_or_else(|| reference.trim().trim_start_matches(':'))
                .to_string(),
        );
    }
    trimmed
        .strip_prefix("ex ")
        .or_else(|| trimmed.strip_prefix("vide "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.strip_suffix(".txt").unwrap_or(value))
        .map(|value| {
            if value.starts_with('C') {
                format!("Commune/{value}")
            } else {
                value.to_string()
            }
        })
}

fn split_trailing_number(input: &str) -> Option<(&str, &str)> {
    let (head, tail) = input.rsplit_once(' ')?;
    if tail.chars().all(|ch| ch.is_ascii_digit()) {
        Some((head, tail))
    } else {
        None
    }
}

fn parse_include(input: &str) -> IncludeSpec {
    let mut parts = input.splitn(3, ':');
    let file = parts.next().unwrap_or_default().trim();
    let section = parts.next().map(str::trim);
    let tail = parts.next().map(str::trim).unwrap_or_default();

    let mut include = IncludeSpec {
        target: IncludeTarget {
            file: (!file.is_empty()).then(|| file.to_string()),
            section: section
                .filter(|section| !section.is_empty())
                .map(ToOwned::to_owned),
        },
        selection: None,
        transforms: Vec::new(),
    };
    parse_include_tail(tail, &mut include);
    include
}

fn parse_include_tail(mut tail: &str, include: &mut IncludeSpec) {
    tail = tail.trim_start_matches(':').trim();
    if tail.is_empty() {
        return;
    }
    if let Some((selection, rest)) = parse_selection_prefix(tail) {
        include.selection = Some(selection);
        tail = rest.trim();
    }
    while let Some(index) = tail.find("s/") {
        tail = &tail[index..];
        if let Some((transform, rest)) = parse_substitution(tail) {
            include.transforms.push(transform);
            tail = rest.trim();
        } else {
            break;
        }
    }
}

fn parse_selection_prefix(input: &str) -> Option<(TextSelection, &str)> {
    let token_end = input.find(char::is_whitespace).unwrap_or(input.len());
    let token = &input[..token_end];
    if token.is_empty() || !token.chars().all(|ch| ch.is_ascii_digit() || ch == '-') {
        return None;
    }
    let (start, end) = if let Some((start, end)) = token.split_once('-') {
        (
            start.parse::<usize>().ok()?,
            end.parse::<usize>().ok().filter(|end| *end > 0),
        )
    } else {
        (token.parse::<usize>().ok()?, None)
    };
    Some((TextSelection { start, end }, &input[token_end..]))
}

fn parse_substitution(input: &str) -> Option<(TextTransform, &str)> {
    let mut chars = input.chars();
    if chars.next()? != 's' {
        return None;
    }
    let delimiter = chars.next()?;
    let rest = &input[2..];
    let (pattern, rest) = take_until_unescaped(rest, delimiter)?;
    let (replacement, rest) = take_until_unescaped(rest, delimiter)?;
    let flags_len = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphabetic())
        .map(char::len_utf8)
        .sum::<usize>();
    let flags = rest[..flags_len].to_string();
    Some((
        TextTransform {
            pattern,
            replacement,
            flags,
        },
        &rest[flags_len..],
    ))
}

fn take_until_unescaped(input: &str, delimiter: char) -> Option<(String, &str)> {
    let mut output = String::new();
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if escaped {
            output.push('\\');
            output.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == delimiter {
            return Some((output, &input[index + ch.len_utf8()..]));
        }
        output.push(ch);
    }
    None
}

fn apply_substitute(text: &str, transform: &TextTransform) -> String {
    let pattern = unescape_legacy(&transform.pattern);
    let replacement = unescape_legacy(&transform.replacement);
    if pattern == ";;.*" {
        return text
            .lines()
            .map(|line| line.split_once(";;").map(|(head, _)| head).unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n");
    }
    if pattern == "$" {
        return if transform.flags.contains('m') {
            text.lines()
                .map(|line| format!("{line}{replacement}"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            format!("{text}{replacement}")
        };
    }
    if pattern == "^" {
        return if transform.flags.contains('m') {
            text.lines()
                .map(|line| format!("{replacement}{line}"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            format!("{replacement}{text}")
        };
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        if let Some(start) = text.find(prefix) {
            let mut output = String::new();
            output.push_str(&text[..start]);
            output.push_str(&replacement);
            return output;
        }
    }
    if transform.flags.contains('g') {
        text.replace(&pattern, &replacement)
    } else {
        text.replacen(&pattern, &replacement, 1)
    }
}

fn unescape_legacy(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => output.push('\n'),
                Some(other) => output.push(other),
                None => output.push('\\'),
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn parse_macro_call(input: &str) -> (String, Vec<String>) {
    let trimmed = input.trim();
    if let Some(text) = trimmed.strip_prefix("rubrica ") {
        return (
            "rubrica".to_string(),
            vec![text.trim().trim_end_matches('.').to_string()],
        );
    }
    if let Some((name, args)) = trimmed.split_once('(') {
        if let Some(args) = args.strip_suffix(')') {
            return (
                name.trim().to_string(),
                args.split(',')
                    .map(str::trim)
                    .filter(|arg| !arg.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
            );
        }
    }
    (trimmed.to_string(), Vec::new())
}

fn normalized_command_name(name: &str) -> String {
    let name = name.trim().trim_end_matches('.').trim();
    match name {
        "teDeum" => "Te Deum".to_string(),
        "Dominus_vobiscum" | "Dominus_vobiscum1" | "Dominus_vobiscum2" => "Dominus".to_string(),
        "Benedicamus_Domino" => "Benedicamus Domino".to_string(),
        "Divinum_auxilium" => "Divinum auxilium".to_string(),
        other => other.replace('_', " "),
    }
}

fn script_command_section(name: &str) -> Option<&'static str> {
    match name.trim().trim_end_matches('.') {
        "teDeum" => Some("Te Deum"),
        "Dominus_vobiscum" | "Dominus_vobiscum1" | "Dominus_vobiscum2" => Some("Dominus"),
        "Benedicamus_Domino" => Some("Benedicamus Domino"),
        "Divinum_auxilium" => Some("Divinum auxilium"),
        _ => None,
    }
}

fn include_source_path(
    language: &str,
    current_source_path: &str,
    file: Option<&str>,
) -> Option<String> {
    file.map_or_else(
        || Some(current_source_path.to_string()),
        |file| {
            let file = file.strip_suffix(".txt").unwrap_or(file);
            Some(format!(
                "web/www/horas/{}/{}.txt",
                divinum_language_dir(language),
                file
            ))
        },
    )
}

fn common_section_candidates(
    language: &str,
    current_source_path: &str,
    name: &str,
) -> Vec<(String, String)> {
    let dir = divinum_language_dir(language);
    let variants = section_name_variants(name);
    let files = [
        format!("web/www/horas/{dir}/Psalterium/Common/Prayers.txt"),
        format!("web/www/horas/{dir}/Psalterium/Common/Translate.txt"),
        format!("web/www/horas/{dir}/Psalterium/Common/Rubricae.txt"),
        format!("web/www/horas/{dir}/Psalterium/Special/Preces.txt"),
        format!("web/www/horas/{dir}/Psalterium/Doxologies.txt"),
        current_source_path.to_string(),
    ];
    files
        .into_iter()
        .flat_map(|file| {
            variants
                .iter()
                .map(move |name| (file.clone(), name.clone()))
        })
        .collect()
}

fn section_name_variants(name: &str) -> Vec<String> {
    let mut variants = vec![name.trim().to_string()];
    variants.push(name.trim().trim_end_matches('.').to_string());
    variants.push(name.trim().replace('_', " "));
    variants.push(name.trim().replace('-', " "));
    variants.push(split_camel_words(name.trim()));
    variants.sort();
    variants.dedup();
    variants
}

fn split_camel_words(input: &str) -> String {
    let mut output = String::new();
    let mut previous_lowercase = false;
    for ch in input.chars() {
        if previous_lowercase && ch.is_ascii_uppercase() {
            output.push(' ');
        }
        output.push(ch);
        previous_lowercase = ch.is_ascii_lowercase();
    }
    output
}

fn canonical_lines(items: &[ContentItem]) -> Vec<String> {
    let mut lines = Vec::new();
    for item in items {
        match item {
            ContentItem::Text { text }
            | ContentItem::Heading { text }
            | ContentItem::Citation { text }
            | ContentItem::Versicle { text }
            | ContentItem::Response { text }
            | ContentItem::ShortResponse { text }
            | ContentItem::Prayer { text }
            | ContentItem::Blessing { text } => lines.extend(text.lines().map(ToOwned::to_owned)),
            ContentItem::Antiphon { text } => lines.push(text.clone()),
            ContentItem::Rubric { text } => lines.push(format!("/:{}:/", text)),
            ContentItem::Marker { text } => lines.push(format!("!{text}")),
            ContentItem::PsalmRef {
                number,
                start,
                end,
                optional,
            } => lines.push(render_psalm_spec(&PsalmReference {
                number: number.clone(),
                start: start.clone(),
                end: end.clone(),
                optional: *optional,
            })),
            ContentItem::Psalmody { antiphon, psalms } => {
                lines.push(format!(
                    "{};;{}",
                    antiphon,
                    psalms
                        .iter()
                        .map(render_psalm_spec)
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
            ContentItem::TableRow {
                label,
                text,
                psalms,
            } => {
                lines.push(format!(
                    "{}={};;{}",
                    label,
                    text.as_deref().unwrap_or(""),
                    psalms
                        .iter()
                        .map(render_psalm_spec)
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
            ContentItem::Rank {
                label,
                value,
                common,
            } => lines.push(format!(
                ";;{};;{};;{}",
                label.as_deref().unwrap_or(""),
                value.map(|value| value.to_string()).unwrap_or_default(),
                common.as_deref().unwrap_or("")
            )),
            ContentItem::Rule { tokens } => lines.extend(tokens.iter().map(|token| token.label())),
        }
    }
    lines
}

fn render_psalm_spec(psalm: &PsalmReference) -> String {
    let mut output = psalm.number.clone();
    if let Some(start) = &psalm.start {
        output.push('(');
        output.push_str(start);
        if let Some(end) = &psalm.end {
            output.push('-');
            output.push_str(end);
        }
        output.push(')');
    }
    if psalm.optional {
        format!("[{output}]")
    } else {
        output
    }
}

fn select_lines(lines: &[String], selection: TextSelection) -> Vec<String> {
    if selection.start == 0 {
        return lines.to_vec();
    }
    let start = selection.start.saturating_sub(1);
    let end = selection.end.unwrap_or(selection.start).min(lines.len());
    if start >= end {
        Vec::new()
    } else {
        lines[start..end].to_vec()
    }
}

fn flush_literal(items: &mut Vec<ContentItem>, literal: &mut Vec<String>) {
    if literal.is_empty() {
        return;
    }
    while literal.first().is_some_and(|line| line.trim().is_empty()) {
        literal.remove(0);
    }
    while literal.last().is_some_and(|line| line.trim().is_empty()) {
        literal.pop();
    }
    if !literal.is_empty() {
        items.push(ContentItem::Text {
            text: literal.join("\n"),
        });
    }
    literal.clear();
}

fn remove_previous_literal_line(items: &mut Vec<ContentItem>, literal: &mut Vec<String>) {
    while literal.last().is_some_and(|line| line.trim().is_empty()) {
        literal.pop();
    }
    if literal.pop().is_some() {
        return;
    }

    let Some(ContentItem::Text { text }) = items.last_mut() else {
        return;
    };
    let mut lines = text.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines.pop();
    if lines.is_empty() {
        items.pop();
    } else {
        *text = lines.join("\n");
    }
}

fn split_conditional_prefix(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('(')?;
    let (condition, suffix) = rest.split_once(')')?;
    if suffix.trim().is_empty() || !is_rubrical_condition(condition) {
        return None;
    }
    Some((condition.trim(), suffix.trim()))
}

fn split_legacy_parenthetical_prefix(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('(')?;
    let (condition, suffix) = rest.split_once(')')?;
    let suffix = suffix.trim();
    if suffix.is_empty() {
        return None;
    }
    Some((condition.trim(), suffix))
}

fn is_rubrical_condition(condition: &str) -> bool {
    let condition = condition.trim();
    condition.starts_with("rubrica ")
        || condition.starts_with("sed rubrica ")
        || condition.starts_with("deinde rubrica ")
}

fn looks_like_legacy_condition(condition: &str) -> bool {
    let lower = condition.trim().to_ascii_lowercase();
    lower.starts_with("sed ")
        || lower.starts_with("feria ")
        || lower.starts_with("nisi ")
        || lower.starts_with("rubrica ")
        || lower.starts_with("deinde ")
        || lower.contains(" tempore ")
        || lower.contains(" post ")
}

fn split_inline_macro(line: &str) -> Option<(&str, &str)> {
    let index = line.rfind(" $")?;
    let call = line[index + 2..].trim();
    let first = call.chars().next()?;
    if !first.is_alphabetic() {
        return None;
    }
    Some((&line[..index], call))
}

fn is_roman_1960_replacement_marker(rubric: &str) -> bool {
    let lower = rubric.to_ascii_lowercase();
    lower.starts_with("sed ")
        && lower.contains("dicitur")
        && rubrical_condition_matches_roman_1960(rubric)
}

fn rubrical_condition_matches_roman_1960(condition: &str) -> bool {
    let normalized = condition.to_ascii_lowercase();
    let tokens = normalized
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens
        .windows(3)
        .any(|window| window == ["nisi", "rubrica", "1960"])
        || tokens
            .windows(3)
            .any(|window| window == ["nisi", "rubrica", "196"])
    {
        return false;
    }
    tokens
        .iter()
        .any(|token| *token == "1960" || *token == "196")
}

fn parse_rubric_line(line: &str) -> Option<String> {
    if line.starts_with("(/") {
        return None;
    }
    line.strip_prefix("/:")
        .and_then(|value| value.strip_suffix(":/"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            line.strip_prefix('(')
                .and_then(|value| value.strip_suffix(')'))
                .map(str::trim)
                .filter(|value| {
                    value.starts_with("rubrica ")
                        || value.starts_with("sed rubrica ")
                        || value.starts_with("deinde ")
                        || value.starts_with("Deinde ")
                })
                .map(ToOwned::to_owned)
        })
}

fn normalize_antiphon(input: &str) -> String {
    normalize_space(input).replace(" * ", " * ")
}

fn normalize_space(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_section_name(input: &str) -> String {
    input
        .trim()
        .trim_end_matches('_')
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn looks_like_psalm_list(input: &str) -> bool {
    split_psalm_specs(input).iter().all(|spec| {
        spec.trim_matches(['[', ']'])
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit())
    })
}

fn roman_1960_sanctoral_replacements(corpus: &SourceCorpus) -> BTreeSet<String> {
    let mut replacements = BTreeSet::new();
    let stems = corpus
        .files
        .values()
        .filter(|file| {
            file.language == "la" && file.service == "office" && file.category == "Sancti"
        })
        .filter_map(file_stem)
        .collect::<BTreeSet<_>>();
    for stem in &stems {
        let Some(base) = stem.strip_suffix('r') else {
            continue;
        };
        if !stems.contains(base) {
            replacements.insert(base.to_string());
            continue;
        }
        let base_path = format!("web/www/horas/Latin/Sancti/{base}.txt");
        if corpus
            .files
            .get(&base_path)
            .is_some_and(|file| source_file_rank_label(file).is_some_and(|rank| rank == "Vigilia"))
        {
            replacements.insert(base.to_string());
        }
    }
    replacements
}

fn source_file_rank_label(file: &SourceFile) -> Option<&str> {
    file.sections
        .iter()
        .find(|section| normalize_section_name(&section.name) == "rank")
        .and_then(|section| {
            section
                .body
                .lines()
                .find_map(|line| line.split(";;").nth(1).map(str::trim))
        })
        .filter(|label| !label.is_empty())
}

fn file_stem(file: &SourceFile) -> Option<String> {
    Path::new(&file.source_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToOwned::to_owned)
}

fn canonical_source(
    file: &SourceFile,
    corpus: &SourceCorpus,
    sanctoral_replacements: &BTreeSet<String>,
) -> Option<CanonicalSource> {
    if file.service == "martyrology" {
        let stem = file_stem(file)?;
        if !is_fixed_date_stem(&stem) {
            return None;
        }
        if file.language == "la" && !file.category.starts_with("Martyrologium1960") {
            return None;
        }
        if file.language == "en" && file.category != "Martyrologium" {
            return None;
        }
        return Some(CanonicalSource {
            key: format!("martyrology/{}", slug(&stem)),
            bundle: "martyrology".to_string(),
            priority: 10,
        });
    }

    if file.service != "office" {
        return None;
    }

    let stem = file_stem(file)?;
    match file.category.as_str() {
        "Tempora" => {
            let (stem, priority) = canonical_temporal_stem(&stem);
            Some(CanonicalSource {
                key: format!("proper/temporal/{}", slug(&stem)),
                bundle: "propers-temporal".to_string(),
                priority,
            })
        }
        "Sancti" => {
            let (stem, priority) = canonical_sanctoral_stem(&stem, corpus, sanctoral_replacements);
            Some(CanonicalSource {
                key: format!("proper/sanctoral/{}", slug(&stem)),
                bundle: "propers-sanctoral".to_string(),
                priority,
            })
        }
        "Commune" => Some(CanonicalSource {
            key: format!("common/{}", slug(&stem)),
            bundle: "commons".to_string(),
            priority: common_source_priority(&stem),
        }),
        "Psalterium" => canonical_psalter_source(file, &stem),
        _ => None,
    }
}

fn is_fixed_date_stem(stem: &str) -> bool {
    let bytes = stem.as_bytes();
    bytes.len() == 5
        && bytes[2] == b'-'
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
}

fn canonical_temporal_stem(stem: &str) -> (String, i32) {
    if let Some(base) = stem.strip_suffix("Feria") {
        return (base.to_string(), 50);
    }
    if let Some(base) = stem.strip_suffix('o') {
        return (base.to_string(), 30);
    }
    if let Some(base) = stem.strip_suffix('t') {
        return (base.to_string(), 20);
    }
    if let Some(base) = stem.strip_suffix('r') {
        return (base.to_string(), 10);
    }
    (stem.to_string(), 40)
}

fn canonical_sanctoral_stem(
    stem: &str,
    corpus: &SourceCorpus,
    sanctoral_replacements: &BTreeSet<String>,
) -> (String, i32) {
    let Some(base) = stem.strip_suffix('r') else {
        return (stem.to_string(), 40);
    };
    let base_exists = ["Latin", "English"].iter().any(|language| {
        corpus
            .files
            .contains_key(&format!("web/www/horas/{language}/Sancti/{base}.txt"))
    });
    if sanctoral_replacements.contains(base) || !base_exists {
        (base.to_string(), 60)
    } else {
        (format!("{base}-reduced"), 10)
    }
}

fn common_source_priority(stem: &str) -> i32 {
    if stem
        .chars()
        .last()
        .is_some_and(|ch| ch.is_ascii_lowercase())
    {
        50
    } else {
        40
    }
}

fn canonical_psalter_source(file: &SourceFile, stem: &str) -> Option<CanonicalSource> {
    let path = file.source_path.as_str();
    let (key, bundle, priority) = if path.contains("/Psalterium/Common/Prayers.txt") {
        ("ordinary/formulae".to_string(), "ordinary", 60)
    } else if path.contains("/Psalterium/Common/Translate.txt") {
        ("ordinary/formulae".to_string(), "ordinary", 30)
    } else if path.contains("/Psalterium/Special/Major Special.txt") {
        ("ordinary/major".to_string(), "ordinary", 40)
    } else if path.contains("/Psalterium/Special/Minor Special.txt") {
        ("ordinary/minor".to_string(), "ordinary", 40)
    } else if path.contains("/Psalterium/Special/Prima Special.txt") {
        ("ordinary/prime".to_string(), "ordinary", 40)
    } else if path.contains("/Psalterium/Special/Matutinum Special.txt") {
        ("ordinary/matins".to_string(), "ordinary", 40)
    } else if path.contains("/Psalterium/Psalmi/Psalmi major.txt") {
        ("psalter/major".to_string(), "psalter", 40)
    } else if path.contains("/Psalterium/Psalmi/Psalmi minor.txt") {
        ("psalter/minor".to_string(), "psalter", 40)
    } else if path.contains("/Psalterium/Psalmi/Psalmi matutinum.txt") {
        ("psalter/matins".to_string(), "psalter", 40)
    } else if path.contains("/Psalterium/Psalmorum/") {
        (
            format!("psalm/{}", slug(stem.trim_start_matches("Psalm"))),
            "psalms",
            40,
        )
    } else if path.ends_with("/Psalterium/Benedictions.txt") {
        ("ordinary/benedictions".to_string(), "ordinary", 40)
    } else if path.ends_with("/Psalterium/Mariaant.txt") {
        ("ordinary/marian-antiphons".to_string(), "ordinary", 40)
    } else {
        return None;
    };
    Some(CanonicalSource {
        key,
        bundle: bundle.to_string(),
        priority,
    })
}

fn canonical_slot(source_key: &str, source_path: &str, section: &str) -> Option<CanonicalSlot> {
    if is_comment_section(section) {
        return None;
    }
    if source_key.starts_with("psalm/") || source_key.starts_with("martyrology/") {
        return Some(slot("raw", "psalm", 40));
    }
    if source_key.starts_with("proper/") || source_key.starts_with("common/") {
        return canonical_proper_slot(section);
    }
    if source_key == "ordinary/formulae" {
        return Some(slot(&slug(section), formula_role(section), 40));
    }
    if source_key == "ordinary/benedictions" {
        return Some(slot(&canonical_benediction_slot(section)?, "blessing", 40));
    }
    if source_key == "ordinary/major" {
        return Some(slot(&slug(section), ordinary_major_role(section), 40));
    }
    if source_key == "ordinary/minor" {
        return Some(slot(
            &canonical_minor_ordinary_slot(section),
            ordinary_minor_role(section),
            40,
        ));
    }
    if source_key == "ordinary/prime" {
        return Some(slot(
            &canonical_prime_ordinary_slot(section),
            ordinary_prime_role(section),
            40,
        ));
    }
    if source_key == "ordinary/matins" {
        return Some(slot(&slug(section), ordinary_matins_role(section), 40));
    }
    if source_key == "ordinary/marian-antiphons" {
        return Some(slot(&slug(section), "marian_antiphon", 40));
    }
    if source_key.starts_with("psalter/") {
        return Some(slot(&slug(section), "psalmody", 40));
    }
    if source_path.contains("/Psalterium/") {
        return Some(slot(&slug(section), "note", 10));
    }
    None
}

fn is_comment_section(section: &str) -> bool {
    matches!(
        normalize_section_name(section).as_str(),
        "comment" | "comments"
    )
}

fn canonical_proper_slot(section: &str) -> Option<CanonicalSlot> {
    let normalized = normalize_section_name(section);
    let lower = normalized.as_str();
    match lower {
        "officium" => Some(slot("title", "note", 100)),
        "rank" => Some(slot("rank", "rubric", 100)),
        "rule" => Some(slot("rules", "rubric", 100)),
        "invit" => Some(slot("matins-invitatory", "invitatory", 100)),
        "hymnus matutinum" => Some(slot("matins-hymn", "hymn", 100)),
        "ant matutinum" => Some(slot("matins-psalmody", "psalmody", 100)),
        "ant laudes" => Some(slot("lauds-psalmody", "psalmody", 100)),
        "capitulum laudes" => Some(slot("lauds-chapter", "chapter", 100)),
        "hymnus laudes" => Some(slot("lauds-hymn", "hymn", 100)),
        "versum 2" => Some(slot("lauds-versicle", "versicle", 100)),
        "ant 2" => Some(slot("lauds-gospel-antiphon", "antiphon", 100)),
        "ant vespera" => Some(slot("vespers-psalmody", "psalmody", 100)),
        "capitulum vespera" | "capitulum vespera 1" | "capitulum vespera 3" => Some(slot(
            "vespers-chapter",
            "chapter",
            vespers_chapter_priority(lower),
        )),
        "hymnus vespera" | "hymnusm vespera" => Some(slot(
            "vespers-hymn",
            "hymn",
            if lower.starts_with("hymnusm") {
                90
            } else {
                100
            },
        )),
        "versum 1" | "versum 3" => Some(slot(
            "vespers-versicle",
            "versicle",
            if lower == "versum 3" { 100 } else { 80 },
        )),
        "ant 3" => Some(slot("vespers-gospel-antiphon", "antiphon", 100)),
        "oratio matutinum" => Some(slot("matins-collect", "collect", 100)),
        "oratio" => Some(slot("collect", "collect", 60)),
        "oratio2" | "oratio 2" => Some(slot("daytime-collect", "collect", 100)),
        "oratio3" | "oratio 3" => Some(slot("vespers-collect", "collect", 100)),
        "oratio 1 loco" | "oratio 2 loco" => Some(slot("collect", "collect", 40)),
        "lectio prima" => Some(slot("prime-short-reading", "short_reading", 100)),
        _ => {
            if let Some(number) = lower.strip_prefix("lectio") {
                return canonical_lesson_slot(number);
            }
            if let Some(number) = lower.strip_prefix("responsory") {
                return canonical_responsory_slot(number);
            }
            if let Some(nocturn) = lower
                .strip_prefix("nocturn ")
                .and_then(|tail| tail.strip_suffix(" versum"))
            {
                return Some(slot(
                    &format!("matins-nocturn-{}-versicle", slug(nocturn)),
                    "versicle",
                    100,
                ));
            }
            if let Some(hour) = lower.strip_prefix("capitulum ") {
                return minor_hour_slot(hour, "chapter", "chapter");
            }
            if let Some(hour) = lower
                .strip_prefix("responsory breve ")
                .or_else(|| lower.strip_prefix("responsory breve "))
            {
                return minor_hour_slot(hour, "short-responsory", "short_responsory");
            }
            if let Some(hour) = lower.strip_prefix("versum ") {
                return minor_hour_slot(hour, "versicle", "versicle");
            }
            if let Some(hour) = lower.strip_prefix("ant ") {
                return minor_hour_slot(hour, "antiphon", "antiphon");
            }
            None
        }
    }
}

fn canonical_lesson_slot(number: &str) -> Option<CanonicalSlot> {
    let number = number.trim();
    if number == "93" || number == "94" {
        return Some(slot("matins-reading-3-abbreviated", "reading", 100));
    }
    let number = number.parse::<usize>().ok()?;
    Some(slot(&format!("matins-reading-{number}"), "reading", 100))
}

fn canonical_responsory_slot(number: &str) -> Option<CanonicalSlot> {
    let number = number.trim().parse::<usize>().ok()?;
    Some(slot(
        &format!("matins-responsory-{number}"),
        "responsory",
        100,
    ))
}

fn minor_hour_slot(hour: &str, suffix: &str, role: &str) -> Option<CanonicalSlot> {
    let hour = canonical_minor_hour(hour)?;
    Some(slot(&format!("{hour}-{suffix}"), role, 100))
}

fn canonical_minor_hour(hour: &str) -> Option<&'static str> {
    match hour.trim() {
        "prima" => Some("prime"),
        "tertia" => Some("terce"),
        "sexta" => Some("sext"),
        "nona" => Some("none"),
        _ => None,
    }
}

fn vespers_chapter_priority(section: &str) -> i32 {
    match section {
        "capitulum vespera 3" => 110,
        "capitulum vespera" => 100,
        "capitulum vespera 1" => 90,
        _ => 80,
    }
}

fn canonical_benediction_slot(section: &str) -> Option<String> {
    match normalize_section_name(section).as_str() {
        "absolutiones" => Some("matins-absolutions".to_string()),
        "nocturn 1" => Some("matins-blessings-nocturn-1".to_string()),
        "nocturn 2" => Some("matins-blessings-nocturn-2".to_string()),
        "nocturn 3" => Some("matins-blessings-nocturn-3".to_string()),
        "nocturn 3 12-25" => Some("matins-blessings-nocturn-3-christmas".to_string()),
        _ => None,
    }
}

fn canonical_minor_ordinary_slot(section: &str) -> String {
    match normalize_section_name(section).as_str() {
        "lectio completorium" => "compline-short-reading".to_string(),
        "completorium" => "compline-chapter".to_string(),
        "responsory completorium" => "compline-short-responsory".to_string(),
        "versum 4" => "compline-versicle".to_string(),
        "ant 4" => "compline-gospel-antiphon".to_string(),
        "ant 4 quad" => "compline-gospel-antiphon-lent".to_string(),
        "ant 4 quad5" => "compline-gospel-antiphon-passiontide".to_string(),
        "ant 4 pasch" => "compline-gospel-antiphon-easter".to_string(),
        other => slug(other),
    }
}

fn canonical_prime_ordinary_slot(section: &str) -> String {
    match normalize_section_name(section).as_str() {
        "hymnus prima" => "prime-hymn".to_string(),
        "responsory" => "prime-short-responsory".to_string(),
        "versum" => "prime-versicle".to_string(),
        other => slug(other),
    }
}

fn formula_role(section: &str) -> &'static str {
    let lower = normalize_section_name(section);
    if lower.contains("benedictio") {
        "blessing"
    } else if lower.contains("oratio") {
        "collect"
    } else if lower.contains("preces") {
        "preces"
    } else {
        "rubric"
    }
}

fn ordinary_major_role(section: &str) -> &'static str {
    let lower = normalize_section_name(section);
    if lower.contains("hymnus") {
        "hymn"
    } else if lower.contains("responsory") {
        "short_responsory"
    } else if lower.contains("versum") {
        "versicle"
    } else if lower.contains("ant") {
        "antiphon"
    } else {
        "chapter"
    }
}

fn ordinary_minor_role(section: &str) -> &'static str {
    let lower = normalize_section_name(section);
    if lower.contains("hymnus") {
        "hymn"
    } else if lower.contains("responsory") {
        "short_responsory"
    } else if lower.contains("versum") {
        "versicle"
    } else if lower.contains("ant") {
        "antiphon"
    } else if lower.contains("lectio") {
        "short_reading"
    } else {
        "chapter"
    }
}

fn ordinary_prime_role(section: &str) -> &'static str {
    let lower = normalize_section_name(section);
    if lower.contains("hymnus") {
        "hymn"
    } else if lower.contains("responsory") {
        "short_responsory"
    } else if lower.contains("versum") {
        "versicle"
    } else {
        "short_reading"
    }
}

fn ordinary_matins_role(section: &str) -> &'static str {
    let lower = normalize_section_name(section);
    if lower.contains("hymnus") {
        "hymn"
    } else if lower.contains("invit") {
        "invitatory"
    } else {
        "note"
    }
}

fn slot(key: &str, role: &str, priority: i32) -> CanonicalSlot {
    CanonicalSlot {
        key: slug(key),
        role: role.to_string(),
        priority,
    }
}

fn canonicalize_content(language: &str, items: Vec<ContentItem>) -> Vec<ContentItem> {
    let mut output = Vec::new();
    for item in items {
        match item {
            ContentItem::Text { text } => output.extend(
                semantic_content_lines(language, &text)
                    .into_iter()
                    .filter(|item| !is_import_artifact(item)),
            ),
            item => match clean_content_item(language, item) {
                ContentItem::Rubric { text } if is_import_artifact_text(&text) => {}
                ContentItem::Marker { text } if is_import_artifact_text(&text) => {}
                ContentItem::Marker { text } if looks_like_citation(&text) => {
                    output.push(ContentItem::Citation { text })
                }
                ContentItem::Marker { text } => output.push(ContentItem::Heading { text }),
                ContentItem::TableRow {
                    label,
                    text,
                    psalms,
                } => output.push(ContentItem::TableRow {
                    label: canonical_table_label(&label),
                    text,
                    psalms,
                }),
                ContentItem::Psalmody { antiphon, psalms } => {
                    output.push(ContentItem::Psalmody { antiphon, psalms })
                }
                other => output.push(other),
            },
        }
    }
    output
}

fn clean_content_item(language: &str, item: ContentItem) -> ContentItem {
    match item {
        ContentItem::Text { text } => ContentItem::Text {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Heading { text } => ContentItem::Heading {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Citation { text } => ContentItem::Citation {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Versicle { text } => ContentItem::Versicle {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Response { text } => ContentItem::Response {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::ShortResponse { text } => ContentItem::ShortResponse {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Prayer { text } => ContentItem::Prayer {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Blessing { text } => ContentItem::Blessing {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Antiphon { text } => ContentItem::Antiphon {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Rubric { text } => ContentItem::Rubric {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Marker { text } => ContentItem::Marker {
            text: clean_canonical_text(language, &text),
        },
        ContentItem::Psalmody { antiphon, psalms } => ContentItem::Psalmody {
            antiphon: clean_canonical_text(language, &antiphon),
            psalms,
        },
        ContentItem::TableRow {
            label,
            text,
            psalms,
        } => ContentItem::TableRow {
            label: clean_canonical_text(language, &label),
            text: text.map(|text| clean_canonical_text(language, &text)),
            psalms,
        },
        other @ ContentItem::PsalmRef { .. } => other,
        other @ ContentItem::Rank { .. } => other,
        other @ ContentItem::Rule { .. } => other,
    }
}

fn normalize_record_content(source_key: &str, slot_key: &str, content: &mut Vec<ContentItem>) {
    if source_key == "ordinary/formulae" && slot_key == "benedictio-completorium" {
        content.retain(|item| !is_blessing_request(item));
    }
}

fn is_blessing_request(item: &ContentItem) -> bool {
    let ContentItem::Versicle { text } = item else {
        return false;
    };
    let normalized = strip_accents_ascii(text).to_ascii_lowercase();
    normalized.contains("iube") && normalized.contains("benedicere")
        || normalized.contains("jube") && normalized.contains("benedicere")
        || normalized.contains("grant") && normalized.contains("blessing")
}

fn strip_accents_ascii(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'â' | 'ä' | 'ā' | 'ă' | 'ą' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ā' | 'Ă' | 'Ą' => {
                'a'
            }
            'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'É' | 'È' | 'Ê' | 'Ë' | 'Ē'
            | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'ī' | 'ĭ' | 'Í' | 'Ì' | 'Î' | 'Ï' | 'Ī' | 'Ĭ' => {
                'i'
            }
            'ó' | 'ò' | 'ô' | 'ö' | 'ō' | 'ŏ' | 'ő' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Ō' | 'Ŏ' | 'Ő' => {
                'o'
            }
            'ú' | 'ù' | 'û' | 'ü' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ū' | 'Ŭ'
            | 'Ů' | 'Ű' => 'u',
            'ý' | 'ÿ' | 'Ý' => 'y',
            'æ' | 'Æ' => 'e',
            other => other,
        })
        .collect()
}

fn semantic_content_lines(language: &str, text: &str) -> Vec<ContentItem> {
    let mut output = Vec::new();
    for raw in text.lines() {
        let line = clean_canonical_text(language, raw);
        if line.is_empty() {
            output.push(ContentItem::Text {
                text: String::new(),
            });
        } else if let Some(rest) = strip_prefix(&line, "R.br.") {
            output.push(ContentItem::ShortResponse { text: rest });
        } else if let Some(rest) = strip_prefix(&line, "R/.").or_else(|| strip_prefix(&line, "R."))
        {
            output.push(ContentItem::Response { text: rest });
        } else if let Some(rest) = strip_prefix(&line, "V/.").or_else(|| strip_prefix(&line, "V."))
        {
            push_versicle_with_optional_response(&mut output, rest);
        } else if let Some(rest) = strip_prefix(&line, "Ant.") {
            output.push(ContentItem::Antiphon {
                text: normalize_antiphon(&rest),
            });
        } else if let Some(rest) =
            strip_prefix(&line, "Benedictio.").or_else(|| strip_prefix(&line, "Benediction."))
        {
            output.push(ContentItem::Blessing { text: rest });
        } else if let Some(rest) = strip_prefix(&line, "v.") {
            output.push(ContentItem::Text { text: rest });
        } else if let Some(rest) = strip_prefix(&line, "r.") {
            output.push(ContentItem::Prayer { text: rest });
        } else if looks_like_citation(&line) {
            output.push(ContentItem::Citation { text: line });
        } else {
            output.push(ContentItem::Text { text: line });
        }
    }
    output
}

fn push_versicle_with_optional_response(output: &mut Vec<ContentItem>, text: String) {
    let Some((versicle, response)) = split_embedded_response(&text) else {
        output.push(ContentItem::Versicle { text });
        return;
    };
    output.push(ContentItem::Versicle { text: versicle });
    output.push(ContentItem::Response { text: response });
}

fn split_embedded_response(text: &str) -> Option<(String, String)> {
    for marker in [" R/. ", " R. "] {
        if let Some((versicle, response)) = text.split_once(marker) {
            let versicle = versicle.trim();
            let response = response.trim();
            if !versicle.is_empty() && !response.is_empty() {
                return Some((versicle.to_string(), response.to_string()));
            }
        }
    }
    None
}

fn is_import_artifact(item: &ContentItem) -> bool {
    match item {
        ContentItem::Text { text }
        | ContentItem::Rubric { text }
        | ContentItem::Heading { text }
        | ContentItem::Citation { text }
        | ContentItem::Versicle { text }
        | ContentItem::Response { text }
        | ContentItem::ShortResponse { text }
        | ContentItem::Prayer { text }
        | ContentItem::Blessing { text }
        | ContentItem::Antiphon { text }
        | ContentItem::Marker { text } => is_import_artifact_text(text),
        ContentItem::PsalmRef { .. }
        | ContentItem::Psalmody { .. }
        | ContentItem::TableRow { .. } => false,
        ContentItem::Rank { .. } | ContentItem::Rule { .. } => false,
    }
}

fn is_import_artifact_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("migrated reference")
        || lower.contains("migrated command")
        || lower.contains("cyclic migrated")
        || lower.contains("original regex")
}

fn clean_canonical_text(language: &str, text: &str) -> String {
    let mut output = normalize_space(text);
    output = strip_inline_legacy_transform(&output);
    output = remove_braced_source_markers(&output);
    output = output.replace("<FONT COLOR=red>*</FONT>", "*");
    output = output.replace("<font color=red>*</font>", "*");
    output = strip_html_tags(&output);
    output = decode_html_entities(&output);
    // Keep the raw cross markers (`+`, `++`, `+++`) so the renderer can style
    // them itself (special glyph, colour, …); do not bake in `✠`/`✙` here.
    output = output.replace('_', " ");
    output = output.replace("/:", "(").replace(":/", ")");
    while output.contains("((") || output.contains("))") {
        output = output.replace("((", "(").replace("))", ")");
    }
    if language == "la" {
        output = normalize_latin_orthography(&output);
    }
    normalize_space(&output)
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&#8213;", "—")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

fn normalize_latin_orthography(input: &str) -> String {
    input
        .replace('J', "I")
        .replace('j', "i")
        .replace("Jube", "Iube")
        .replace("jube", "iube")
        .replace("Adj", "Adi")
        .replace("adj", "adi")
        .replace("adju", "adiu")
        .replace("Joá", "Ioá")
        .replace("Jo", "Io")
        .replace("jo", "io")
        .replace("Jes", "Ies")
        .replace("jes", "ies")
        .replace("cujus", "cuius")
        .replace("Cujus", "Cuius")
        .replace("ejus", "eius")
        .replace("Ej", "Ei")
        .replace("eúmdem", "eúndem")
}

fn strip_html_tags(input: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

fn strip_inline_legacy_transform(line: &str) -> String {
    if let Some(index) = line.find(" s/$/") {
        line[..index].trim_end().to_string()
    } else {
        line.to_string()
    }
}

fn remove_braced_source_markers(input: &str) -> String {
    let mut output = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("{:") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find(":}") {
            rest = &after_start[end + 2..];
        } else {
            output.push_str(&rest[start..]);
            return output;
        }
    }
    output.push_str(rest);
    output
}

fn strip_prefix(line: &str, prefix: &str) -> Option<String> {
    line.strip_prefix(prefix)
        .map(str::trim)
        .filter(|rest| !rest.is_empty())
        .map(ToOwned::to_owned)
}

fn canonical_table_label(label: &str) -> String {
    match normalize_space(label).as_str() {
        "Dominica" => "sunday",
        "Feria II" => "monday",
        "Feria III" => "tuesday",
        "Feria IV" => "wednesday",
        "Feria V" => "thursday",
        "Feria VI" => "friday",
        "Sabbato" | "Sabato" => "saturday",
        other => other,
    }
    .to_string()
}

fn looks_like_citation(text: &str) -> bool {
    let text = text.trim();
    if text.starts_with("Psalmus ") || text.starts_with("Psalm ") {
        return false;
    }
    text.chars().any(|ch| ch.is_ascii_digit()) && text.contains(':')
}

fn fill_common_inheritance(
    bundles: &mut BTreeMap<BundleKey, BTreeMap<String, PrioritizedTextRecord>>,
) {
    let referenced_stems = referenced_common_stems(bundles);
    for (key, records) in bundles.iter_mut() {
        if key.category != "commons" {
            continue;
        }
        let mut ids = records.keys().cloned().collect::<Vec<_>>();
        let mut stems = ids
            .iter()
            .filter_map(|id| id.rsplit_once('.').map(|(source, _)| source))
            .filter_map(|source| source.strip_prefix("common."))
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>();
        stems.extend(referenced_stems.iter().cloned());
        let mut inherited = Vec::new();
        for stem in stems {
            for fallback in common_fallback_stems(&stem) {
                let fallback_prefix = format!("common.{fallback}.");
                let fallback_records = ids
                    .iter()
                    .filter(|id| id.starts_with(&fallback_prefix))
                    .cloned()
                    .collect::<Vec<_>>();
                for fallback_id in fallback_records {
                    let Some(slot) = fallback_id.strip_prefix(&fallback_prefix) else {
                        continue;
                    };
                    let target_id = format!("common.{stem}.{slot}");
                    if records.contains_key(&target_id)
                        || inherited
                            .iter()
                            .any(|record: &TextRecord| record.id == target_id)
                    {
                        continue;
                    }
                    let Some(fallback_record) = records.get(&fallback_id) else {
                        continue;
                    };
                    let mut record = fallback_record.record.clone();
                    record.id = target_id;
                    inherited.push(record);
                }
            }
        }
        for record in inherited {
            if !records.contains_key(&record.id) {
                ids.push(record.id.clone());
            }
            records
                .entry(record.id.clone())
                .or_insert(PrioritizedTextRecord {
                    priority: -100,
                    record,
                });
        }
    }
}

fn structural_aliases_from_primary_language(
    corpus: &SourceCorpus,
    sanctoral_replacements: &BTreeSet<String>,
) -> Vec<StructuralAlias> {
    let mut aliases = Vec::new();
    let mut seen = BTreeSet::new();
    for file in corpus.files.values() {
        if file.language != "la" {
            continue;
        }
        let Some(source) = canonical_source(file, corpus, sanctoral_replacements) else {
            continue;
        };
        for section in &file.sections {
            let Some(target_slot) = canonical_slot(&source.key, &file.source_path, &section.name)
            else {
                continue;
            };
            let Some(include) = single_section_include(&section.body) else {
                continue;
            };
            let Some((source_key, source_bundle)) =
                include_reference_source(include.target.file.as_deref(), &source.key)
            else {
                continue;
            };
            let included_section = include
                .target
                .section
                .as_deref()
                .unwrap_or(section.name.as_str());
            let Some(source_slot) = canonical_slot(&source_key, "", included_section) else {
                continue;
            };
            let alias = StructuralAlias {
                target_bundle: source.bundle.clone(),
                target_id: format!("{}.{}", source.key.replace('/', "."), target_slot.key),
                target_role: target_slot.role,
                source_bundle,
                source_id: format!("{}.{}", source_key.replace('/', "."), source_slot.key),
            };
            if seen.insert((alias.target_id.clone(), alias.source_id.clone())) {
                aliases.push(alias);
            }
        }
    }
    aliases
}

fn fill_structural_aliases(
    bundles: &mut BTreeMap<BundleKey, BTreeMap<String, PrioritizedTextRecord>>,
    aliases: &[StructuralAlias],
) {
    let languages = bundles
        .keys()
        .map(|key| key.language.clone())
        .collect::<BTreeSet<_>>();
    for language in languages {
        for alias in aliases {
            let source_key = BundleKey {
                language: language.clone(),
                category: alias.source_bundle.clone(),
            };
            let Some(mut record) = bundles
                .get(&source_key)
                .and_then(|records| records.get(&alias.source_id))
                .map(|record| record.record.clone())
            else {
                continue;
            };
            let target_key = BundleKey {
                language: language.clone(),
                category: alias.target_bundle.clone(),
            };
            let Some(target) = bundles.get_mut(&target_key) else {
                continue;
            };
            if target.contains_key(&alias.target_id) {
                continue;
            }
            record.id = alias.target_id.clone();
            record.role = alias.target_role.clone();
            target.insert(
                record.id.clone(),
                PrioritizedTextRecord {
                    priority: -90,
                    record,
                },
            );
        }
    }
}

fn single_section_include(body: &str) -> Option<IncludeSpec> {
    let mut lines = body.lines().map(str::trim).filter(|line| !line.is_empty());
    let line = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    line.strip_prefix('@').map(parse_include)
}

fn include_reference_source(
    file: Option<&str>,
    current_source_key: &str,
) -> Option<(String, String)> {
    let file = file?;
    let file = file.strip_suffix(".txt").unwrap_or(file);
    if let Some(common) = file.strip_prefix("Commune/") {
        return Some((format!("common/{}", slug(common)), "commons".to_string()));
    }
    if let Some(temporal) = file.strip_prefix("Tempora/") {
        return Some((
            format!("proper/temporal/{}", slug(temporal)),
            "propers-temporal".to_string(),
        ));
    }
    if let Some(sanctoral) = file.strip_prefix("Sancti/") {
        return Some((
            format!("proper/sanctoral/{}", slug(sanctoral)),
            "propers-sanctoral".to_string(),
        ));
    }
    if looks_like_common_stem(file) {
        return Some((format!("common/{}", slug(file)), "commons".to_string()));
    }
    if let Some(current_common) = current_source_key.strip_prefix("common/") {
        let _ = current_common;
        return Some((format!("common/{}", slug(file)), "commons".to_string()));
    }
    None
}

fn referenced_common_stems(
    bundles: &BTreeMap<BundleKey, BTreeMap<String, PrioritizedTextRecord>>,
) -> BTreeSet<String> {
    let mut stems = BTreeSet::new();
    for records in bundles.values() {
        for record in records.values() {
            for item in &record.record.content {
                collect_common_references(item, &mut stems);
            }
        }
    }
    stems
}

fn collect_common_references(item: &ContentItem, stems: &mut BTreeSet<String>) {
    match item {
        ContentItem::Rank {
            common: Some(common),
            ..
        } => {
            if let Some(stem) = common_stem_from_reference(common) {
                stems.insert(stem);
            }
        }
        ContentItem::Rule { tokens } => {
            for token in tokens {
                if let RuleToken::SourceRef { target, .. } = token {
                    if let Some(stem) = common_stem_from_reference(target) {
                        stems.insert(stem);
                    }
                }
            }
        }
        _ => {}
    }
}

fn common_stem_from_reference(reference: &str) -> Option<String> {
    let reference = reference
        .trim()
        .strip_suffix(".txt")
        .unwrap_or(reference.trim());
    reference
        .strip_prefix("Commune/")
        .or_else(|| reference.strip_prefix("common/"))
        .or_else(|| reference.strip_prefix("common."))
        .or_else(|| looks_like_common_stem(reference).then_some(reference))
        .map(slug)
}

fn looks_like_common_stem(reference: &str) -> bool {
    let mut chars = reference.chars();
    matches!(chars.next(), Some('C' | 'c')) && chars.next().is_some_and(|ch| ch.is_ascii_digit())
}

fn common_fallback_stems(stem: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut current = stem.to_string();
    loop {
        let Some(next) = common_fallback_stem(&current) else {
            break;
        };
        if output.iter().any(|existing| existing == &next) {
            break;
        }
        current = next.clone();
        output.push(next);
    }
    output
}

fn common_fallback_stem(stem: &str) -> Option<String> {
    let base = stem.trim_end_matches(|ch: char| ch.is_ascii_lowercase());
    if !base.is_empty() && base.len() != stem.len() {
        return Some(base.trim_end_matches('-').to_string());
    }
    if let Some((head, tail)) = stem.rsplit_once('-') {
        if tail.chars().all(|ch| ch.is_ascii_digit()) {
            return Some(head.to_string());
        }
    }
    None
}

fn normalize_bundles(
    bundles: BTreeMap<BundleKey, BTreeMap<String, PrioritizedTextRecord>>,
) -> Result<NormalizedYaml, String> {
    let mut sources = BTreeMap::<String, SourceYaml>::new();
    let mut source_categories = BTreeMap::<String, String>::new();
    let mut sections = BTreeMap::<(String, String), SectionAccumulator>::new();

    for (bundle_key, records) in bundles {
        for prioritized in records.into_values() {
            let record = prioritized.record;
            let Some((source_key, section_key)) = source_and_section_from_record_id(&record.id)
            else {
                continue;
            };
            source_categories
                .entry(source_key.clone())
                .or_insert_with(|| bundle_key.category.clone());

            if let Some(metadata) = metadata_from_record(&record.content) {
                let source = sources.entry(source_key).or_default();
                source.metadata.merge(metadata);
                continue;
            }

            let key = (source_key, section_key);
            let section = sections.entry(key).or_insert_with(|| SectionAccumulator {
                role: record.role.clone(),
                content: BTreeMap::new(),
            });
            section.role = prefer_role(&section.role, &record.role);
            section
                .content
                .insert(bundle_key.language.clone(), record.content);
        }
    }

    let mut corpus = BTreeMap::<String, BTreeMap<String, CorpusRecordYaml>>::new();
    let mut dedupe = BTreeMap::<String, String>::new();
    let mut corpus_category = BTreeMap::<String, String>::new();

    for ((source_key, section_key), section) in sections {
        if section.content.is_empty() {
            continue;
        }
        let category = source_categories
            .get(&source_key)
            .cloned()
            .unwrap_or_else(|| source_category(&source_key));
        let fingerprint = corpus_fingerprint(&section.role, &section.content)?;
        let text_id = if let Some(existing) = dedupe.get(&fingerprint) {
            existing.clone()
        } else {
            let text_id = corpus_text_id(&section.role, &section.content, &fingerprint);
            dedupe.insert(fingerprint, text_id.clone());
            corpus_category.insert(text_id.clone(), category.clone());
            corpus.entry(category.clone()).or_default().insert(
                text_id.clone(),
                CorpusRecordYaml {
                    role: section.role.clone(),
                    content: section.content,
                },
            );
            text_id
        };
        let source = sources.entry(source_key.clone()).or_default();
        source.sections.insert(
            section_key,
            SourceSectionYaml {
                role: section.role,
                text_id,
            },
        );
        source_categories.entry(source_key).or_insert(category);
    }

    let mut sources_by_category = BTreeMap::<String, BTreeMap<String, SourceYaml>>::new();
    for (source_key, source) in sources {
        let category = source_categories
            .get(&source_key)
            .cloned()
            .or_else(|| {
                source
                    .sections
                    .values()
                    .find_map(|section| corpus_category.get(&section.text_id).cloned())
            })
            .unwrap_or_else(|| source_category(&source_key));
        sources_by_category
            .entry(category)
            .or_default()
            .insert(source_key, source);
    }

    Ok(NormalizedYaml {
        corpus,
        sources: sources_by_category,
    })
}

/// Emits the new books + lexicon schema (the runtime contract in
/// `breviarium_data::schema`). `lexicon/{category}.yaml` carries the same
/// deduped multilingual texts as the corpus; `books/{book}.yaml` regroups the
/// sources by liturgical book (temporal/sanctoral/commons/psalter/ordinary/
/// martyrology), each office reduced to rank/flags/values/common + canonical
/// slot → text-id. Returns the number of files written.
fn emit_books_lexicon(normalized: &NormalizedYaml, data_dir: &Path) -> Result<usize, String> {
    let mut written = 0;
    for (category, records) in &normalized.corpus {
        // Fully type/clean each text here, at import time, so the lexicon stores
        // semantic nodes and the runtime never re-parses strings.
        let typed: BTreeMap<String, CorpusRecordYaml> = records
            .iter()
            .map(|(id, record)| {
                (
                    id.clone(),
                    CorpusRecordYaml {
                        role: record.role.clone(),
                        content: record
                            .content
                            .iter()
                            .map(|(language, items)| {
                                (language.clone(), type_content(language, items))
                            })
                            .collect(),
                    },
                )
            })
            .collect();
        let path = data_dir
            .join("lexicon")
            .join(format!("{}.yaml", slug(category)));
        let yaml = yaml_serde::to_string(&LexiconFileOut { texts: &typed })
            .map_err(|error| format!("failed to serialize lexicon: {error}"))?;
        fs::write(&path, yaml).map_err(|error| format!("{}: {error}", path.display()))?;
        written += 1;
    }

    let mut books = BTreeMap::<String, BTreeMap<String, OfficeOut>>::new();
    for sources in normalized.sources.values() {
        for (source_key, source) in sources {
            let (book, office_key) = book_and_office(source_key);
            books
                .entry(book)
                .or_default()
                .insert(office_key, office_out(source_key, source));
        }
    }
    if let Some(ordinary) = books.get_mut("ordinary") {
        split_ordinary_major(ordinary);
        split_ordinary_minor(ordinary);
        split_ordinary_prime(ordinary);
        split_ordinary_compline(ordinary);
        split_ordinary_matins(ordinary);
    }
    for (book, offices) in &books {
        let path = data_dir.join("books").join(format!("{book}.yaml"));
        let yaml = yaml_serde::to_string(&BookFileOut { offices })
            .map_err(|error| format!("failed to serialize book `{book}`: {error}"))?;
        fs::write(&path, yaml).map_err(|error| format!("{}: {error}", path.display()))?;
        written += 1;
    }
    Ok(written)
}

/// Splits the flat `ordinary/major` office (whose sections encode
/// selector+role in DO names like `dominica-laudes`, `hymnus-adv-vespera`) into
/// canonical season/day/selector offices keyed by canonical slots, so the
/// resolver fills the major chapter-hymn-verse block by plain stack lookup.
/// Sections the resolver never consults are left in the `major` office.
fn split_ordinary_major(offices: &mut BTreeMap<String, OfficeOut>) {
    let Some(major) = offices.remove("major") else {
        return;
    };
    let hour = |do_hour: &str| match do_hour {
        "laudes" => Some("lauds"),
        "vespera" => Some("vespers"),
        _ => None,
    };
    let mut new = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut put = |office: String, slot: &str, id: String| {
        new.entry(office).or_default().insert(slot.to_string(), id);
    };
    let mut leftover = OfficeOut::default();

    for (key, id) in major.slots {
        let mapped = (|| {
            // {sel}-laudes / {sel}-vespera  -> major-{sel}.{hour}-chapter
            for sel in ["dominica", "feria"] {
                for do_hour in ["laudes", "vespera"] {
                    if key == format!("{sel}-{do_hour}") {
                        let h = hour(do_hour)?;
                        let office = if sel == "dominica" {
                            "major-sunday"
                        } else {
                            "major-feria"
                        };
                        return Some((office.to_string(), format!("{h}-chapter")));
                    }
                    if key == format!("responsory-{sel}-{do_hour}") {
                        let h = hour(do_hour)?;
                        let office = if sel == "dominica" {
                            "major-sunday-2"
                        } else {
                            "major-feria-2"
                        };
                        return Some((office.to_string(), format!("{h}-chapter")));
                    }
                }
                // {sel}-versum-2 -> lauds-versicle ; {sel}-versum-3 -> vespers-versicle
                if key == format!("{sel}-versum-2") {
                    let office = if sel == "dominica" {
                        "major-sunday"
                    } else {
                        "major-feria"
                    };
                    return Some((office.to_string(), "lauds-versicle".to_string()));
                }
                if key == format!("{sel}-versum-3") {
                    let office = if sel == "dominica" {
                        "major-sunday"
                    } else {
                        "major-feria"
                    };
                    return Some((office.to_string(), "vespers-versicle".to_string()));
                }
            }
            // hymns: hymnus-{season}-{hour}, hymnusm-{season}-{hour},
            // hymnus-day{n}-{hour}, hymnusm-day6-vespera
            for (prefix, monastic) in [("hymnus-", false), ("hymnusm-", true)] {
                if let Some(rest) = key.strip_prefix(prefix) {
                    for do_hour in ["laudes", "vespera"] {
                        if let Some(mid) = rest.strip_suffix(&format!("-{do_hour}")) {
                            let h = hour(do_hour)?;
                            // mid is a season (adv/quad/quad5/pasch) or day{n}
                            let is_known = matches!(mid, "adv" | "quad" | "quad5" | "pasch")
                                || mid.starts_with("day");
                            if !is_known {
                                return None;
                            }
                            let office = if monastic {
                                format!("major-monastic-{mid}")
                            } else {
                                format!("major-{mid}")
                            };
                            return Some((office, format!("{h}-hymn")));
                        }
                    }
                }
            }
            // gospel-canticle antiphons: {sel}-ant-2 -> lauds, -ant-3 -> vespers
            for n in 0..=7u32 {
                let sel = if n == 0 {
                    "dominica".to_string()
                } else {
                    format!("feria{n}")
                };
                if key == format!("{sel}-ant-2") {
                    return Some((
                        format!("major-{sel}-ant"),
                        "lauds-gospel-antiphon".to_string(),
                    ));
                }
                if key == format!("{sel}-ant-3") {
                    return Some((
                        format!("major-{sel}-ant"),
                        "vespers-gospel-antiphon".to_string(),
                    ));
                }
            }
            None
        })();
        match mapped {
            Some((office, slot)) => put(office, &slot, id),
            None => {
                leftover.slots.insert(key, id);
            }
        }
    }

    if !leftover.slots.is_empty() {
        offices.insert("major".to_string(), leftover);
    }
    for (office, slots) in new {
        offices.insert(
            office,
            OfficeOut {
                slots,
                ..OfficeOut::default()
            },
        );
    }
}

/// Splits the flat `ordinary/minor` office (DO names like `feria-tertia`,
/// `responsory-breve-quad-sexta`, `versum-pasch-nona`) into canonical
/// `minor-{season}` offices keyed by `{hour}-chapter` / `{hour}-short-responsory`
/// / `{hour}-versicle`, plus `minor-hymn` for the daytime hymns. Compline and
/// unused variant sections stay in the `minor` office.
fn split_ordinary_minor(offices: &mut BTreeMap<String, OfficeOut>) {
    let Some(minor) = offices.remove("minor") else {
        return;
    };
    const SEASONS: &[&str] = &["dominica", "feria", "adv", "quad", "quad5", "pasch"];
    // (DO hour name in the section key, canonical hour used in slot names)
    const HOURS: &[(&str, &str)] = &[("tertia", "terce"), ("sexta", "sext"), ("nona", "none")];
    let mut new = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut put = |office: String, slot: String, id: String| {
        new.entry(office).or_default().insert(slot, id);
    };
    let mut leftover = OfficeOut::default();

    for (key, id) in minor.slots {
        let mapped = (|| {
            for (do_hour, hour) in HOURS {
                if key == format!("hymnus-{do_hour}") {
                    return Some(("minor-hymn".to_string(), format!("{hour}-hymn")));
                }
                for season in SEASONS {
                    if key == format!("{season}-{do_hour}") {
                        return Some((format!("minor-{season}"), format!("{hour}-chapter")));
                    }
                    if key == format!("responsory-breve-{season}-{do_hour}") {
                        return Some((
                            format!("minor-{season}"),
                            format!("{hour}-short-responsory"),
                        ));
                    }
                    if key == format!("versum-{season}-{do_hour}") {
                        return Some((format!("minor-{season}"), format!("{hour}-versicle")));
                    }
                }
            }
            None
        })();
        match mapped {
            Some((office, slot)) => put(office, slot, id),
            None => {
                leftover.slots.insert(key, id);
            }
        }
    }

    if !leftover.slots.is_empty() {
        offices.insert("minor".to_string(), leftover);
    }
    for (office, slots) in new {
        offices.insert(
            office,
            OfficeOut {
                slots,
                ..OfficeOut::default()
            },
        );
    }
}

/// Splits the flat `ordinary/prime` office into `prime-{sunday|feria}` (chapter),
/// `prime-{season}` (short-reading + seasonal responsory) and `prime-fixed`
/// (hymn, short-responsory, versicle). Unused sections stay in `prime`.
fn split_ordinary_prime(offices: &mut BTreeMap<String, OfficeOut>) {
    let Some(prime) = offices.remove("prime") else {
        return;
    };
    const SEASONS: &[&str] = &[
        "adv",
        "nat",
        "epi",
        "quad",
        "quad5",
        "pasch",
        "per-annum",
        "asc",
        "pent",
    ];
    let mut new = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut put = |office: String, slot: &str, id: String| {
        new.entry(office).or_default().insert(slot.to_string(), id);
    };
    let mut leftover = OfficeOut::default();

    for (key, id) in prime.slots {
        let mapped = match key.as_str() {
            "dominica" => Some(("prime-sunday".to_string(), "chapter")),
            "feria" => Some(("prime-feria".to_string(), "chapter")),
            "prime-hymn" => Some(("prime-fixed".to_string(), "hymn")),
            "prime-short-responsory" => Some(("prime-fixed".to_string(), "short-responsory")),
            "prime-versicle" => Some(("prime-fixed".to_string(), "versicle")),
            _ => {
                if let Some(season) = key.strip_prefix("responsory-") {
                    Some((format!("prime-{season}"), "seasonal-responsory"))
                } else if SEASONS.contains(&key.as_str()) {
                    Some((format!("prime-{key}"), "short-reading"))
                } else {
                    None
                }
            }
        };
        match mapped {
            Some((office, slot)) => put(office, slot, id),
            None => {
                leftover.slots.insert(key, id);
            }
        }
    }

    if !leftover.slots.is_empty() {
        offices.insert("prime".to_string(), leftover);
    }
    for (office, slots) in new {
        offices.insert(
            office,
            OfficeOut {
                slots,
                ..OfficeOut::default()
            },
        );
    }
}

/// Extracts the Compline sections that the `split_ordinary_minor` pass left in
/// the `minor` office into a `compline` office (chapter / short-responsory /
/// versicle / short-reading / the seasonal Nunc-dimittis antiphons) and
/// `compline-{season}` offices for the seasonal hymn.
fn split_ordinary_compline(offices: &mut BTreeMap<String, OfficeOut>) {
    let Some(minor) = offices.get_mut("minor") else {
        return;
    };
    let mut compline = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut keep = BTreeMap::<String, String>::new();
    for (key, id) in std::mem::take(&mut minor.slots) {
        let mapped = if key == "compline-chapter" {
            Some(("compline".to_string(), "chapter".to_string()))
        } else if key == "compline-short-responsory" {
            Some(("compline".to_string(), "short-responsory".to_string()))
        } else if key == "compline-versicle" {
            Some(("compline".to_string(), "versicle".to_string()))
        } else if key == "compline-short-reading" {
            Some(("compline".to_string(), "short-reading".to_string()))
        } else if let Some(rest) = key.strip_prefix("compline-gospel-antiphon") {
            let slot = format!("gospel-antiphon{rest}");
            Some(("compline".to_string(), slot))
        } else if key == "hymnus-completorium" {
            Some(("compline".to_string(), "hymn".to_string()))
        } else if let Some(season) = key.strip_prefix("hymnus-completorium-") {
            Some((format!("compline-{season}"), "hymn".to_string()))
        } else {
            None
        };
        match mapped {
            Some((office, slot)) => {
                compline.entry(office).or_default().insert(slot, id);
            }
            None => {
                keep.insert(key, id);
            }
        }
    }
    minor.slots = keep;
    if minor.slots.is_empty() {
        offices.remove("minor");
    }
    for (office, slots) in compline {
        offices.insert(
            office,
            OfficeOut {
                slots,
                ..OfficeOut::default()
            },
        );
    }
}

/// Splits `ordinary/matins` so the invitatory antiphon lives in a `matins`
/// office (slot `invitatory`) and the ordinary hymns in `matins-{season}` /
/// `matins-day{n}` offices (slot `hymn`). Unused variants stay in `matins`.
fn split_ordinary_matins(offices: &mut BTreeMap<String, OfficeOut>) {
    let Some(matins) = offices.remove("matins") else {
        return;
    };
    let mut new = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut put = |office: String, slot: &str, id: String| {
        new.entry(office).or_default().insert(slot.to_string(), id);
    };
    let mut leftover = OfficeOut::default();

    for (key, id) in matins.slots {
        let mapped = match key.as_str() {
            "invit" => Some(("matins".to_string(), "invitatory")),
            "hymnus-adv" => Some(("matins-adv".to_string(), "hymn")),
            "hymnus-quad" => Some(("matins-quad".to_string(), "hymn")),
            "hymnus-pasch" => Some(("matins-pasch".to_string(), "hymn")),
            _ => key
                .strip_suffix("-hymnus")
                .filter(|day| day.starts_with("day") && !key.contains("hymnusm"))
                .map(|day| (format!("matins-{day}"), "hymn")),
        };
        match mapped {
            Some((office, slot)) => put(office, slot, id),
            None => {
                leftover.slots.insert(key, id);
            }
        }
    }

    // The `matins` office collects both the invitatory and the leftover variants.
    for (slot, id) in leftover.slots {
        new.entry("matins".to_string())
            .or_default()
            .insert(slot, id);
    }
    for (office, slots) in new {
        offices.insert(
            office,
            OfficeOut {
                slots,
                ..OfficeOut::default()
            },
        );
    }
}

// ---- import-time typing + cleaning of lexicon content ----
//
// Ported from the old runtime `semantic_text_nodes` / `clean_*`. Running it here
// means the lexicon stores fully semantic, cleaned nodes and the runtime maps
// them 1:1 with no string re-parsing.

fn type_content(language: &str, items: &[ContentItem]) -> Vec<ContentItem> {
    let mut out = Vec::new();
    for item in items {
        match item {
            // A blank line (a stanza break) — kept so `coalesce_text` joins it
            // into the block as a blank line; `semantic_split("")` would drop it.
            ContentItem::Text { text } if text.is_empty() => out.push(ContentItem::Text {
                text: String::new(),
            }),
            ContentItem::Text { text } => out.extend(semantic_split(language, text)),
            ContentItem::Heading { text } => out.push(ContentItem::Heading {
                text: clean_text(text),
            }),
            ContentItem::Citation { text } => out.push(ContentItem::Citation {
                text: clean_text(text),
            }),
            ContentItem::Versicle { text } => out.push(ContentItem::Versicle {
                text: clean_text(text),
            }),
            ContentItem::Response { text } => out.push(ContentItem::Response {
                text: clean_text(text),
            }),
            ContentItem::ShortResponse { text } => out.push(ContentItem::ShortResponse {
                text: clean_text(text),
            }),
            ContentItem::Prayer { text } => out.push(ContentItem::Prayer {
                text: clean_text(text),
            }),
            ContentItem::Antiphon { text } => out.push(ContentItem::Antiphon {
                text: clean_antiphon(text),
            }),
            ContentItem::Blessing { text } => out.push(ContentItem::Blessing {
                text: clean_blessing(text),
            }),
            ContentItem::Rubric { text } => out.push(ContentItem::Rubric {
                text: clean_text(text),
            }),
            ContentItem::Marker { text } => out.push(marker_item(text)),
            ContentItem::Psalmody { antiphon, psalms } => out.push(ContentItem::Psalmody {
                antiphon: clean_antiphon(antiphon),
                psalms: psalms.clone(),
            }),
            ContentItem::TableRow {
                label,
                text,
                psalms,
            } => out.push(ContentItem::TableRow {
                label: clean_text(label),
                text: text.as_ref().map(|value| clean_antiphon(value)),
                psalms: psalms.clone(),
            }),
            other => out.push(other.clone()),
        }
    }
    coalesce_text(out)
}

/// Merges runs of consecutive `Text` items into a single multiline `Text` item
/// (joined by newlines). A hymn, whose source lines each became their own item,
/// thus becomes one block — with internal blank lines preserved as stanza
/// breaks — so it renders as one paragraph instead of one `<p>` per line.
fn coalesce_text(items: Vec<ContentItem>) -> Vec<ContentItem> {
    let mut out: Vec<ContentItem> = Vec::new();
    for item in items {
        match (out.last_mut(), &item) {
            (Some(ContentItem::Text { text: prev }), ContentItem::Text { text: cur }) => {
                prev.push('\n');
                prev.push_str(cur);
            }
            _ => out.push(item),
        }
    }
    out
}

fn semantic_split(language: &str, text: &str) -> Vec<ContentItem> {
    let mut nodes = Vec::new();
    for raw_line in text.lines() {
        let line = clean_text(raw_line);
        if line.trim().is_empty() {
            nodes.push(ContentItem::Text {
                text: String::new(),
            });
            continue;
        }
        if let Some((heading, citation)) = split_paren(&line) {
            nodes.push(ContentItem::Heading { text: heading });
            nodes.push(ContentItem::Citation { text: citation });
        } else if let Some(rest) = strip_role(&line, "R.br.") {
            nodes.push(ContentItem::ShortResponse { text: rest });
        } else if let Some(rest) = strip_role(&line, "R/.") {
            nodes.push(ContentItem::Response { text: rest });
        } else if let Some(rest) = strip_role(&line, "R.") {
            nodes.push(ContentItem::Response { text: rest });
        } else if let Some(rest) = strip_role(&line, "V/.") {
            nodes.push(ContentItem::Versicle { text: rest });
        } else if let Some(rest) = strip_role(&line, "V.") {
            nodes.push(ContentItem::Versicle { text: rest });
        } else if let Some(rest) = strip_role(&line, "Ant.") {
            nodes.push(ContentItem::Antiphon {
                text: clean_antiphon(&rest),
            });
        } else if let Some(rest) = strip_role(&line, "Benedictio.") {
            nodes.push(ContentItem::Blessing {
                text: clean_blessing(&rest),
            });
        } else if let Some(rest) = strip_role(&line, "Benediction.") {
            nodes.push(ContentItem::Blessing {
                text: clean_blessing(&rest),
            });
        } else if let Some(rest) = strip_role(&line, "v.") {
            nodes.push(classify(rest));
        } else if let Some(rest) = strip_role(&line, "r.") {
            nodes.push(ContentItem::Prayer { text: rest });
        } else if looks_like_cite(&line) {
            nodes.push(ContentItem::Citation { text: line });
        } else {
            nodes.push(classify(line));
        }
    }
    let _ = language;
    nodes
}

fn marker_item(text: &str) -> ContentItem {
    let text = clean_text(text);
    let lower = text.to_ascii_lowercase();
    if looks_like_cite(&text) {
        ContentItem::Citation { text }
    } else if lower.contains("omittitur") || lower == "omit" || lower.starts_with("skip ") {
        ContentItem::Rubric { text }
    } else {
        ContentItem::Heading { text }
    }
}

fn classify(line: String) -> ContentItem {
    if looks_like_cite(&line) {
        ContentItem::Citation { text: line }
    } else if line.eq_ignore_ascii_case("oremus.") || line.eq_ignore_ascii_case("let us pray.") {
        ContentItem::Prayer { text: line }
    } else {
        ContentItem::Text {
            text: clean_text(&line),
        }
    }
}

fn strip_role(line: &str, prefix: &str) -> Option<String> {
    line.strip_prefix(prefix)
        .map(str::trim)
        .filter(|rest| !rest.is_empty())
        .map(ToOwned::to_owned)
}

fn clean_antiphon(text: &str) -> String {
    let text = clean_text(text);
    text.strip_prefix("Ant. ")
        .unwrap_or(&text)
        .trim()
        .to_string()
}

fn clean_blessing(text: &str) -> String {
    let text = clean_text(text);
    text.strip_prefix("Benedictio.")
        .or_else(|| text.strip_prefix("Benediction."))
        .unwrap_or(&text)
        .trim()
        .to_string()
}

fn clean_text(text: &str) -> String {
    text.trim().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn split_paren(line: &str) -> Option<(String, String)> {
    let inner = line.strip_prefix('(')?.strip_suffix(')')?;
    let (heading, citation) = inner.split_once(" * ")?;
    Some((clean_text(heading), clean_text(citation)))
}

fn looks_like_cite(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.starts_with("Psalmus ") || text.starts_with("Psalm ") {
        return false;
    }
    let has_digit = text.chars().any(|ch| ch.is_ascii_digit());
    has_digit
        && (text.contains(':')
            || text.starts_with("Ier ")
            || text.starts_with("Jer ")
            || text.starts_with("Luc. ")
            || text.starts_with("Luke ")
            || text.starts_with("1 ")
            || text.starts_with("2 ")
            || text.starts_with("3 "))
}

/// Splits a flat source key into its (book, book-relative office key).
fn book_and_office(key: &str) -> (String, String) {
    const PREFIXES: &[(&str, &str)] = &[
        ("proper/temporal/", "temporal"),
        ("proper/sanctoral/", "sanctoral"),
        ("common/", "commons"),
        ("psalter/", "psalter"),
        ("ordinary/", "ordinary"),
        ("martyrology/", "martyrology"),
    ];
    for (prefix, book) in PREFIXES {
        if let Some(rest) = key.strip_prefix(prefix) {
            return ((*book).to_string(), rest.to_string());
        }
    }
    match key.split_once('/') {
        Some((book, rest)) => (book.to_string(), rest.to_string()),
        None => (key.to_string(), key.to_string()),
    }
}

/// Reduces a legacy `SourceYaml` to a books-schema office: rank, parsed
/// flags/values, the inherited common, and canonical slot → text-id.
/// Resolves a Divinum-Officium common reference (`Commune/C5`, `Sancti/01-06`,
/// `C2b`, or a bare relative stem) to a canonical `book/office` key, so the
/// checked-in YAML carries no DO reference syntax.
fn resolve_common(source_key: &str, reference: &str) -> String {
    let reference = reference.strip_suffix(".txt").unwrap_or(reference);
    if let Some(x) = reference.strip_prefix("Commune/") {
        format!("commons/{}", slug(x))
    } else if let Some(x) = reference.strip_prefix("Tempora/") {
        format!("temporal/{}", slug(x))
    } else if let Some(x) = reference.strip_prefix("Sancti/") {
        format!("sanctoral/{}", slug(x))
    } else if reference
        .strip_prefix('C')
        .and_then(|tail| tail.chars().next())
        .is_some_and(|ch| ch.is_ascii_digit())
    {
        format!("commons/{}", slug(reference))
    } else {
        let (book, _) = book_and_office(source_key);
        format!("{book}/{}", slug(reference))
    }
}

fn office_out(source_key: &str, source: &SourceYaml) -> OfficeOut {
    let mut flags = Vec::new();
    let mut values = BTreeMap::new();
    let mut common = None;
    for rule in &source.metadata.rules {
        match rule {
            RuleToken::Flag { id, .. } => flags.push(id.clone()),
            RuleToken::Value { key, value, .. } => {
                values.insert(key.clone(), value.clone());
            }
            // Last SourceRef wins, and a rule SourceRef takes precedence over
            // rank.common — matching the old `rule_maps` / `rule_source.or(rank_common)`.
            RuleToken::SourceRef { target, .. } => common = Some(target.clone()),
        }
    }
    let rank = source
        .metadata
        .rank
        .as_ref()
        .filter(|rank| rank.label.is_some() || rank.value.is_some())
        .map(|rank| RankOut {
            name: rank.label.clone(),
            value: rank.value,
        });
    if common.is_none() {
        if let Some(rank) = &source.metadata.rank {
            common = rank.common.clone();
        }
    }
    let common = common.map(|reference| resolve_common(source_key, &reference));
    let slots = source
        .sections
        .iter()
        .map(|(key, section)| (key.clone(), section.text_id.clone()))
        .collect();
    OfficeOut {
        rank,
        flags,
        values,
        common,
        slots,
    }
}

fn source_and_section_from_record_id(record_id: &str) -> Option<(String, String)> {
    let (source, section) = record_id.rsplit_once('.')?;
    Some((source.replace('.', "/"), section.to_string()))
}

fn metadata_from_record(content: &[ContentItem]) -> Option<SourceMetadataYaml> {
    let mut metadata = SourceMetadataYaml::default();
    for item in content {
        match item {
            ContentItem::Rank {
                label,
                value,
                common,
            } => {
                metadata.rank = Some(RankYaml {
                    label: label.clone(),
                    value: *value,
                    common: common.clone(),
                });
            }
            ContentItem::Rule { tokens } => {
                metadata.rules.extend(tokens.clone());
            }
            _ => return None,
        }
    }
    (!metadata.is_empty()).then_some(metadata)
}

fn prefer_role(current: &str, candidate: &str) -> String {
    if current == candidate || candidate == "rubric" {
        current.to_string()
    } else if current == "rubric" {
        candidate.to_string()
    } else {
        current.to_string()
    }
}

fn source_category(source_key: &str) -> String {
    match source_key.split('/').next().unwrap_or("misc") {
        "proper" => source_key.split('/').take(2).collect::<Vec<_>>().join("-"),
        "common" => "commons".to_string(),
        "ordinary" => "ordinary".to_string(),
        "psalter" => "psalter".to_string(),
        "psalm" => "psalms".to_string(),
        "martyrology" => "martyrology".to_string(),
        other => other.to_string(),
    }
}

fn corpus_fingerprint(
    role: &str,
    content: &BTreeMap<String, Vec<ContentItem>>,
) -> Result<String, String> {
    let mut material = String::new();
    material.push_str(role);
    material.push('\n');
    material.push_str(
        &yaml_serde::to_string(content)
            .map_err(|error| format!("failed to serialize corpus fingerprint: {error}"))?,
    );
    Ok(material)
}

fn corpus_text_id(
    role: &str,
    content: &BTreeMap<String, Vec<ContentItem>>,
    fingerprint: &str,
) -> String {
    let mut stem = content
        .get("la")
        .or_else(|| content.get("en"))
        .and_then(|items| items.iter().find_map(content_item_text))
        .map(slug)
        .filter(|slug| !slug.is_empty())
        .unwrap_or_else(|| "text".to_string());
    if stem.len() > 48 {
        stem.truncate(48);
        stem = stem.trim_end_matches('-').to_string();
    }
    format!("{}.{}-{}", slug(role), stem, short_hash(fingerprint))
}

fn content_item_text(item: &ContentItem) -> Option<&str> {
    match item {
        ContentItem::Text { text }
        | ContentItem::Heading { text }
        | ContentItem::Citation { text }
        | ContentItem::Versicle { text }
        | ContentItem::Response { text }
        | ContentItem::ShortResponse { text }
        | ContentItem::Prayer { text }
        | ContentItem::Blessing { text }
        | ContentItem::Antiphon { text }
        | ContentItem::Rubric { text }
        | ContentItem::Marker { text } => Some(text),
        ContentItem::Psalmody { antiphon, .. } => Some(antiphon),
        ContentItem::TableRow { text, .. } => text.as_deref(),
        ContentItem::PsalmRef { .. } | ContentItem::Rank { .. } | ContentItem::Rule { .. } => None,
    }
}

fn short_hash(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")[..10].to_string()
}

#[derive(Default)]
struct NormalizedYaml {
    corpus: BTreeMap<String, BTreeMap<String, CorpusRecordYaml>>,
    sources: BTreeMap<String, BTreeMap<String, SourceYaml>>,
}

struct SectionAccumulator {
    role: String,
    content: BTreeMap<String, Vec<ContentItem>>,
}

#[derive(Serialize)]
struct CorpusRecordYaml {
    role: String,
    content: BTreeMap<String, Vec<ContentItem>>,
}

// ---- new books + lexicon schema (see `breviarium_data::schema`) ----

#[derive(Serialize)]
struct LexiconFileOut<'a> {
    texts: &'a BTreeMap<String, CorpusRecordYaml>,
}

#[derive(Serialize)]
struct BookFileOut<'a> {
    offices: &'a BTreeMap<String, OfficeOut>,
}

#[derive(Default, Serialize)]
struct OfficeOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    rank: Option<RankOut>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    flags: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    values: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    common: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    slots: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct RankOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<f32>,
}

#[derive(Default, Serialize)]
struct SourceYaml {
    #[serde(skip_serializing_if = "SourceMetadataYaml::is_empty")]
    metadata: SourceMetadataYaml,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    sections: BTreeMap<String, SourceSectionYaml>,
}

#[derive(Serialize)]
struct SourceSectionYaml {
    role: String,
    text_id: String,
}

#[derive(Default, Serialize)]
struct SourceMetadataYaml {
    #[serde(skip_serializing_if = "Option::is_none")]
    rank: Option<RankYaml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rules: Vec<RuleToken>,
}

impl SourceMetadataYaml {
    fn is_empty(&self) -> bool {
        self.rank.is_none() && self.rules.is_empty()
    }

    fn merge(&mut self, other: Self) {
        if self.rank.is_none() {
            self.rank = other.rank;
        }
        for token in other.rules {
            if !self
                .rules
                .iter()
                .any(|existing| existing.key() == token.key())
            {
                self.rules.push(token);
            }
        }
    }
}

#[derive(Serialize)]
struct RankYaml {
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    common: Option<String>,
}

fn sanitize_yaml_text(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '\u{2028}' | '\u{2029}' => '\n',
            '\n' | '\t' => ch,
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect()
}

fn is_binary_asset(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "pdf" | "ico")
    )
}

fn semantic_service(service_root: &str, parts: &[&str]) -> &'static str {
    match service_root {
        "horas"
            if parts
                .get(2)
                .is_some_and(|part| part.starts_with("Martyrologium")) =>
        {
            "martyrology"
        }
        "horas" if parts.get(2) == Some(&"Necrologium") => "necrology",
        "horas" if parts.get(2) == Some(&"Appendix") => "appendix",
        "horas" => "office",
        "missa" => "mass",
        "Tabulae" => "tables",
        _ => "other",
    }
}

fn language_id(service_root: &str, parts: &[&str]) -> &'static str {
    if service_root == "Tabulae" {
        return "und";
    }
    match parts.get(1).copied().unwrap_or_default() {
        "Latin" => "la",
        "English" => "en",
        "Deutsch" => "de",
        "Espanol" => "es",
        "Francais" => "fr",
        "Italiano" => "it",
        "Nederlands" => "nl",
        "Polski" => "pl",
        "Portugues" => "pt",
        "Ukrainian" => "uk",
        "Vietnamice" => "vi",
        "Dansk" => "da",
        "Magyar" => "hu",
        "Hebrew" => "he",
        "Bohemice" => "bohemice",
        "Cesky-Schaller" => "cesky-schaller",
        "Polski-Newer" => "pl-newer",
        "Latin-Bea" => "la-bea",
        "Latin-gabc" => "la-gabc",
        "Ordinarium" => "ordinarium",
        "Help" => "help",
        _ => "und",
    }
}

fn divinum_language_dir(language: &str) -> &'static str {
    match language {
        "la" => "Latin",
        "en" => "English",
        "de" => "Deutsch",
        "es" => "Espanol",
        "fr" => "Francais",
        "it" => "Italiano",
        "nl" => "Nederlands",
        "pl" => "Polski",
        "pt" => "Portugues",
        "uk" => "Ukrainian",
        "vi" => "Vietnamice",
        "da" => "Dansk",
        "hu" => "Magyar",
        "he" => "Hebrew",
        "bohemice" => "Bohemice",
        "cesky-schaller" => "Cesky-Schaller",
        "pl-newer" => "Polski-Newer",
        "la-bea" => "Latin-Bea",
        "la-gabc" => "Latin-gabc",
        "ordinarium" => "Ordinarium",
        "help" => "Help",
        _ => "Latin",
    }
}

fn category(service_root: &str, parts: &[&str]) -> String {
    if service_root == "Tabulae" {
        parts.get(1).copied().unwrap_or("root").to_string()
    } else {
        parts.get(2).copied().unwrap_or("root").to_string()
    }
}

fn relative_to(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map_err(|error| format!("{} relative to {}: {error}", path.display(), root.display()))
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn slug(input: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            output.push('-');
            last_dash = true;
        }
    }
    output.trim_matches('-').to_string()
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct BundleKey {
    language: String,
    category: String,
}

#[derive(Clone, Debug)]
struct SourceFile {
    service: String,
    language: String,
    category: String,
    source_path: String,
    sections: Vec<Section>,
}

#[derive(Clone, Debug)]
struct SectionLocation {
    source_path: String,
    section_name: String,
}

#[derive(Clone, Debug)]
struct TextRecord {
    id: String,
    role: String,
    content: Vec<ContentItem>,
}

#[derive(Clone, Debug)]
struct PrioritizedTextRecord {
    priority: i32,
    record: TextRecord,
}

#[derive(Clone, Debug)]
struct CanonicalSource {
    key: String,
    bundle: String,
    priority: i32,
}

#[derive(Clone, Debug)]
struct CanonicalSlot {
    key: String,
    role: String,
    priority: i32,
}

#[derive(Clone, Debug)]
struct StructuralAlias {
    target_bundle: String,
    target_id: String,
    target_role: String,
    source_bundle: String,
    source_id: String,
}

#[derive(Clone, Debug)]
struct Section {
    name: String,
    qualifier: Option<String>,
    body: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SectionKind {
    Text,
    Psalmody,
    Table,
    Rank,
    Rule,
}

impl SectionKind {
    fn from_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if lower == "rank" {
            Self::Rank
        } else if lower == "rule" {
            Self::Rule
        } else if lower.starts_with("ant")
            || lower.starts_with("day")
            || lower == "prima"
            || lower == "tertia"
            || lower == "sexta"
            || lower == "nona"
            || lower == "completorium"
        {
            Self::Psalmody
        } else if lower == "tridentinum"
            || lower == "monastic"
            || lower == "monastic_"
            || lower == "cistercian_"
        {
            Self::Table
        } else {
            Self::Text
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentItem {
    Text {
        text: String,
    },
    Heading {
        text: String,
    },
    Citation {
        text: String,
    },
    Versicle {
        text: String,
    },
    Response {
        text: String,
    },
    ShortResponse {
        text: String,
    },
    Prayer {
        text: String,
    },
    Blessing {
        text: String,
    },
    Antiphon {
        text: String,
    },
    Rubric {
        text: String,
    },
    Marker {
        text: String,
    },
    PsalmRef {
        number: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        start: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        end: Option<String>,
        #[serde(skip_serializing_if = "is_false")]
        optional: bool,
    },
    Psalmody {
        antiphon: String,
        psalms: Vec<PsalmReference>,
    },
    TableRow {
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        psalms: Vec<PsalmReference>,
    },
    Rank {
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        common: Option<String>,
    },
    Rule {
        tokens: Vec<RuleToken>,
    },
}

#[derive(Clone, Debug, Serialize)]
struct PsalmReference {
    number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    optional: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RuleToken {
    Flag {
        id: String,
        label: String,
    },
    SourceRef {
        relation: String,
        target: String,
    },
    Value {
        key: String,
        value: String,
        label: String,
    },
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl RuleToken {
    fn key(&self) -> String {
        match self {
            Self::Flag { id, .. } => format!("flag:{id}"),
            Self::SourceRef { relation, target } => format!("source:{relation}:{target}"),
            Self::Value { key, value, .. } => format!("value:{key}:{value}"),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Flag { label, .. } => label.clone(),
            Self::SourceRef { relation, target } => format!("{relation} {target}"),
            Self::Value { label, .. } => label.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct IncludeSpec {
    target: IncludeTarget,
    selection: Option<TextSelection>,
    transforms: Vec<TextTransform>,
}

#[derive(Clone, Debug)]
struct IncludeTarget {
    file: Option<String>,
    section: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct TextSelection {
    start: usize,
    end: Option<usize>,
}

#[derive(Clone, Debug)]
struct TextTransform {
    pattern: String,
    replacement: String,
    flags: String,
}

#[derive(Default)]
struct ImportStats {
    imported_files: usize,
    lossy_decoded_files: usize,
    skipped_binary_assets: usize,
    generated_corpus_texts: usize,
    generated_source_sections: usize,
    generated_bundles: usize,
}
