#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryPermission {
    Read,
    Write,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPermission {
    Read,
    Write,
    Publish,
    Rollback,
    Admin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerContext {
    pub subject_id: String,
    registry_permissions: Vec<RegistryPermission>,
    config_permissions: Vec<ConfigPermission>,
}

impl CallerContext {
    pub fn new(subject_id: impl Into<String>) -> Self {
        Self {
            subject_id: subject_id.into(),
            registry_permissions: Vec::new(),
            config_permissions: Vec::new(),
        }
    }

    pub fn with_registry_permission(mut self, permission: RegistryPermission) -> Self {
        if !self.registry_permissions.contains(&permission) {
            self.registry_permissions.push(permission);
        }
        self
    }

    pub fn with_config_permission(mut self, permission: ConfigPermission) -> Self {
        if !self.config_permissions.contains(&permission) {
            self.config_permissions.push(permission);
        }
        self
    }

    pub fn has_registry_permission(&self, required: RegistryPermission) -> bool {
        self.registry_permissions
            .contains(&RegistryPermission::Admin)
            || self.registry_permissions.contains(&required)
    }

    pub fn has_config_permission(&self, required: ConfigPermission) -> bool {
        self.config_permissions.contains(&ConfigPermission::Admin)
            || self.config_permissions.contains(&required)
    }
}
