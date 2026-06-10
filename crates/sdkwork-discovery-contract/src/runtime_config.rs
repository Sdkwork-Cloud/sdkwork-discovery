#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEnvironment {
    Development,
    Test,
    Staging,
    Production,
}

impl RuntimeEnvironment {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "development" => Some(Self::Development),
            "test" => Some(Self::Test),
            "staging" => Some(Self::Staging),
            "production" => Some(Self::Production),
            _ => None,
        }
    }

    pub fn from_profile(profile: &str) -> Option<Self> {
        match profile {
            "dev" => Some(Self::Development),
            "test" => Some(Self::Test),
            "staging" => Some(Self::Staging),
            "prod" => Some(Self::Production),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Test => "test",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeDeploymentMode {
    Server,
    Container,
    Local,
    Test,
}

impl RuntimeDeploymentMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "server" => Some(Self::Server),
            "container" => Some(Self::Container),
            "local" => Some(Self::Local),
            "test" => Some(Self::Test),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTarget {
    Server,
    Container,
    TestRunner,
}

impl RuntimeTarget {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "server" => Some(Self::Server),
            "container" => Some(Self::Container),
            "test-runner" => Some(Self::TestRunner),
            _ => None,
        }
    }
}
