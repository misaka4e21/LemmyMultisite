pub mod admin_allow_instance;
pub mod admin_block_instance;
pub mod admin_change_username;
pub mod admin_list_users;
pub mod federated_instances;
pub mod list_all_media;
pub mod mod_log;
pub mod purge;
pub mod registration_applications;

#[cfg(test)]
mod configuration_tenant_tests;
#[cfg(test)]
mod oauth_tenant_tests;
#[cfg(test)]
mod local_name_tests;
