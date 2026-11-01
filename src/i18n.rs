//! Explicit message catalogs. User paths and diagnostic details are never catalog keys.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Russian,
    French,
    German,
    Spanish,
    Italian,
    #[default]
    #[serde(other)]
    English,
}
impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Russian,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Italian,
    ];
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::Italian => "Italiano",
        }
    }
    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|language| *language == self)
            .unwrap_or(0)
    }
}
fn catalogs() -> &'static [BTreeMap<String, String>; 6] {
    static CATALOGS: OnceLock<[BTreeMap<String, String>; 6]> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        [
            include_str!("locales/en.json"),
            include_str!("locales/ru.json"),
            include_str!("locales/fr.json"),
            include_str!("locales/de.json"),
            include_str!("locales/es.json"),
            include_str!("locales/it.json"),
        ]
        .map(|catalog| serde_json::from_str(catalog).expect("validated embedded message catalog"))
    })
}
pub fn tr(language: Language, key: &str) -> &str {
    catalogs()[language.index()]
        .get(key)
        .or_else(|| catalogs()[0].get(key))
        .map_or(key, String::as_str)
}
pub fn format(language: Language, key: &str, arguments: &[(&str, String)]) -> String {
    // Substitute the template once; braces in user paths remain literal.
    let template = tr(language, key);
    let mut output = String::with_capacity(template.len());
    let mut remaining = template;
    while let Some(start) = remaining.find('{') {
        let Some(end) = remaining[start..].find('}') else {
            break;
        };
        output.push_str(&remaining[..start]);
        let name = &remaining[start + 1..start + end];
        if let Some((_, value)) = arguments.iter().find(|(argument, _)| *argument == name) {
            output.push_str(value);
        } else {
            output.push_str(&remaining[start..=start + end]);
        }
        remaining = &remaining[start + end + 1..];
    }
    output.push_str(remaining);
    output
}
#[derive(Clone, Debug)]
pub struct Message {
    key: String,
    arguments: Vec<(String, String)>,
    diagnostic: bool,
}
impl Message {
    pub fn key(key: &str) -> Self {
        Self {
            key: key.into(),
            arguments: Vec::new(),
            diagnostic: false,
        }
    }
    pub fn with(key: &str, name: &str, value: impl ToString) -> Self {
        Self {
            key: key.into(),
            arguments: vec![(name.into(), value.to_string())],
            diagnostic: false,
        }
    }
    pub fn render(&self, language: Language) -> String {
        if self.diagnostic {
            return self
                .key
                .split(": ")
                .map(|part| tr(language, part))
                .collect::<Vec<_>>()
                .join(": ");
        }
        format(
            language,
            &self.key,
            &self
                .arguments
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone()))
                .collect::<Vec<_>>(),
        )
    }
    pub fn contains(&self, value: &str) -> bool {
        self.key.contains(value)
    }
}
impl From<&str> for Message {
    fn from(key: &str) -> Self {
        Self::key(key)
    }
}
impl From<String> for Message {
    fn from(diagnostic: String) -> Self {
        Self {
            key: diagnostic,
            arguments: Vec::new(),
            diagnostic: true,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn placeholders(text: &str) -> Vec<&str> {
        let mut result: Vec<_> = text
            .split('{')
            .skip(1)
            .filter_map(|part| part.split_once('}').map(|(key, _)| key))
            .collect();
        result.sort_unstable();
        result
    }
    #[test]
    fn catalogs_cover_all_messages_and_preserve_parameters() {
        let english = &catalogs()[0];
        for language in Language::ALL {
            let catalog = &catalogs()[language.index()];
            assert_eq!(
                catalog.keys().collect::<Vec<_>>(),
                english.keys().collect::<Vec<_>>(),
                "{language:?}"
            );
            for (key, value) in catalog {
                assert!(!value.trim().is_empty(), "{language:?}: {key}");
                assert_eq!(
                    placeholders(key),
                    placeholders(value),
                    "{language:?}: {key}"
                );
            }
        }
    }
    #[test]
    fn cached_status_changes_language_without_translating_a_path() {
        let status = Message::with("Saved {path}", "path", "C:/Before/{version}/Settings.png");
        assert_eq!(
            status.render(Language::Russian),
            "Сохранено: C:/Before/{version}/Settings.png"
        );
        assert_eq!(
            status.render(Language::French),
            "Enregistré : C:/Before/{version}/Settings.png"
        );
        assert_eq!(
            tr(Language::German, "Future English fallback"),
            "Future English fallback"
        );
    }
}
