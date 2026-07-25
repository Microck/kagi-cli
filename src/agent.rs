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
const KAGI_RESEARCH_SKILL_SOURCE: &str = include_str!("../skills/kagi-research/SKILL.md");
const KAGI_CONTENT_SKILL_SOURCE: &str = include_str!("../skills/kagi-content/SKILL.md");
const KAGI_ASSISTANT_SKILL_SOURCE: &str = include_str!("../skills/kagi-assistant/SKILL.md");
const KAGI_MONITORING_SKILL_SOURCE: &str = include_str!("../skills/kagi-monitoring/SKILL.md");
const KAGI_ACCOUNT_CONFIG_SKILL_SOURCE: &str =
    include_str!("../skills/kagi-account-config/SKILL.md");

const SKILLS: &[EmbeddedSkill] = &[
    EmbeddedSkill {
        name: KAGI_SKILL,
        description: "Route Kagi CLI tasks to the right embedded workflow skill",
        source: KAGI_SKILL_SOURCE,
    },
    EmbeddedSkill {
        name: "kagi-research",
        description: "Research a topic with Kagi Search, Quick Answer, News, and source follow-up",
        source: KAGI_RESEARCH_SKILL_SOURCE,
    },
    EmbeddedSkill {
        name: "kagi-content",
        description: "Extract, summarize, question, or translate web page content",
        source: KAGI_CONTENT_SKILL_SOURCE,
    },
    EmbeddedSkill {
        name: "kagi-assistant",
        description: "Run Kagi Assistant conversations, threads, attachments, and custom assistants",
        source: KAGI_ASSISTANT_SKILL_SOURCE,
    },
    EmbeddedSkill {
        name: "kagi-monitoring",
        description: "Build repeatable Kagi batch, watch, notification, and history workflows",
        source: KAGI_MONITORING_SKILL_SOURCE,
    },
    EmbeddedSkill {
        name: "kagi-account-config",
        description: "Configure Kagi authentication, profiles, lenses, bangs, redirects, and site preferences",
        source: KAGI_ACCOUNT_CONFIG_SKILL_SOURCE,
    },
];

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
        Some(name) if skill_source(name).is_some() => Some(format!("embedded://skills/{name}")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn embedded_skills_have_unique_names_and_descriptions() {
        let names: HashSet<_> = skills().iter().map(|skill| skill.name).collect();
        let descriptions: HashSet<_> = skills().iter().map(|skill| skill.description).collect();

        assert_eq!(names.len(), skills().len());
        assert_eq!(descriptions.len(), skills().len());
    }

    #[test]
    fn embedded_skill_registry_exposes_every_workflow() {
        let names: Vec<_> = skills().iter().map(|skill| skill.name).collect();

        assert_eq!(
            names,
            [
                "kagi",
                "kagi-research",
                "kagi-content",
                "kagi-assistant",
                "kagi-monitoring",
                "kagi-account-config",
            ]
        );

        for name in names {
            assert_eq!(
                skill_locator(Some(name)).as_deref(),
                Some(format!("embedded://skills/{name}").as_str())
            );
            assert!(
                skill_content(name).is_some_and(|content| content.starts_with("# Kagi")),
                "{name} should expose a frontmatter-free skill body"
            );
        }
    }
}
