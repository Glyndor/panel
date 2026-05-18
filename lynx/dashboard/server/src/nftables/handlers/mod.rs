mod global;
mod local;

pub use global::{create_global_rule, delete_global_rule, list_global_rules, push_global_rules};
pub use local::{create_local_rule, delete_local_rule, list_local_rules, push_local_rules};
