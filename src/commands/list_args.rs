use std::collections::HashSet;

use serde_json::json;

/// Shared arguments for list commands providing bounded output support.
#[derive(clap::Args, Clone, Debug)]
pub struct ListArgs {
    /// Maximum number of items to return
    #[arg(long)]
    pub limit: Option<usize>,

    /// Number of items to skip
    #[arg(long, default_value = "0")]
    pub offset: usize,

    /// Comma-separated list of fields to include in JSON output
    #[arg(long)]
    pub fields: Option<String>,
}

impl ListArgs {
    /// Apply offset and limit to a slice, returning a paginated subset.
    pub fn paginate<'a, T>(&self, items: &'a [T]) -> &'a [T] {
        let start = self.offset.min(items.len());
        let remaining = &items[start..];
        match self.limit {
            Some(limit) => &remaining[..limit.min(remaining.len())],
            None => remaining,
        }
    }

    /// Filter JSON values to only include specified fields.
    /// Returns items unchanged if no fields filter is set.
    pub fn filter_fields(&self, items: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        let fields = match &self.fields {
            Some(f) => f,
            None => return items,
        };

        let field_set: HashSet<&str> = fields.split(',').map(|f| f.trim()).collect();
        items
            .into_iter()
            .map(|item| {
                if let Some(obj) = item.as_object() {
                    let filtered: serde_json::Map<String, serde_json::Value> = obj
                        .iter()
                        .filter(|(k, _)| field_set.contains(k.as_str()))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    serde_json::Value::Object(filtered)
                } else {
                    item
                }
            })
            .collect()
    }

    /// Wrap items in a pagination envelope for JSON output.
    pub fn paginated_json(&self, items: &[serde_json::Value], total: usize) -> serde_json::Value {
        json!({
            "items": items,
            "total": total,
            "offset": self.offset,
            "limit": self.limit,
        })
    }
}
