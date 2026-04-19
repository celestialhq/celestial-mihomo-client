//! Some config file template

/// template for new a profile item
pub const ITEM_LOCAL: &str = "# Profile Template for Celestial

proxies: []

proxy-groups: []

rules: []
";

/// enhanced profile
pub const ITEM_MERGE: &str = "# Profile Enhancement Merge Template for Celestial

profile:
  store-selected: true
";

pub const ITEM_MERGE_EMPTY: &str = "# Profile Enhancement Merge Template for Celestial

";

/// enhanced profile
pub const ITEM_SCRIPT: &str = "// Define main function (script entry)

function main(config, profileName) {
  return config;
}
";

/// enhanced profile
pub const ITEM_RULES: &str = "# Profile Enhancement Rules Template for Celestial

prepend: []

append: []

delete: []
";

/// enhanced profile
pub const ITEM_PROXIES: &str = "# Profile Enhancement Proxies Template for Celestial

prepend: []

append: []

delete: []
";

/// enhanced profile
pub const ITEM_GROUPS: &str = "# Profile Enhancement Groups Template for Celestial

prepend: []

append: []

delete: []
";
