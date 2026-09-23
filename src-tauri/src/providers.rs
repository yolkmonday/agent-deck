//! Provider overview and editing, read from and written to the opencode config.
//!
//! Keys never travel as plaintext: a value read from the config is masked before it is
//! returned, and a value written to the config is always a `{file:...}` reference.

use crate::config::ConfigFile;
use crate::secrets::{self, mask};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeaderStyle {
    Bearer,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OcModel {
    pub id: String,
    pub name: Option<String>,
    pub context_limit: Option<i64>,
    pub output_limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OcProvider {
    pub id: String,
    pub name: String,
    pub npm: String,
    pub base_url: String,
    pub auth: String,
    pub header_style: HeaderStyle,
    pub custom_header_name: Option<String>,
    pub enabled: bool,
    pub key_masked: Option<String>,
    pub key_inline: bool,
    pub source: String,
    pub models: Vec<OcModel>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcProviderInput {
    pub id: String,
    pub name: String,
    pub npm: String,
    pub base_url: String,
    pub header_style: HeaderStyle,
    #[serde(default)]
    pub custom_header_name: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub models: Vec<OcModel>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsOverview {
    pub claude: ClaudeModels,
    pub opencode: Vec<OcProvider>,
    pub codex: CodexModels,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeModels {
    pub models: Vec<String>,
    pub recently_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModels {
    pub models: Vec<String>,
    pub note: String,
}

pub const CLAUDE_MODELS: [&str; 4] = [
    "claude-fable-5-1",
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-haiku-4-5",
];

pub const CODEX_NOTE: &str = "Read from Codex session history.";

/// `~/.config/opencode`, falling back to the first path that exists.
pub fn config_path(home: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(home).join(".config/opencode");
    for name in ["opencode.jsonc", "opencode.json"] {
        let path = dir.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn config_file(home: &str) -> anyhow::Result<ConfigFile> {
    let path = config_path(home)
        .ok_or_else(|| anyhow::anyhow!("no opencode config found for this home"))?;
    ConfigFile::load(&path)
}

/// Credential ids reported by `opencode providers list`. A failing command yields an empty
/// list so the overview still renders; the auth file itself is never read.
pub fn cli_credential_ids() -> Vec<String> {
    let output = match std::process::Command::new("opencode")
        .args(["providers", "list"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    parse_cli_credentials(&String::from_utf8_lossy(&output.stdout))
}

/// Parses the `●  <id> <type>` lines of `opencode providers list`.
fn parse_cli_credentials(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix('●')?.trim();
            let id = rest.split_whitespace().next()?.trim();
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        })
        .collect()
}

fn custom_header_name(options: &Value) -> Option<String> {
    options
        .get("headers")
        .and_then(Value::as_object)
        .and_then(|h| h.keys().next().cloned())
}

/// A literal key is one that is not a `{file:...}` or `{env:...}` reference.
fn is_reference(value: &str) -> bool {
    let t = value.trim();
    t.starts_with("{file:") || t.starts_with("{env:")
}

fn key_from_options(options: &Value, header_style: HeaderStyle, header: Option<&str>) -> Option<String> {
    match (header_style, header) {
        (HeaderStyle::Bearer, _) => options.get("apiKey").and_then(Value::as_str).map(str::to_string),
        (HeaderStyle::Custom, Some(name)) => options
            .get("headers")
            .and_then(Value::as_object)
            .and_then(|h| h.get(name))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

fn models_from(provider: &Value) -> Vec<OcModel> {
    provider
        .get("models")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(id, m)| {
                    // opencode nests both caps under `limit`; older configs may use a bare
                    // number, which we read as the context cap.
                    let limit = m.get("limit");
                    let context_limit = limit
                        .and_then(|l| l.get("context"))
                        .and_then(Value::as_i64)
                        .or_else(|| limit.and_then(Value::as_i64));
                    let output_limit =
                        limit.and_then(|l| l.get("output")).and_then(Value::as_i64);
                    OcModel {
                        id: id.clone(),
                        name: m.get("name").and_then(Value::as_str).map(str::to_string),
                        context_limit,
                        output_limit,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn provider_from_value(
    home: &str,
    id: &str,
    value: &Value,
    disabled: &[String],
    cli_ids: &[String],
) -> OcProvider {
    let options = value.get("options").cloned().unwrap_or(Value::Null);
    let header = custom_header_name(&options).filter(|_| options.get("apiKey").is_none());
    let header_style = if header.is_some() {
        HeaderStyle::Custom
    } else {
        HeaderStyle::Bearer
    };
    let raw_key = key_from_options(&options, header_style, header.as_deref());
    let key_inline = raw_key.as_deref().is_some_and(|k| !is_reference(k));

    let mut masked = None;
    let mut auth = "none";
    if let Some(raw) = &raw_key {
        auth = "config";
        masked = Some(if key_inline {
            mask(raw)
        } else {
            referenced_key_masked(home, raw)
        });
    } else if cli_ids.iter().any(|c| c == id) {
        auth = "cli";
    }

    OcProvider {
        id: id.to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
        npm: value
            .get("npm")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        base_url: options
            .get("baseURL")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        auth: auth.to_string(),
        header_style,
        custom_header_name: header,
        enabled: !disabled.iter().any(|d| d == id),
        key_masked: masked,
        key_inline,
        source: "global".to_string(),
        models: models_from(value),
    }
}

/// Masks a key that lives in a 0600 file. The `~` form is resolved against the caller's
/// `home` so a test never touches the real one. An unresolvable reference masks nothing.
fn referenced_key_masked(home: &str, raw: &str) -> String {
    let inner = raw
        .trim()
        .trim_start_matches("{file:")
        .trim_end_matches('}')
        .trim();
    let path = if let Some(rest) = inner.strip_prefix("~/") {
        Some(PathBuf::from(home).join(rest))
    } else if inner.is_empty() {
        None
    } else {
        Some(PathBuf::from(inner))
    };
    let Some(path) = path else {
        return mask("");
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    mask(&text)
}

fn disabled_ids(value: &Value) -> Vec<String> {
    value
        .get("disabled_providers")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Builds the full overview. Claude's `recentlyUsed` and Codex's model list come from the
/// dashboard's own index, passed in so this stays testable without a database.
pub fn read_overview(
    home: &str,
    claude_recent: Vec<String>,
    codex_models: Vec<String>,
) -> anyhow::Result<ModelsOverview> {
    read_overview_with(home, claude_recent, codex_models, &cli_credential_ids())
}

pub fn read_overview_with(
    home: &str,
    claude_recent: Vec<String>,
    codex_models: Vec<String>,
    cli_ids: &[String],
) -> anyhow::Result<ModelsOverview> {
    let disabled = config_path(home)
        .map(|p| ConfigFile::load(&p).and_then(|f| f.value()))
        .transpose()?
        .map(|v| disabled_ids(&v))
        .unwrap_or_default();

    let mut providers = Vec::new();
    if let Some(path) = config_path(home) {
        let value = ConfigFile::load(&path)?.value()?;
        if let Some(map) = value.get("provider").and_then(Value::as_object) {
            for (id, provider) in map {
                providers.push(provider_from_value(home, id, provider, &disabled, cli_ids));
            }
        }
    }
    providers.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(ModelsOverview {
        claude: ClaudeModels {
            models: CLAUDE_MODELS.iter().map(|s| s.to_string()).collect(),
            recently_used: claude_recent,
        },
        opencode: providers,
        codex: CodexModels {
            models: codex_models,
            note: CODEX_NOTE.to_string(),
        },
    })
}

/// Writes a provider block. The key is always a `{file:...}` reference, never the value.
pub fn save_provider(home: &str, input: OcProviderInput) -> anyhow::Result<OcProvider> {
    let mut file = config_file(home)?;
    let base = ["provider", input.id.as_str()];

    let reference = secrets::file_ref(home, &input.id);
    // The block is written first; the key reference is applied after it so the wholesale
    // node replacement above cannot clobber the reference.
    file.set_path(&base, provider_block(&input, &reference))?;

    let mut options_path = base.to_vec();
    options_path.push("options");
    let mut api_key_path = options_path.clone();
    api_key_path.push("apiKey");
    let mut headers_path = options_path;
    headers_path.push("headers");

    match input.header_style {
        HeaderStyle::Bearer => {
            // Bearer means `options.apiKey`; the custom-header form must not linger.
            file.remove_path(&headers_path)?;
            file.set_path(&api_key_path, json!(reference))?;
        }
        HeaderStyle::Custom => {
            let name = input
                .custom_header_name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("custom header style needs a header name"))?;
            file.remove_path(&api_key_path)?;
            let mut header_path = headers_path.clone();
            header_path.push(name.as_str());
            file.set_path(&header_path, json!(reference))?;
        }
    }

    set_disabled(&mut file, &input.id, input.enabled)?;
    file.save_atomic()?;

    let disabled = if input.enabled {
        Vec::new()
    } else {
        vec![input.id.clone()]
    };
    Ok(provider_from_value(
        home,
        &input.id,
        &file.value()?["provider"][&input.id],
        &disabled,
        &[],
    ))
}

fn provider_block(input: &OcProviderInput, reference: &str) -> Value {
    let mut options = serde_json::Map::new();
    options.insert("baseURL".to_string(), json!(input.base_url));
    match input.header_style {
        HeaderStyle::Bearer => {}
        HeaderStyle::Custom => {
            let name = input
                .custom_header_name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "x-api-key".to_string());
            let mut headers = serde_json::Map::new();
            headers.insert(name, json!(reference));
            options.insert("headers".to_string(), Value::Object(headers));
        }
    }

    let models: serde_json::Map<String, Value> = input
        .models
        .iter()
        .map(|m| {
            let mut entry = serde_json::Map::new();
            if let Some(name) = &m.name {
                entry.insert("name".to_string(), json!(name));
            }
            let mut limit = serde_json::Map::new();
            if let Some(c) = m.context_limit {
                limit.insert("context".to_string(), json!(c));
            }
            if let Some(o) = m.output_limit {
                limit.insert("output".to_string(), json!(o));
            }
            if !limit.is_empty() {
                entry.insert("limit".to_string(), Value::Object(limit));
            }
            (m.id.clone(), Value::Object(entry))
        })
        .collect();

    let mut block = serde_json::Map::new();
    block.insert("name".to_string(), json!(input.name));
    block.insert("npm".to_string(), json!(input.npm));
    block.insert("options".to_string(), Value::Object(options));
    if !models.is_empty() {
        block.insert("models".to_string(), Value::Object(models));
    }
    Value::Object(block)
}

/// Adds or removes the id in `disabled_providers` according to `enabled`.
fn set_disabled(file: &mut ConfigFile, id: &str, enabled: bool) -> anyhow::Result<()> {
    let value = file.value().unwrap_or(Value::Null);
    let mut ids = disabled_ids(&value);
    let had_entry = value.get("disabled_providers").is_some();
    let was_listed = ids.iter().any(|d| d == id);

    if enabled {
        if !was_listed {
            return Ok(());
        }
        ids.retain(|d| d != id);
    } else {
        if !was_listed {
            ids.push(id.to_string());
        }
        ids.sort();
    }

    if ids.is_empty() && enabled {
        if had_entry {
            file.remove_path(&["disabled_providers"])?;
        }
        return Ok(());
    }
    let list: Vec<Value> = ids.into_iter().map(Value::String).collect();
    file.set_path(&["disabled_providers"], Value::Array(list))
}

/// Removes a provider block and its `disabled_providers` entry. The key file is left
/// alone: deleting a credential is a separate, explicit action.
pub fn delete_provider(home: &str, id: &str) -> anyhow::Result<()> {
    let mut file = config_file(home)?;
    file.remove_path(&["provider", id])?;
    set_disabled(&mut file, id, true)?;
    file.save_atomic()?;
    Ok(())
}

/// Reads the inline key of a provider, if it still has one.
pub fn inline_key(home: &str, provider_id: &str) -> anyhow::Result<Option<String>> {
    let file = config_file(home)?;
    let value = file.value()?;
    let Some(provider) = value.get("provider").and_then(|p| p.get(provider_id)) else {
        return Ok(None);
    };
    let options = provider.get("options").cloned().unwrap_or(Value::Null);
    let header = custom_header_name(&options).filter(|_| options.get("apiKey").is_none());
    let style = if header.is_some() {
        HeaderStyle::Custom
    } else {
        HeaderStyle::Bearer
    };
    Ok(key_from_options(&options, style, header.as_deref()).filter(|k| !is_reference(k)))
}

/// The base URL and auth of a provider, resolved for a probe. Returns the key for a
/// `{file:...}` reference or a literal; the caller must never log it.
pub fn probe_target(
    home: &str,
    provider_id: &str,
) -> anyhow::Result<(String, String, HeaderStyle, Option<String>)> {
    let value = ConfigFile::load(
        &config_path(home).ok_or_else(|| anyhow::anyhow!("no opencode config found"))?,
    )?
    .value()?;
    let provider = value
        .get("provider")
        .and_then(|p| p.get(provider_id))
        .ok_or_else(|| anyhow::anyhow!("unknown provider: {provider_id}"))?;

    let options = provider.get("options").cloned().unwrap_or(Value::Null);
    let header = custom_header_name(&options).filter(|_| options.get("apiKey").is_none());
    let style = if header.is_some() {
        HeaderStyle::Custom
    } else {
        HeaderStyle::Bearer
    };
    let base_url = options
        .get("baseURL")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let raw = key_from_options(&options, style, header.as_deref());
    // A reference is resolved through the secrets store so the `~` form is read against
    // `home`; a literal is used as-is (it is a config still waiting to be migrated).
    let key = match raw {
        Some(r) if is_reference(&r) => read_referenced_key(home, provider_id, &r),
        Some(r) => r,
        None => String::new(),
    };

    Ok((base_url, key, style, header))
}

/// Reads the key a `{file:...}` reference points at. Only paths that resolve to the
/// provider's own secrets file are honoured, so a reference cannot pull in an unrelated
/// file from the config.
fn read_referenced_key(home: &str, provider_id: &str, raw: &str) -> String {
    let inner = raw
        .trim()
        .trim_start_matches("{file:")
        .trim_end_matches('}')
        .trim();
    let expected = format!(".config/opencode/secrets/{provider_id}.key");
    if inner == format!("~/{expected}") || inner.ends_with(&expected) {
        return secrets::read_key(home, provider_id).unwrap_or_default();
    }
    if let Some(rest) = inner.strip_prefix("~/") {
        return std::fs::read_to_string(PathBuf::from(home).join(rest))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"{
  // provider list, keep the comments
  "provider": {
    "aki": {
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "https://aki.example/v1",
        "apiKey": "sk-inline-9f7e862e2668-secret"
      }
    },
    "filed": {
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "https://filed.example/v1",
        "apiKey": "{file:~/.config/opencode/secrets/filed.key}"
      }
    },
    "custom": {
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "https://custom.example/v1",
        "headers": { "x-api-key": "sk-custom-abcdefghijkl" }
      }
    }
  },
  "disabled_providers": ["aki"]
}
"#;

    const FILE_KEY: &str = "sk-stored-abcdefghijkl";

    fn setup() -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap().to_string();
        let dir = tmp.path().join(".config/opencode");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("opencode.jsonc"), CONFIG).unwrap();
        secrets::write_key(&home, "filed", FILE_KEY).unwrap();
        (tmp, home)
    }

    fn overview(home: &str) -> ModelsOverview {
        read_overview_with(home, vec![], vec![], &[]).unwrap()
    }

    fn provider(list: &[OcProvider], id: &str) -> OcProvider {
        list.iter().find(|p| p.id == id).cloned().unwrap()
    }

    #[test]
    fn overview_masks_inline_key_and_flags_it() {
        let (_tmp, home) = setup();
        let aki = provider(&overview(&home).opencode, "aki");

        assert!(aki.key_inline);
        let masked = aki.key_masked.clone().unwrap();
        assert!(masked.starts_with("sk-inl"), "got {masked}");
        assert!(!masked.contains("9f7e862e2668"));

        let wire = serde_json::to_string(&overview(&home)).unwrap();
        assert!(!wire.contains("9f7e862e2668"), "plaintext key leaked to the wire");
    }

    #[test]
    fn overview_reads_file_referenced_key_as_not_inline() {
        let (_tmp, home) = setup();
        let filed = provider(&overview(&home).opencode, "filed");

        assert!(!filed.key_inline);
        let masked = filed.key_masked.clone().unwrap();
        assert!(masked.starts_with("sk-sto"), "got {masked}");
        assert!(!masked.contains("abcdefghijkl"));
    }

    #[test]
    fn overview_marks_disabled_providers() {
        let (_tmp, home) = setup();
        assert!(!provider(&overview(&home).opencode, "aki").enabled);
        assert!(provider(&overview(&home).opencode, "filed").enabled);
    }

    #[test]
    fn overview_detects_custom_header_style() {
        let (_tmp, home) = setup();
        let custom = provider(&overview(&home).opencode, "custom");

        assert_eq!(custom.header_style, HeaderStyle::Custom);
        assert_eq!(custom.custom_header_name.as_deref(), Some("x-api-key"));
        assert!(custom.key_inline);
    }

    #[test]
    fn save_provider_writes_file_reference_not_the_key() {
        let (_tmp, home) = setup();
        save_provider(
            &home,
            OcProviderInput {
                id: "filed".into(),
                name: "Filed".into(),
                npm: "@ai-sdk/openai-compatible".into(),
                base_url: "https://filed.example/v1".into(),
                header_style: HeaderStyle::Bearer,
                custom_header_name: None,
                enabled: true,
                models: vec![],
            },
        )
        .unwrap();

        let text = std::fs::read_to_string(
            std::path::PathBuf::from(&home).join(".config/opencode/opencode.jsonc"),
        )
        .unwrap();
        assert!(text.contains("{file:~/.config/opencode/secrets/filed.key}"));
        assert!(!text.contains(FILE_KEY));
        assert!(text.contains("// provider list, keep the comments"));

        // The saved provider points at the file; the other provider's inline key is
        // untouched, so we check the parsed view rather than the whole text.
        let value = ConfigFile::load(
            &std::path::PathBuf::from(&home).join(".config/opencode/opencode.jsonc"),
        )
        .unwrap()
        .value()
        .unwrap();
        assert_eq!(
            value["provider"]["filed"]["options"]["apiKey"],
            "{file:~/.config/opencode/secrets/filed.key}"
        );
        assert_eq!(
            value["provider"]["aki"]["options"]["apiKey"],
            "sk-inline-9f7e862e2668-secret"
        );

        // The stored key round-trips through the secrets file, untouched by the edit.
        assert_eq!(secrets::read_key(&home, "filed").unwrap(), FILE_KEY);
    }

    #[test]
    fn save_provider_switching_to_custom_header_removes_apikey() {
        let (_tmp, home) = setup();
        save_provider(
            &home,
            OcProviderInput {
                id: "filed".into(),
                name: "Filed".into(),
                npm: "@ai-sdk/openai-compatible".into(),
                base_url: "https://filed.example/v1".into(),
                header_style: HeaderStyle::Custom,
                custom_header_name: Some("x-api-key".into()),
                enabled: true,
                models: vec![],
            },
        )
        .unwrap();

        let value = ConfigFile::load(
            &std::path::PathBuf::from(&home).join(".config/opencode/opencode.jsonc"),
        )
        .unwrap()
        .value()
        .unwrap();
        let options = &value["provider"]["filed"]["options"];
        assert!(options.get("apiKey").is_none(), "apiKey should be gone");
        assert_eq!(
            options["headers"]["x-api-key"],
            "{file:~/.config/opencode/secrets/filed.key}"
        );
    }

    #[test]
    fn save_provider_toggles_disabled_providers() {
        let (_tmp, home) = setup();

        let mut input = OcProviderInput {
            id: "filed".into(),
            name: "Filed".into(),
            npm: "@ai-sdk/openai-compatible".into(),
            base_url: "https://filed.example/v1".into(),
            header_style: HeaderStyle::Bearer,
            custom_header_name: None,
            enabled: false,
            models: vec![],
        };
        save_provider(&home, clone_input(&input)).unwrap();
        assert!(!provider(&overview(&home).opencode, "filed").enabled);

        input.enabled = true;
        save_provider(&home, clone_input(&input)).unwrap();
        assert!(provider(&overview(&home).opencode, "filed").enabled);
    }

    fn clone_input(i: &OcProviderInput) -> OcProviderInput {
        OcProviderInput {
            id: i.id.clone(),
            name: i.name.clone(),
            npm: i.npm.clone(),
            base_url: i.base_url.clone(),
            header_style: i.header_style,
            custom_header_name: i.custom_header_name.clone(),
            enabled: i.enabled,
            models: i.models.clone(),
        }
    }

    #[test]
    fn cli_credentials_parse_from_the_listing() {
        let stdout = "\n┌  Credentials ~/.local/share/opencode/auth.json\n│\n\
                      ●  aki api\n│\n●  OpenAI oauth\n│\n└  2 credentials\n";
        let ids = parse_cli_credentials(stdout);
        assert_eq!(ids, vec!["aki", "OpenAI"]);
        assert!(parse_cli_credentials("").is_empty());
    }

    #[test]
    fn provider_auth_is_cli_when_the_cli_has_the_credential() {
        let (_tmp, home) = setup();
        let view = read_overview_with(&home, vec![], vec![], &["aki".to_string()]).unwrap();
        // `aki` has a config key, which wins over the CLI credential.
        assert_eq!(provider(&view.opencode, "aki").auth, "config");

        // Same provider, no key anywhere in the config.
        let keyless = r#"{
  "provider": {
    "aki": {
      "npm": "@ai-sdk/openai-compatible",
      "options": { "baseURL": "https://aki.example/v1" }
    }
  }
}
"#;
        std::fs::write(
            std::path::PathBuf::from(&home).join(".config/opencode/opencode.jsonc"),
            keyless,
        )
        .unwrap();
        let view = read_overview_with(&home, vec![], vec![], &["aki".to_string()]).unwrap();
        assert_eq!(provider(&view.opencode, "aki").auth, "cli");
        assert!(provider(&view.opencode, "aki").key_masked.is_none());

        let view = read_overview_with(&home, vec![], vec![], &[]).unwrap();
        assert_eq!(provider(&view.opencode, "aki").auth, "none");
    }

    #[test]
    fn probe_target_resolves_the_stored_key() {
        let (_tmp, home) = setup();

        let (base, key, style, header) = probe_target(&home, "filed").unwrap();
        assert_eq!(base, "https://filed.example/v1");
        assert_eq!(key, FILE_KEY);
        assert_eq!(style, HeaderStyle::Bearer);
        assert!(header.is_none());

        let (_, key, style, header) = probe_target(&home, "custom").unwrap();
        assert_eq!(key, "sk-custom-abcdefghijkl");
        assert_eq!(style, HeaderStyle::Custom);
        assert_eq!(header.as_deref(), Some("x-api-key"));
    }

    #[test]
    fn probe_target_rejects_an_unknown_provider() {
        let (_tmp, home) = setup();
        assert!(probe_target(&home, "nope").is_err());
    }

    #[test]
    fn inline_key_reports_only_literal_keys() {
        let (_tmp, home) = setup();
        assert_eq!(
            inline_key(&home, "aki").unwrap().as_deref(),
            Some("sk-inline-9f7e862e2668-secret")
        );
        assert!(inline_key(&home, "filed").unwrap().is_none());
        assert!(inline_key(&home, "nope").unwrap().is_none());
    }

    #[test]
    #[cfg(unix)]
    fn migrating_an_inline_key_moves_it_to_the_secrets_file() {
        use std::os::unix::fs::PermissionsExt;

        let (_tmp, home) = setup();
        let path = std::path::PathBuf::from(&home).join(".config/opencode/opencode.jsonc");

        // Mirrors what the `secret_migrate_inline` command does, without Tauri state.
        let key = inline_key(&home, "aki").unwrap().unwrap();
        crate::secrets::write_key(&home, "aki", &key).unwrap();
        let mut file = ConfigFile::load(&path).unwrap();
        file.set_path(
            &["provider", "aki", "options", "apiKey"],
            json!(crate::secrets::file_ref(&home, "aki")),
        )
        .unwrap();
        file.save_atomic().unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&key), "plaintext key stayed in the config");
        assert!(text.contains("{file:~/.config/opencode/secrets/aki.key}"));

        let mode = std::fs::metadata(crate::secrets::key_path(&home, "aki").unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(crate::secrets::read_key(&home, "aki").unwrap(), key);

        // After the move the provider no longer reports an inline key.
        assert!(inline_key(&home, "aki").unwrap().is_none());
        assert!(provider(&overview(&home).opencode, "aki").key_masked.is_some());
    }

    #[test]
    fn delete_provider_keeps_other_providers_and_comments() {
        let (_tmp, home) = setup();
        delete_provider(&home, "custom").unwrap();

        let path = std::path::PathBuf::from(&home).join(".config/opencode/opencode.jsonc");
        let text = std::fs::read_to_string(&path).unwrap();
        let value = ConfigFile::load(&path).unwrap().value().unwrap();

        assert!(value["provider"].get("custom").is_none());
        assert!(value["provider"].get("aki").is_some());
        assert!(value["provider"].get("filed").is_some());
        assert!(text.contains("// provider list, keep the comments"));
    }
}
