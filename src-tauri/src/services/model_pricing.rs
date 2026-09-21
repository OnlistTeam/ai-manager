use crate::config::atomic_write;
use crate::database::{lock_conn, Database};
use crate::error::AppError;
use rusqlite::{params, Transaction};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

const MODEL_PRICING_FILE_NAME: &str = "model-pricing.json";
const MODEL_PRICING_FILE_VERSION: u32 = 1;

static MODEL_PRICING_FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn file_lock() -> &'static Mutex<()> {
    MODEL_PRICING_FILE_LOCK.get_or_init(|| Mutex::new(()))
}

fn default_true() -> bool {
    true
}

fn default_file_version() -> u32 {
    MODEL_PRICING_FILE_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPricingInfo {
    pub model_id: String,
    pub display_name: String,
    pub input_cost_per_million: String,
    pub output_cost_per_million: String,
    pub cache_read_cost_per_million: String,
    pub cache_creation_cost_per_million: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDevSyncConfig {
    #[serde(default)]
    pub auto_sync_enabled: bool,
    #[serde(default = "default_true")]
    pub include_common_models: bool,
    #[serde(default)]
    pub selected_model_keys: Vec<String>,
    #[serde(default)]
    pub excluded_common_model_keys: Vec<String>,
    #[serde(default)]
    pub last_sync_at: Option<i64>,
    #[serde(default)]
    pub last_sync_error: Option<String>,
}

impl Default for ModelsDevSyncConfig {
    fn default() -> Self {
        Self {
            auto_sync_enabled: false,
            include_common_models: true,
            selected_model_keys: Vec::new(),
            excluded_common_model_keys: Vec::new(),
            last_sync_at: None,
            last_sync_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelPricingFile {
    #[serde(default = "default_file_version")]
    version: u32,
    #[serde(default)]
    models_dev_sync: ModelsDevSyncConfig,
    #[serde(default)]
    models: Vec<ModelPricingInfo>,
    #[serde(default)]
    deleted_model_ids: Vec<String>,
}

impl Default for ModelPricingFile {
    fn default() -> Self {
        Self {
            version: MODEL_PRICING_FILE_VERSION,
            models_dev_sync: ModelsDevSyncConfig::default(),
            models: Vec::new(),
            deleted_model_ids: Vec::new(),
        }
    }
}

pub fn model_pricing_file_path() -> PathBuf {
    crate::infrastructure::paths::product_data_dir().join(MODEL_PRICING_FILE_NAME)
}

fn normalize_decimal(label: &str, value: &str) -> Result<String, AppError> {
    let value = value.trim();
    let parsed = Decimal::from_str(value).map_err(|error| {
        AppError::localized(
            "usage.invalidPrice",
            format!("{label} price is invalid: {value} - {error}"),
        )
    })?;
    if parsed < Decimal::ZERO {
        return Err(AppError::localized(
            "usage.invalidPrice",
            format!("{label} price must be non-negative: {value}"),
        ));
    }
    Ok(value.to_string())
}

fn normalize_pricing(entry: ModelPricingInfo) -> Result<ModelPricingInfo, AppError> {
    let model_id = entry.model_id.trim().to_string();
    let display_name = entry.display_name.trim().to_string();
    if model_id.is_empty() {
        return Err(AppError::localized(
            "usage.modelIdRequired",
            "Model ID is required",
        ));
    }
    if display_name.is_empty() {
        return Err(AppError::localized(
            "usage.displayNameRequired",
            "Display name is required",
        ));
    }

    Ok(ModelPricingInfo {
        model_id,
        display_name,
        input_cost_per_million: normalize_decimal("input_cost", &entry.input_cost_per_million)?,
        output_cost_per_million: normalize_decimal("output_cost", &entry.output_cost_per_million)?,
        cache_read_cost_per_million: normalize_decimal(
            "cache_read_cost",
            &entry.cache_read_cost_per_million,
        )?,
        cache_creation_cost_per_million: normalize_decimal(
            "cache_creation_cost",
            &entry.cache_creation_cost_per_million,
        )?,
    })
}

fn normalize_key_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_sync_config(mut config: ModelsDevSyncConfig) -> ModelsDevSyncConfig {
    config.selected_model_keys = normalize_key_list(config.selected_model_keys);
    config.excluded_common_model_keys = normalize_key_list(config.excluded_common_model_keys);
    config.last_sync_error = config.last_sync_error.and_then(|error| {
        let trimmed = error.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.chars().take(1000).collect())
        }
    });
    config
}

fn normalize_file(mut file: ModelPricingFile) -> Result<ModelPricingFile, AppError> {
    if file.version > MODEL_PRICING_FILE_VERSION {
        return Err(AppError::Config(format!(
            "model-pricing.json version {} is newer than supported version {}",
            file.version, MODEL_PRICING_FILE_VERSION
        )));
    }

    let deleted = normalize_key_list(file.deleted_model_ids)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut models = BTreeMap::new();
    for entry in file.models {
        let entry = normalize_pricing(entry)?;
        if !deleted.contains(&entry.model_id) {
            models.insert(entry.model_id.clone(), entry);
        }
    }

    file.version = MODEL_PRICING_FILE_VERSION;
    file.models_dev_sync = normalize_sync_config(file.models_dev_sync);
    file.models = models.into_values().collect();
    file.deleted_model_ids = deleted.into_iter().collect();
    Ok(file)
}

fn read_file_unlocked() -> Result<Option<ModelPricingFile>, AppError> {
    let path = model_pricing_file_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|error| AppError::io(&path, error))?;
    let file = serde_json::from_str(&content).map_err(|error| AppError::json(&path, error))?;
    normalize_file(file).map(Some)
}

fn write_file_unlocked(file: &ModelPricingFile) -> Result<(), AppError> {
    let path = model_pricing_file_path();
    let mut data = serde_json::to_vec_pretty(file).map_err(|error| {
        AppError::Config(format!("Failed to serialize model pricing config: {error}"))
    })?;
    data.push(b'\n');
    atomic_write(&path, &data)
}

fn load_or_create_file_unlocked() -> Result<ModelPricingFile, AppError> {
    if let Some(file) = read_file_unlocked()? {
        return Ok(file);
    }

    // The local file stores user/models.dev overrides only. Exporting the
    // complete seeded table here would turn built-in prices into overrides and
    // roll back future repair_current_model_pricing corrections on startup.
    let file = ModelPricingFile::default();
    write_file_unlocked(&file)?;
    Ok(file)
}

fn upsert_pricing(
    transaction: &Transaction<'_>,
    entry: &ModelPricingInfo,
) -> Result<usize, AppError> {
    transaction
        .execute(
            "INSERT INTO model_pricing (
                model_id, display_name, input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(model_id) DO UPDATE SET
                display_name = excluded.display_name,
                input_cost_per_million = excluded.input_cost_per_million,
                output_cost_per_million = excluded.output_cost_per_million,
                cache_read_cost_per_million = excluded.cache_read_cost_per_million,
                cache_creation_cost_per_million = excluded.cache_creation_cost_per_million
            WHERE display_name <> excluded.display_name
               OR input_cost_per_million <> excluded.input_cost_per_million
               OR output_cost_per_million <> excluded.output_cost_per_million
               OR cache_read_cost_per_million <> excluded.cache_read_cost_per_million
               OR cache_creation_cost_per_million <> excluded.cache_creation_cost_per_million",
            params![
                entry.model_id,
                entry.display_name,
                entry.input_cost_per_million,
                entry.output_cost_per_million,
                entry.cache_read_cost_per_million,
                entry.cache_creation_cost_per_million
            ],
        )
        .map_err(|error| AppError::Database(format!("Failed to update model pricing: {error}")))
}

fn apply_file_to_database(
    db: &Database,
    file: &ModelPricingFile,
) -> Result<(usize, usize), AppError> {
    let mut conn = lock_conn!(db.conn);
    let transaction = conn.transaction()?;
    let mut upserted = 0;
    for entry in &file.models {
        upserted += upsert_pricing(&transaction, entry)?;
    }
    let mut deleted = 0;
    for model_id in &file.deleted_model_ids {
        deleted += transaction.execute(
            "DELETE FROM model_pricing WHERE model_id = ?1",
            params![model_id],
        )?;
    }
    transaction.commit()?;
    Ok((upserted, deleted))
}

/// Load user-maintained overrides from product AppData/model-pricing.json.
/// Built-in rows remain database-owned so application updates can repair them;
/// the file contains only explicit overrides and deletion tombstones.
pub fn sync_local_model_pricing(db: &Database) -> Result<usize, AppError> {
    let (upserted, deleted) = {
        let _file_guard = file_lock().lock().map_err(|error| {
            AppError::Config(format!("Failed to lock model pricing file: {error}"))
        })?;
        let file = load_or_create_file_unlocked()?;
        apply_file_to_database(db, &file)?
    };

    // Deleting pricing cannot make a zero-cost usage row calculable. In
    // particular, seeded rows covered by tombstones may be reinserted and
    // deleted on every startup; they must not trigger a full-table backfill.
    if upserted > 0 {
        if let Err(error) = db.backfill_missing_usage_costs() {
            log::warn!(
                "Failed to backfill historical usage cost after local model pricing sync: {error}"
            );
        }
    }
    Ok(upserted + deleted)
}

#[cfg(test)]
mod tests {}
