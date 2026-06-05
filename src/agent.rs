//! Embedded agent skills for kagi-cli.

/// Metadata and source for an embedded skill.
pub struct EmbeddedSkill {
    pub name: &'static str,
    pub description: &'static str,
    pub source: &'static str,
}

/// Core skill name used by `kagi skills`.
pub const KAGI_SKILL: &str = "kagi";

const KAGI_SKILL_SOURCE: &str = include_str!("../skills/kagi/SKILL.md");

const SKILLS: &[EmbeddedSkill] = &[EmbeddedSkill {
    name: KAGI_SKILL,
    description: "Core CLI usage guide for Kagi search, Assistant, extraction, summarization, and account settings",
    source: KAGI_SKILL_SOURCE,
}];

/// Returns all embedded skills.
pub const fn skills() -> &'static [EmbeddedSkill] {
    SKILLS
}

/// Returns the requested embedded skill content with frontmatter removed.
pub fn skill_content(name: &str) -> Option<String> {
    skill_source(name).map(strip_frontmatter)
}

/// Returns the requested skill plus any embedded reference material.
///
/// Kagi currently ships only the core skill body. Keeping this separate from
/// `skill_content` preserves Harbor-compatible `skills get <name> --full`
/// semantics for future references/templates without changing the command API.
pub fn skill_full_content(name: &str) -> Option<String> {
    skill_content(name)
}

fn skill_source(name: &str) -> Option<&'static str> {
    SKILLS
        .iter()
        .find(|skill| skill.name == name)
        .map(|skill| skill.source)
}

/// Returns a stable locator for an embedded skill or the embedded skill root.
pub fn skill_locator(name: Option<&str>) -> Option<String> {
    match name {
        None => Some("embedded://skills".to_string()),
        Some(KAGI_SKILL) => Some(format!("embedded://skills/{KAGI_SKILL}")),
        Some(_) => None,
    }
}

fn strip_frontmatter(source: &'static str) -> String {
    let mut found_start = false;
    let mut frontmatter_ended = false;
    let mut output = Vec::new();

    for line in source.lines() {
        if frontmatter_ended {
            output.push(line);
            continue;
        }

        // Harbor supports skills wrapped in a ```skill fence. Kagi does not
        // currently emit that wrapper, but accepting it keeps the embedded
        // parser compatible with the reference shape.
        if line.starts_with("```") {
            continue;
        }

        if !found_start {
            if line == "---" {
                found_start = true;
            }
            continue;
        }

        if line == "---" {
            frontmatter_ended = true;
        }
    }

    if !frontmatter_ended {
        return source.trim().to_string();
    }

    output.join("\n").trim().to_string()
}
