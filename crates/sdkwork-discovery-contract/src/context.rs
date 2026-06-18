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
    pub tenant_id: Option<String>,
    pub organization_id: Option<String>,
    registry_permissions: Vec<RegistryPermission>,
    config_permissions: Vec<ConfigPermission>,
}

impl CallerContext {
    pub fn new(subject_id: impl Into<String>) -> Self {
        Self {
            subject_id: subject_id.into(),
            tenant_id: None,
            organization_id: None,
            registry_permissions: Vec::new(),
            config_permissions: Vec::new(),
        }
    }

    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        let tenant_id = tenant_id.into();
        if !tenant_id.trim().is_empty() {
            self.tenant_id = Some(tenant_id);
        }
        self
    }

    pub fn with_organization_id(mut self, organization_id: impl Into<String>) -> Self {
        let organization_id = organization_id.into();
        if !organization_id.trim().is_empty() {
            self.organization_id = Some(organization_id);
        }
        self
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
